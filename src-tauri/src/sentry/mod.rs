//! Sentry Phase 2 — foreground window capture (task 315).
//!
//! Every ~30 seconds while enabled, samples the OS's foreground window
//! (title + app name + PID) via active-win-pos-rs. Buffers events in
//! memory; every ~5 minutes flushes the batch to the cloud endpoint
//! `POST /me/telemetry/ingest`.
//!
//! Enablement is gated by a local meta flag `sentry.app_window.enabled`
//! (toggled from Settings). Cloud-side consent (from Sentry Phase 0)
//! is a second layer of protection — the ingest endpoint discards
//! events for kinds the user has revoked, so a stale local flag can't
//! leak data.
//!
//! Sample data captured per event:
//!   - captured_at (UTC ISO)
//!   - app_name (e.g. "Google Chrome", "Slack")
//!   - window_title (e.g. "Inbox – Gmail")
//!   - pid
//!
//! NOT captured:
//!   - Screen contents (Sentry Phase 3+, gated by 'screen' consent)
//!   - Text inside the window (never — 'content' consent gates any
//!     future capture)
//!   - Mouse position, keystrokes (out of scope)

pub mod cmd;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task;
use tokio::time::{sleep, Duration};

use crate::cloud::{read_jwt, CLOUD_BASE};

const SAMPLE_INTERVAL_SECS: u64 = 30;
const FLUSH_INTERVAL_SECS: u64 = 5 * 60;
const MAX_BUFFER: usize = 400;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppWindowEvent {
    pub captured_at: String,
    pub app_name: String,
    pub window_title: String,
    pub pid: i64,
}

/// State machine — Sentry can be running or stopped globally.
pub struct SentryState {
    buffer: Arc<Mutex<Vec<AppWindowEvent>>>,
    handle: Option<tokio::task::JoinHandle<()>>,
    http: reqwest::Client,
}

impl SentryState {
    pub fn new(http: reqwest::Client) -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            handle: None,
            http,
        }
    }

    /// Spawn the sampling loop. Idempotent — no-op if already running.
    pub fn start(&mut self) {
        if self.handle.is_some() {
            return;
        }
        let buffer = self.buffer.clone();
        let http = self.http.clone();
        let handle = task::spawn(async move {
            let mut ticks_since_flush = 0u64;
            let flush_every_ticks = FLUSH_INTERVAL_SECS / SAMPLE_INTERVAL_SECS;
            loop {
                sleep(Duration::from_secs(SAMPLE_INTERVAL_SECS)).await;
                if let Some(event) = sample_now() {
                    let mut buf = buffer.lock().await;
                    if buf.len() < MAX_BUFFER {
                        buf.push(event);
                    }
                }
                ticks_since_flush += 1;
                if ticks_since_flush >= flush_every_ticks {
                    ticks_since_flush = 0;
                    let mut buf = buffer.lock().await;
                    if !buf.is_empty() {
                        let batch: Vec<AppWindowEvent> = buf.drain(..).collect();
                        drop(buf);
                        if let Err(e) = flush_batch(&http, &batch).await {
                            tracing::warn!("sentry: flush failed: {e}");
                            // Re-buffer up to MAX_BUFFER so we don't lose
                            // data over a transient network error.
                            let mut buf = buffer.lock().await;
                            for ev in batch.into_iter().take(MAX_BUFFER - buf.len()) {
                                buf.push(ev);
                            }
                        }
                    }
                }
            }
        });
        self.handle = Some(handle);
    }

    pub fn stop(&mut self) {
        if let Some(h) = self.handle.take() {
            h.abort();
        }
    }

    pub async fn buffered_count(&self) -> usize {
        self.buffer.lock().await.len()
    }
}

/// Grab the current foreground window's app + title + pid. Returns
/// None when there is no active window (uncommon) or the platform
/// call fails.
fn sample_now() -> Option<AppWindowEvent> {
    let win = active_win_pos_rs::get_active_window().ok()?;
    Some(AppWindowEvent {
        captured_at: chrono::Utc::now().to_rfc3339(),
        app_name: win.app_name,
        window_title: win.title,
        pid: win.process_id as i64,
    })
}

async fn flush_batch(http: &reqwest::Client, batch: &[AppWindowEvent]) -> Result<()> {
    let jwt = read_jwt().ok_or_else(|| anyhow::anyhow!("not signed in"))?;
    let url = format!("{}/me/telemetry/ingest", CLOUD_BASE);
    let events = batch
        .iter()
        .map(|e| {
            serde_json::json!({
                "kind": "app_window",
                "captured_at": e.captured_at,
                "payload": {
                    "app_name": e.app_name,
                    "window_title": e.window_title,
                    "pid": e.pid,
                },
            })
        })
        .collect::<Vec<_>>();
    http.post(&url)
        .bearer_auth(jwt)
        .json(&serde_json::json!({ "events": events }))
        .send()
        .await?
        .error_for_status()?;
    Ok(())
}
