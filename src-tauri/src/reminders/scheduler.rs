use std::sync::Arc;
use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;

use crate::db::Db;

/// Format `now` the same way SQLite's `CURRENT_TIMESTAMP` does
/// (`YYYY-MM-DD HH:MM:SS` UTC) so string comparisons against `remind_at`
/// stay consistent with the rest of the schema.
fn now_string() -> String {
    chrono::Utc::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

/// Spawn a background tokio task that polls every 30 seconds for due
/// time-based reminders and dispatches an OS notification for each one,
/// then marks them as fired.
pub fn spawn(app: AppHandle, db: Arc<Db>) {
    tauri::async_runtime::spawn(async move {
        // Small initial delay so we don't fire on the very first tick before
        // the rest of setup is fully complete.
        tokio::time::sleep(Duration::from_secs(5)).await;
        let mut ticker = tokio::time::interval(Duration::from_secs(30));
        // Skip the first immediate tick from `interval`.
        ticker.tick().await;
        loop {
            ticker.tick().await;
            if let Err(e) = tick(&app, &db).await {
                tracing::warn!("reminders scheduler tick failed: {e}");
            }
        }
    });
}

async fn tick(app: &AppHandle, db: &Db) -> anyhow::Result<()> {
    let now = now_string();
    let due = super::due_now(&db.pool, &now)
        .await
        .map_err(|e| anyhow::anyhow!(e.to_string()))?;

    if due.is_empty() {
        return Ok(());
    }

    for reminder in due {
        match app
            .notification()
            .builder()
            .title("Travis")
            .body(&reminder.text)
            .show()
        {
            Ok(_) => tracing::info!("reminder fired id={} text={}", reminder.id, reminder.text),
            Err(e) => tracing::warn!(
                "failed to show notification for reminder id={}: {e}",
                reminder.id
            ),
        }

        if let Err(e) = super::mark_fired(&db.pool, reminder.id, &now).await {
            tracing::warn!("failed to mark reminder {} fired: {e}", reminder.id);
        }
    }

    Ok(())
}
