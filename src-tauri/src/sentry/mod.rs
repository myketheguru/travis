//! Sentry Phase 2 + 3 — foreground window + screenshot capture.
//!
//! Every ~30 seconds while enabled, samples the OS's foreground window
//! (title + app name + PID) via active-win-pos-rs. Every ~5 minutes
//! (10 samples), also captures a resized JPEG screenshot via xcap and
//! saves it to `<app_data>/sentry-snapshots/`. Rolling window keeps
//! the last 20 local snapshots; older ones are pruned.
//!
//! Enablement is gated by a local meta flag `sentry.app_window.enabled`
//! (toggled from Settings, gated behind the SentryConsentModal). The
//! same flag guards both window-metadata sampling AND screenshot
//! capture — no separate opt-in surface today, since the consent
//! modal already spells out both kinds.
//!
//! Sample data captured per event:
//!   - captured_at (UTC ISO)
//!   - app_name (e.g. "Google Chrome", "Slack")
//!   - window_title (e.g. "Inbox – Gmail")
//!   - pid
//!
//! Screenshot capture:
//!   - JPEG, longest side capped at 1600px, quality ~80
//!   - Saved locally as sentry-YYYYMMDD-HHMMSS.jpg
//!   - Prune keeps the last 20; older files deleted on each capture
//!   - Cloud upload of a rolling window ships in the next release —
//!     until then, snapshots stay entirely on-device (which was the
//!     user's stated preference in the consent modal).
//!
//! Still NOT captured:
//!   - Keystrokes, clipboard, password fields
//!   - Content of sensitive workspaces

pub mod cmd;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::task;
use tokio::time::{sleep, Duration};

use crate::cloud::{read_jwt, CLOUD_BASE};

const SAMPLE_INTERVAL_SECS: u64 = 30;
const FLUSH_INTERVAL_SECS: u64 = 5 * 60;
const MAX_BUFFER: usize = 400;
// v0.28.52 — screenshot every 10 samples (~5 minutes at 30s cadence).
// Chosen so a full workday collects ~96 snapshots — enough to reconstruct
// a coarse workday review without ballooning disk usage.
const SCREENSHOT_EVERY_TICKS: u64 = 10;
// Keep last 20 snapshots on disk (~1h40m of rolling context). Older
// files are pruned on every capture so disk usage stays bounded.
const MAX_LOCAL_SNAPSHOTS: usize = 20;
// Longest side (px) any snapshot is resized to before JPEG-encoding.
// 1600 keeps 4K screens readable while cutting file size ~10x.
const SCREENSHOT_MAX_SIDE: u32 = 1600;

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
    snapshot_dir: PathBuf,
}

impl SentryState {
    pub fn new(http: reqwest::Client, snapshot_dir: PathBuf) -> Self {
        Self {
            buffer: Arc::new(Mutex::new(Vec::new())),
            handle: None,
            http,
            snapshot_dir,
        }
    }

