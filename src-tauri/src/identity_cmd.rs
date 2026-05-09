use serde::Serialize;
use tauri::State;

use crate::identity::{self, EntityIndex};
use crate::AppState;

#[tauri::command]
pub async fn list_entities(
    state: State<'_, AppState>,
    kind: Option<String>,
    limit: Option<i64>,
) -> Result<Vec<EntityIndex>, String> {
    let limit = limit.unwrap_or(50);
    identity::list_top(&state.db.pool, kind.as_deref(), limit)
        .await
        .map_err(|e| e.to_string())
}

/// One row in the cross-pack entity list (Phase 4 slice 12). Carries
/// just enough for the list UI: identity, kind, mentions, last
/// activity, and pack attribution where applicable.
#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct EntityListRow {
    pub id: i64,
    pub kind: String,
    pub display_name: String,
    pub pack_slug: Option<String>,
    pub mentions_count: i64,
    pub last_seen: String,
    pub confidence: f64,
    pub workspace_id: i64,
}

/// List entities in the visible workspaces whose `kind` starts with
/// the supplied family prefix — `person` covers `person:unknown`
/// plus any `person:coach`, `person:friend`, etc.; `place` covers
/// `place:unknown` and pack place kinds; `org` covers `org:unknown`.
/// Pack-declared kinds without the colon (e.g. `coach`, `school`,
/// `tutor`, `student`, `dept`, `invoice`) are mapped to families
/// based on entity-kind taxonomy:
///
/// - person family: `person*`, `coach`, `tutor`, `student`, `client`
/// - place family:  `place*`, `school`, `office`
/// - org family:    `org*`, `dept`, `vendor`, `agency`
///
/// Anything that doesn't match a known family is omitted.
#[tauri::command]
pub async fn list_entities_by_family(
    state: State<'_, AppState>,
    family: String,
    limit: Option<i64>,
) -> Result<Vec<EntityListRow>, String> {
    let visible = state.workspace.read().await.visible_ids.clone();
    if visible.is_empty() {
        return Ok(Vec::new());
    }
    let kinds: &[&str] = match family.trim().to_lowercase().as_str() {
        "person" => &["person:unknown", "person:coach", "person:friend",
                      "coach", "tutor", "student", "client"],
        "place" => &["place:unknown", "school", "office"],
        "org" => &["org:unknown", "dept", "vendor", "agency"],
        _ => return Err(format!("unknown family: {family}")),
    };
    let limit = limit.unwrap_or(200).clamp(1, 1000);

    // Build placeholders for both workspaces and kinds.
    let ws_offset = 1usize;
    let ws_placeholders = (ws_offset..ws_offset + visible.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let kind_offset = visible.len() + 1;
    let kind_placeholders = (kind_offset..kind_offset + kinds.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    let limit_param = visible.len() + kinds.len() + 1;
    let sql = format!(
        "SELECT id, kind, display_name, pack_slug, mentions_count,
                last_seen, confidence, workspace_id
         FROM entity
         WHERE workspace_id IN ({ws_placeholders})
           AND archived_at IS NULL
           AND (kind IN ({kind_placeholders})
                OR kind LIKE ?{family_prefix_param})
         ORDER BY mentions_count DESC, last_seen DESC, id DESC
         LIMIT ?{limit_param}",
        family_prefix_param = limit_param + 1,
    );
    let mut q = sqlx::query_as::<_, EntityListRow>(&sql);
    for ws in &visible {
        q = q.bind(ws);
    }
    for k in kinds {
        q = q.bind(*k);
    }
    q = q.bind(limit);
    // Match anything in the family namespace we didn't enumerate
    // explicitly (e.g. a pack adds `person:mentor` later).
    q = q.bind(format!("{}:%", family.trim().to_lowercase()));

    q.fetch_all(&state.db.pool).await.map_err(|e| e.to_string())
}


#[tauri::command]
pub async fn get_profile_blurb(state: State<'_, AppState>) -> Result<String, String> {
    let profile = state
        .db
        .user_profile()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "no user profile yet".to_string())?;
    identity::build_profile_blurb(&state.db.pool, &profile)
        .await
        .map_err(|e| e.to_string())
}
