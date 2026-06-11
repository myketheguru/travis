//! Background capture pipeline.
//!
//! The user has consistently asked for capture (tasks, reminders,
//! entity mentions, etc.) to live in a separate non-blocking
//! pipeline from the chat path — so it can never interfere with
//! the conversation, never gate or pollute Travis's reply.
//!
//! v0.15.2 ships the minimum-viable architectural file-level split:
//! capture *persistence* for the highest-pain fields (tasks,
//! reminders) moves into `tauri::async_runtime::spawn` after the
//! assistant message is appended. The chat path returns
//! immediately; persistence completes in the background.
//!
//! Out of scope for v0.15.2 (queued for v0.15.3 / v0.16):
//! - A second LLM call dedicated to capture extraction (cost +
//!   architectural separation but doubles request cost; deferred).
//! - Moving entity / entity_facts / hypotheses / affect_signals /
//!   workspace_routing persistence into this module (they touch
//!   more shared state and are higher refactoring risk; left
//!   inline for now).
//!
//! The shape of this module is intentionally simple: a snapshot
//! struct that captures everything the persistence code needs from
//! the LLM extraction, and a single `run_background` function that
//! does the persistence under a spawned task. Each persistence
//! call uses the existing journal-side helpers (task::upsert,
//! reminders::upsert) — no logic duplication.

use sqlx::SqlitePool;
use tauri::{AppHandle, Emitter};

use crate::journal::{Extraction, ExtractedReminder, ExtractedTask};
use crate::packs::PackHandle;
use crate::workspaces;

/// Snapshot of everything `run_background` needs. Built inside
/// `journal_ingest` after the LLM call completes, then `move`d
/// into the spawned task. All fields are owned/cloneable so the
/// chat command can return without waiting on persistence.
pub struct CaptureSnapshot {
    pub pool: SqlitePool,
    pub app: AppHandle,
    pub conv_id: i64,
    pub tasks: Vec<ExtractedTask>,
    pub reminders: Vec<ExtractedReminder>,
    pub dest_ws_state: workspaces::State,
    /// v0.19.3 — packs in scope when the snapshot was built. Static
    /// trait-object refs travel safely. Each pack's
    /// `ensure_entity` / `apply_extraction_observations` runs after
    /// tasks + reminders so it never blocks chat.
    pub enabled_packs: Vec<&'static dyn PackHandle>,
    /// v0.19.3 — full extraction JSON snapshot so packs can pluck out
    /// the fields they care about (coach_hours, document_classifications,
    /// pack-specific bucket observations) without core depending on
    /// pack-specific typed shapes.
    pub extraction: Extraction,
    /// v0.19.3 — co-mentioned entities in this turn. Packs use these
    /// as parent_hint for kinds that benefit (e.g. engagement →
    /// school).
    pub entities_snapshot: std::collections::HashMap<String, Vec<String>>,
}