    /// Spawn the sampling loop. Idempotent — no-op if already running.
    pub fn start(&mut self) {
        if self.handle.is_some() {
            return;
        }
        let buffer = self.buffer.clone();
        let http = self.http.clone();
        let snapshot_dir = self.snapshot_dir.clone();
        let handle = task::spawn(async move {
            let mut ticks_since_flush = 0u64;
            let mut ticks_since_shot = 0u64;
            let flush_every_ticks = FLUSH_INTERVAL_SECS / SAMPLE_INTERVAL_SECS;
            loop {
                sleep(Duration::from_secs(SAMPLE_INTERVAL_SECS)).await;
                if let Some(event) = sample_now() {
                    let mut buf = buffer.lock().await;
                    if buf.len() < MAX_BUFFER {
                        buf.push(event);
                    }
                }

                ticks_since_shot += 1;
                if ticks_since_shot >= SCREENSHOT_EVERY_TICKS {
                    ticks_since_shot = 0;
                    let dir = snapshot_dir.clone();
                    // Capture off the sampling loop so a slow OS call
                    // (macOS TCC dialog, X11 stall) doesn't block the
                    // next window sample. spawn_blocking keeps xcap's
                    // synchronous API off the tokio driver.
                    task::spawn_blocking(move || {
                        if let Err(e) = capture_and_prune(&dir) {
                            tracing::warn!("sentry: screenshot failed: {e}");
                        }
                    });
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

    pub fn snapshot_dir(&self) -> &std::path::Path {
        &self.snapshot_dir
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

/// Capture the primary monitor, resize to fit within `SCREENSHOT_MAX_SIDE`,
/// encode as JPEG, save to `dir/sentry-<utc>.jpg`, then prune older
/// snapshots so at most `MAX_LOCAL_SNAPSHOTS` remain.
///
/// Returns the path of the saved snapshot on success. Synchronous —
/// callers should run it inside `task::spawn_blocking` so xcap's
/// platform calls don't block the tokio runtime.
pub fn capture_and_prune(dir: &std::path::Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("create dir: {e}"))?;

    let monitors = xcap::Monitor::all().map_err(|e| format!("enumerate monitors: {e}"))?;
    let primary = monitors
        .into_iter()
        .next()
        .ok_or_else(|| "no monitors detected".to_string())?;
    let raw = primary
        .capture_image()
        .map_err(|e| format!("capture image: {e}"))?;

    let (w, h) = (raw.width(), raw.height());
    let long_side = w.max(h);
    let resized_rgba = if long_side > SCREENSHOT_MAX_SIDE {
        let scale = SCREENSHOT_MAX_SIDE as f32 / long_side as f32;
        let nw = (w as f32 * scale).round() as u32;
        let nh = (h as f32 * scale).round() as u32;
        image::imageops::resize(&raw, nw, nh, image::imageops::FilterType::Triangle)
    } else {
        raw
    };
    // JPEG doesn't carry alpha; drop the channel now so the encoder
    // never has to guess. `image::DynamicImage::to_rgb8` is a straight
    // channel drop, no matrix math.
    let resized =
        image::DynamicImage::ImageRgba8(resized_rgba).to_rgb8();

    let filename = format!(
        "sentry-{}.jpg",
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    );
    let path = dir.join(&filename);
    let file = std::fs::File::create(&path).map_err(|e| format!("create file: {e}"))?;
    let mut writer = std::io::BufWriter::new(file);
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut writer, 80);
    encoder
        .encode_image(&resized)
        .map_err(|e| format!("encode jpeg: {e}"))?;

    prune_snapshots(dir, MAX_LOCAL_SNAPSHOTS);
    Ok(path)
}

/// Keep the newest `max_keep` snapshots in `dir`; delete the rest.
/// Silent on individual failures — a locked file just gets skipped
/// this pass and cleaned up next time.
fn prune_snapshots(dir: &std::path::Path, max_keep: usize) {
    let Ok(iter) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = iter
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("jpg"))
                .unwrap_or(false)
        })
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|n| n.starts_with("sentry-"))
                .unwrap_or(false)
        })
        .collect();
    // File names are UTC timestamps, so lexical sort is chronological.
    entries.sort_by_key(|e| e.file_name());
    if entries.len() <= max_keep {
        return;
    }
    let cull = entries.len() - max_keep;
    for entry in entries.into_iter().take(cull) {
        let _ = std::fs::remove_file(entry.path());
    }
}

/// List all snapshot files in `dir` newest-first, capped at `limit`.
/// Public so `cmd::sentry_list_snapshots` can hand paths + captured_at
/// timestamps to the frontend gallery.
pub fn list_snapshots(dir: &std::path::Path, limit: usize) -> Vec<SentrySnapshotInfo> {
    let Ok(iter) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut entries: Vec<_> = iter
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|s| s.eq_ignore_ascii_case("jpg"))
                .unwrap_or(false)
        })
        .collect();
    // Newest first (reverse lexical since filenames are timestamps).
    entries.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    entries
        .into_iter()
        .take(limit)
        .filter_map(|e| {
            let path = e.path();
            let name = e.file_name().to_string_lossy().to_string();
            let size = e.metadata().ok().map(|m| m.len()).unwrap_or(0);
            // Filename shape: sentry-YYYYMMDD-HHMMSS.jpg → captured_at
            // is that timestamp reinterpreted as ISO-8601 UTC.
            let captured_at = name
                .strip_prefix("sentry-")
                .and_then(|s| s.strip_suffix(".jpg"))
                .and_then(|s| {
                    let mut parts = s.splitn(2, '-');
                    let date = parts.next()?;
                    let time = parts.next()?;
                    if date.len() != 8 || time.len() != 6 {
                        return None;
                    }
                    Some(format!(
                        "{}-{}-{}T{}:{}:{}Z",
                        &date[0..4],
                        &date[4..6],
                        &date[6..8],
                        &time[0..2],
                        &time[2..4],
                        &time[4..6],
                    ))
                })
                .unwrap_or_default();
            Some(SentrySnapshotInfo {
                path: path.to_string_lossy().to_string(),
                filename: name,
                captured_at,
                bytes: size,
            })
        })
        .collect()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SentrySnapshotInfo {
    pub path: String,
    pub filename: String,
    pub captured_at: String,
    pub bytes: u64,
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
