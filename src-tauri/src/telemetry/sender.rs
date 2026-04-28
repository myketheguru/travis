use std::sync::Arc;
use std::time::Duration;

use tauri::AppHandle;
use tokio::time::sleep;

use crate::db::Db;
use crate::telemetry::{self, http_sink::HttpSink};

// Endpoint + bearer token are baked in at compile time. Set them in your shell
// before `npm run tauri build` (or dev). Both must be set for telemetry to fire;
// otherwise the sender silently no-ops, which is what dev builds without a key
// should do.
//
//   $env:TRAVIS_TELEMETRY_URL   = "https://us-central1-<project>.cloudfunctions.net/travisIngest"
//   $env:TRAVIS_TELEMETRY_TOKEN = "<bearer secret>"
const TELEMETRY_URL: Option<&str> = option_env!("TRAVIS_TELEMETRY_URL");
const TELEMETRY_TOKEN: Option<&str> = option_env!("TRAVIS_TELEMETRY_TOKEN");

pub fn spawn(_app: AppHandle, db: Arc<Db>, http: reqwest::Client) {
    if TELEMETRY_URL.is_none() {
        tracing::info!("telemetry: no compile-time TRAVIS_TELEMETRY_URL — sender disabled");
        return;
    }
    tauri::async_runtime::spawn(async move {
        // Stagger startup so we don't compete with onboarding/migrations.
        sleep(Duration::from_secs(15)).await;
        loop {
            tick(&db, &http).await;
            sleep(Duration::from_secs(60)).await;
        }
    });
}

async fn tick(db: &Db, http: &reqwest::Client) {
    let Some(url) = TELEMETRY_URL else { return };

    let pending = match telemetry::pending(&db.pool, 50).await {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("telemetry: read pending failed: {e}");
            return;
        }
    };
    if pending.is_empty() {
        return;
    }

    let sink = HttpSink {
        http: http.clone(),
        url: url.to_string(),
        bearer: TELEMETRY_TOKEN.map(|s| s.to_string()),
    };

    let ids: Vec<i64> = pending.iter().map(|e| e.id).collect();
    use crate::telemetry::TelemetrySink;
    match sink.send(&pending).await {
        Ok(()) => {
            if let Err(e) = telemetry::mark_sent(&db.pool, &ids).await {
                tracing::warn!("telemetry: mark_sent failed: {e}");
            } else {
                tracing::info!("telemetry: sent {} events", ids.len());
                let _ = sqlx::query(
                    "UPDATE telemetry_config SET last_sent_at = CURRENT_TIMESTAMP WHERE id = 1",
                )
                .execute(&db.pool)
                .await;
            }
        }
        Err(e) => {
            tracing::warn!("telemetry: send failed: {e}");
            let _ = telemetry::mark_failed(&db.pool, &ids, &e.to_string()).await;
        }
    }
}