/// Run capture persistence for a single chat turn in the background.
///
/// Best-effort: errors are logged but never propagated — the chat
/// command has already returned, so failing here only loses the
/// capture write. Emits a `capture-applied` Tauri event with
/// counts when finished so a future UI affordance can surface
/// "tracked N tasks behind the scenes".
pub async fn run_background(snap: CaptureSnapshot) {
    let mut task_count = 0usize;
    let mut reminder_count = 0usize;

    // Tasks. Use the same upsert path the inline code uses.
    for t in &snap.tasks {
        let title = t.title.trim();
        if title.is_empty() {
            continue;
        }
        let truncated: String = if title.chars().count() > 120 {
            title.chars().take(120).collect()
        } else {
            title.to_string()
        };
        match crate::domain::task::upsert(
            &snap.pool,
            &snap.dest_ws_state,
            crate::domain::task::TaskInput {
                id: None,
                title: truncated,
                description: t.notes.clone(),
                priority: t.priority,
                due_at: t.due_at.clone(),
                entity_id: None,
                link_kind: None,
                link_id: None,
                source: Some("journal".into()),
            },
        )
        .await
        {
            Ok(_) => task_count += 1,
            Err(e) => tracing::warn!("background capture: task upsert failed: {e}"),
        }
    }

    // Reminders. Same path as inline.
    for r in &snap.reminders {
        let text = r.text.trim();
        if text.is_empty() {
            continue;
        }
        let remind_at = r.remind_at.as_deref().map(str::trim).unwrap_or("");
        if remind_at.is_empty() {
            continue;
        }
        match crate::reminders::upsert(
            &snap.pool,
            snap.dest_ws_state.active_id,
            crate::reminders::ReminderInput {
                id: None,
                text: text.to_string(),
                remind_at: Some(remind_at.to_string()),
                kind: Some("time".into()),
                source: Some("journal".into()),
                link_kind: None,
                link_id: None,
            },
        )
        .await
        {
            Ok(_) => reminder_count += 1,
            Err(e) => tracing::warn!("background capture: reminder upsert failed: {e}"),
        }
    }

    // v0.19.3 — per-pack auto-population. Each pack ensures rows
    // for entities of its declared kinds, then applies its
    // extraction observations (document classifications, pack-
    // specific extraction fields). All best-effort; failures log
    // but never propagate. Schools auto-create FIRST so engagement-
    // type kinds can find them as parent hints.
    let ws_id = snap.dest_ws_state.active_id;
    let extraction_json = serde_json::to_value(&snap.extraction).unwrap_or(serde_json::Value::Null);
    let mut auto_created: usize = 0;

    // Two passes so anchor kinds (school) resolve before downstream
    // ones (engagement) need them as parent_hint.
    for pass in [0, 1] {
        for pack in &snap.enabled_packs {
            for kind in pack.entity_kinds() {
                let is_anchor = matches!(*kind, "school" | "client" | "tutor");
                if pass == 0 && !is_anchor {
                    continue;
                }
                if pass == 1 && is_anchor {
                    continue;
                }
                let bucket = format!("{kind}s");
                let names = match snap.entities_snapshot.get(&bucket) {
                    Some(v) => v.clone(),
                    None => continue,
                };
                // Resolve parent_hint from the same extraction's
                // anchor entities. For non-anchor kinds, look up
                // the first school name's id and pass it so the
                // pack's ensure_entity can wire the FK. v0.20.5:
                // previously this was a TODO that always returned
                // None, leaving engagement.school_id NULL and
                // breaking the drill-down's invoice/hours/doc
                // queries.
                let parent_hint: Option<(&str, i64)> = if !is_anchor {
                    if let Some(school_name) = snap
                        .entities_snapshot
                        .get("schools")
                        .and_then(|s| s.first())
                    {
                        let row: Option<(i64,)> = sqlx::query_as(
                            "SELECT id FROM school
                             WHERE workspace_id = ?1
                               AND LOWER(name) = LOWER(?2)
                             ORDER BY id ASC LIMIT 1",
                        )
                        .bind(ws_id)
                        .bind(school_name)
                        .fetch_optional(&snap.pool)
                        .await
                        .ok()
                        .flatten();
                        row.map(|(id,)| ("school", id))
                    } else {
                        None
                    }
                } else {
                    None
                };
                for name in &names {
                    if let Err(e) = pack
                        .ensure_entity(&snap.pool, ws_id, kind, name, parent_hint)
                        .await
                    {
                        tracing::warn!(
                            "background capture: ensure_entity {}/{}/{}: {e}",
                            pack.slug(),
                            kind,
                            name
                        );
                    } else {
                        auto_created += 1;
                    }
                }
            }
        }
    }

    // Then each pack gets the full extraction to handle its own
    // observation fields (coach_hours, document_classifications,
    // anything else the pack declared on its extraction schema).
    let mut observations_applied: usize = 0;
    for pack in &snap.enabled_packs {
        match pack
            .apply_extraction_observations(&snap.pool, ws_id, snap.conv_id, &extraction_json)
            .await
        {
            Ok(_) => observations_applied += 1,
            Err(e) => tracing::warn!(
                "background capture: apply_extraction_observations {}: {e}",
                pack.slug()
            ),
        }
    }

    // v0.20.2+ Tier 4 — when a document is classified as a sample or
    // template, kick off binary asset extraction in the background.
    // The LLM's next turn can then call `list_template_assets(doc_id)`
    // and embed the actual PNGs / page renders in its run_python
    // output instead of approximating from styling_json.
    if let Some(classifications) = extraction_json
        .get("documentClassifications")
        .and_then(|v| v.as_array())
    {
        for c in classifications {
            let kind = c
                .get("kind")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let is_template_kind =
                kind.starts_with("sample_") || kind.starts_with("template_") || kind == "sample";
            if !is_template_kind {
                continue;
            }
            let Some(doc_id) = c.get("documentId").and_then(|v| v.as_i64()) else {
                continue;
            };
            crate::template_assets::schedule_extraction(
                snap.app.clone(),
                snap.pool.clone(),
                doc_id,
            )
            .await;
        }
    }

    // Tell the UI something landed. v0.15.2 doesn't render this
    // yet; the event is here so a future "tracked N in background"
    // notification can be wired without backend changes.
    let _ = snap.app.emit(
        "capture-applied",
        serde_json::json!({
            "conversationId": snap.conv_id,
            "tasks": task_count,
            "reminders": reminder_count,
            "autoCreated": auto_created,
            "observationsApplied": observations_applied,
        }),
    );

    tracing::info!(
        "background capture: conv {} → {} task(s), {} reminder(s), {} auto-created, {} pack observation pass(es)",
        snap.conv_id,
        task_count,
        reminder_count,
        auto_created,
        observations_applied
    );
}
