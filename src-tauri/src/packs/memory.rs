//! Pack memory store — v0.19.0.
//!
//! Per-pack "rules" / "preferences" / "facts" Travis records during
//! a conversation and recalls into future system prompts.
//!
//! See `migrations/0040_pack_memory.sql` for the schema doc.
//!
//! The hot path is [`recall_for_prompt`] — given a workspace + the
//! pack slugs currently enabled + the spine entity ids currently in
//! conversation context (from the journal's entity extraction), it
//! returns up to N highest-relevance memory rows. The agent loop
//! folds these into the system prompt as a "remember about this work"
//! block so the worker sees them every turn.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PackMemory {
    pub id: i64,
    pub workspace_id: i64,
    pub pack_slug: String,
    pub kind: String,
    pub target_kind: Option<String>,
    pub target_id: Option<i64>,
    pub content: String,
    pub source: String,
    pub conversation_id: Option<i64>,
    pub relevance_score: f64,
    pub pinned: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Copy)]
pub enum MemoryKind {
    Rule,
    Preference,
    Constraint,
    Fact,
    Correction,
}

impl MemoryKind {
    pub fn as_str(self) -> &'static str {
        match self {
            MemoryKind::Rule => "rule",
            MemoryKind::Preference => "preference",
            MemoryKind::Constraint => "constraint",
            MemoryKind::Fact => "fact",
            MemoryKind::Correction => "correction",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "rule" => MemoryKind::Rule,
            "preference" => MemoryKind::Preference,
            "constraint" => MemoryKind::Constraint,
            "correction" => MemoryKind::Correction,
            _ => MemoryKind::Fact,
        }
    }
}

/// Write a new memory. Returns the inserted id. If a memory with
/// identical (workspace, pack, target, content) already exists,
/// updates its `updated_at` + relevance_score and returns its id
/// instead of creating a duplicate.
pub async fn remember(
    pool: &SqlitePool,
    workspace_id: i64,
    pack_slug: &str,
    kind: MemoryKind,
    target_kind: Option<&str>,
    target_id: Option<i64>,
    content: &str,
    source: &str,
    conversation_id: Option<i64>,
) -> anyhow::Result<i64> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        anyhow::bail!("pack memory content is empty");
    }
    // Dedup: same (workspace, pack, target, content) → refresh.
    let existing: Option<(i64,)> = sqlx::query_as(
        "SELECT id FROM pack_memory
         WHERE workspace_id = ?1
           AND pack_slug = ?2
           AND IFNULL(target_kind,'') = IFNULL(?3,'')
           AND IFNULL(target_id, 0) = IFNULL(?4, 0)
           AND content = ?5",
    )
    .bind(workspace_id)
    .bind(pack_slug)
    .bind(target_kind)
    .bind(target_id)
    .bind(trimmed)
    .fetch_optional(pool)
    .await?;

    if let Some((id,)) = existing {
        sqlx::query(
            "UPDATE pack_memory
             SET updated_at = CURRENT_TIMESTAMP,
                 relevance_score = MIN(1.0, relevance_score + 0.2)
             WHERE id = ?1",
        )
        .bind(id)
        .execute(pool)
        .await?;
        return Ok(id);
    }

    let id = sqlx::query(
        "INSERT INTO pack_memory
           (workspace_id, pack_slug, kind, target_kind, target_id,
            content, source, conversation_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(workspace_id)
    .bind(pack_slug)
    .bind(kind.as_str())
    .bind(target_kind)
    .bind(target_id)
    .bind(trimmed)
    .bind(source)
    .bind(conversation_id)
    .execute(pool)
    .await?
    .last_insert_rowid();
    Ok(id)
}

/// Recall: load up to `limit` highest-relevance memories for this
/// workspace's pack slugs, scoped to (a) pack-wide memories and
/// (b) memories whose target is in `in_scope_entities`.
pub async fn recall_for_prompt(
    pool: &SqlitePool,
    workspace_id: i64,
    pack_slugs: &[&str],
    in_scope_entities: &[(String, i64)],
    limit: i64,
) -> anyhow::Result<Vec<PackMemory>> {
    if pack_slugs.is_empty() {
        return Ok(Vec::new());
    }
    let pack_placeholders = (3..3 + pack_slugs.len())
        .map(|i| format!("?{i}"))
        .collect::<Vec<_>>()
        .join(", ");
    // Build the entity-filter OR clause inline. Each entity adds two
    // placeholders (kind + id).
    let mut entity_clauses: Vec<String> = Vec::new();
    let mut next_placeholder = 3 + pack_slugs.len();
    for _ in in_scope_entities {
        entity_clauses.push(format!(
            "(target_kind = ?{} AND target_id = ?{})",
            next_placeholder,
            next_placeholder + 1
        ));
        next_placeholder += 2;
    }
    let entity_filter = if entity_clauses.is_empty() {
        "target_kind IS NULL".to_string()
    } else {
        format!(
            "(target_kind IS NULL OR {})",
            entity_clauses.join(" OR ")
        )
    };
    let sql = format!(
        "SELECT id, workspace_id, pack_slug, kind, target_kind, target_id,
                content, source, conversation_id, relevance_score, pinned,
                created_at, updated_at
         FROM pack_memory
         WHERE workspace_id = ?1
           AND pack_slug IN ({pack_placeholders})
           AND ({entity_filter})
           AND relevance_score >= 0.05
         ORDER BY pinned DESC, relevance_score DESC, updated_at DESC
         LIMIT ?2"
    );
    let mut q = sqlx::query_as::<_, PackMemory>(&sql)
        .bind(workspace_id)
        .bind(limit.clamp(1, 100));
    for s in pack_slugs {
        q = q.bind(*s);
    }
    for (kind, id) in in_scope_entities {
        q = q.bind(kind);
        q = q.bind(id);
    }
    Ok(q.fetch_all(pool).await?)
}

/// Format memories as a system-prompt block. Returns empty string
/// when there's nothing to surface, so the caller can include the
/// result unconditionally.
pub fn format_for_prompt(memories: &[PackMemory]) -> String {
    if memories.is_empty() {
        return String::new();
    }
    let mut out = String::from(
        "=== Pack memory (rules + preferences the user established earlier) ===\n",
    );
    for m in memories {
        let prefix = match m.kind.as_str() {
            "rule" => "RULE",
            "preference" => "PREF",
            "constraint" => "CONSTRAINT",
            "correction" => "CORRECTION",
            _ => "FACT",
        };
        let scope = match (&m.target_kind, m.target_id) {
            (Some(k), Some(id)) => format!(" [{}#{}]", k, id),
            _ => String::new(),
        };
        out.push_str(&format!("- {prefix}{scope}: {}\n", m.content.trim()));
    }
    out.push('\n');
    out
}
