//! `Step` — RAII helper that emits events + persists rows.
//!
//! Usage:
//! ```rust
//! let mut step = Step::start(
//!     &app, &pool, conversation_id,
//!     StepKind::ToolCall,
//!     "Reading PO document",
//!     Some("doc#42 (PS 498 PO)".into()),
//!     None, // no parent
//! ).await?;
//! step.note(&app, "Extracted 8 line items").await;
//! step.complete_ok(&app, &pool, Some("PO total $7,064".into())).await?;
//! ```
//!
//! Dropping a Step without calling complete_* marks it cancelled.

use sqlx::SqlitePool;
use std::time::Instant;
use tauri::{AppHandle, Emitter};

use super::model::{StepEvent, StepKind, StepStatus};

pub struct Step {
    pub id: String,
    pub conversation_id: i64,
    pub parent_step_id: Option<String>,
    pub kind: StepKind,
    started_at_instant: Instant,
    completed: bool,
}

impl Step {
    /// Begin a step — generates a uuid, emits Started, persists row
    /// with status=running.
    pub async fn start(
        app: &AppHandle,
        pool: &SqlitePool,
        conversation_id: i64,
        kind: StepKind,
        name: impl Into<String>,
        detail: Option<String>,
        parent_step_id: Option<String>,
    ) -> anyhow::Result<Step> {
        let id = format!("step_{}", uuid_lite());
        let name = name.into();
        let now = chrono::Utc::now().to_rfc3339();

        sqlx::query(
            "INSERT INTO step
                (id, conversation_id, parent_step_id, kind, name, detail, status, started_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'running', ?7)",
        )
        .bind(&id)
        .bind(conversation_id)
        .bind(parent_step_id.as_deref())
        .bind(kind.as_db_str())
        .bind(&name)
        .bind(detail.as_deref())
        .bind(&now)
        .execute(pool)
        .await?;

        let _ = app.emit(
            "step-event",
            &StepEvent::Started {
                step_id: id.clone(),
                parent_step_id: parent_step_id.clone(),
                conversation_id,
                kind,
                name,
                detail,
                started_at: now,
            },
        );

        Ok(Step {
            id,
            conversation_id,
            parent_step_id,
            kind,
            started_at_instant: Instant::now(),
            completed: false,
        })
    }

    /// Append a note. Persists into notes_json and emits Note.
    pub async fn note(&self, app: &AppHandle, pool: &SqlitePool, text: impl Into<String>) {
        let text = text.into();
        // Append to notes_json
        let _ = sqlx::query(
            "UPDATE step
             SET notes_json = json_insert(notes_json, '$[#]', ?1)
             WHERE id = ?2",
        )
        .bind(&text)
        .bind(&self.id)
        .execute(pool)
        .await;
        let _ = app.emit(
            "step-event",
            &StepEvent::Note {
                step_id: self.id.clone(),
                text,
            },
        );
    }

    /// Mark complete with success.
    pub async fn complete_ok(
        mut self,
        app: &AppHandle,
        pool: &SqlitePool,
        summary: Option<String>,
    ) -> anyhow::Result<()> {
        self.completed = true;
        let duration_ms = self.started_at_instant.elapsed().as_millis() as i64;
        let now = chrono::Utc::now().to_rfc3339();
        sqlx::query(
            "UPDATE step
             SET status = 'ok', summary = ?1, completed_at = ?2, duration_ms = ?3
             WHERE id = ?4",
        )
        .bind(summary.as_deref())
        .bind(&now)
        .bind(duration_ms)
        .bind(&self.id)
        .execute(pool)
        .await?;
        let _ = app.emit(
            "step-event",
            &StepEvent::Result {
                step_id: self.id.clone(),
                status: StepStatus::Ok,
                summary,
                error: None,
            },
        );
        let _ = app.emit(
            "step-event",
            &StepEvent::Completed {
                step_id: self.id.clone(),
                duration_ms: duration_ms as u64,
            },
        );
        Ok(())
    }

    /// Mark complete with failure.
    pub async fn complete_err(
        mut self,
        app: &AppHandle,
        pool: &SqlitePool,
        error: impl Into<String>,
    ) -> anyhow::Result<()> {
        self.completed = true;
        let duration_ms = self.started_at_instant.elapsed().as_millis() as i64;
        let now = chrono::Utc::now().to_rfc3339();
        let error = error.into();
        sqlx::query(
            "UPDATE step
             SET status = 'failed', summary = ?1, completed_at = ?2, duration_ms = ?3
             WHERE id = ?4",
        )
        .bind(&error)
        .bind(&now)
        .bind(duration_ms)
        .bind(&self.id)
        .execute(pool)
        .await?;
        let _ = app.emit(
            "step-event",
            &StepEvent::Result {
                step_id: self.id.clone(),
                status: StepStatus::Failed,
                summary: None,
                error: Some(error),
            },
        );
        let _ = app.emit(
            "step-event",
            &StepEvent::Completed {
                step_id: self.id.clone(),
                duration_ms: duration_ms as u64,
            },
        );
        Ok(())
    }
}

impl Drop for Step {
    fn drop(&mut self) {
        if !self.completed {
            tracing::warn!(
                "Step {} ({}) dropped without completion — marking cancelled in next persistence pass",
                self.id, self.kind.as_db_str()
            );
            // We can't await here. The row stays in 'running' until a
            // background cleanup tick marks orphans as 'cancelled'. A
            // separate cleanup pass on startup handles this.
        }
    }
}

/// Lightweight RFC4122-ish identifier without pulling in the uuid crate.
fn uuid_lite() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let ts = chrono::Utc::now().timestamp_micros();
    let c = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{ts:x}{c:x}")
}
