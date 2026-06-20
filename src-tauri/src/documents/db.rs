//! Document DB operations.
//!
//! The store-side persistence layer for ingested documents. Each
//! ingest reads file bytes, hashes them, copies to managed storage,
//! and inserts a row here. Linking to entities (a PO attaching to a
//! contract + engagement + school) is one row per (document, entity,
//! relation_kind) triple.

use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum IngestStatus {
    Pending,
    Extracted,
    Failed,
    Skipped,
}

impl IngestStatus {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            IngestStatus::Pending => "pending",
            IngestStatus::Extracted => "extracted",
            IngestStatus::Failed => "failed",
            IngestStatus::Skipped => "skipped",
        }
    }

    pub fn from_db_str(s: &str) -> Self {
        match s {
            "extracted" => IngestStatus::Extracted,
            "failed" => IngestStatus::Failed,
            "skipped" => IngestStatus::Skipped,
            _ => IngestStatus::Pending,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    UserDropped,
    GeneratedByTravis,
    Imported,
}

impl Source {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            Source::UserDropped => "user_dropped",
            Source::GeneratedByTravis => "generated_by_travis",
            Source::Imported => "imported",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Document {
    pub id: i64,
    pub workspace_id: i64,
    pub kind: String,
    pub display_name: String,
    pub original_filename: String,
    pub content_hash: String,
    pub relative_path: String,
    pub size_bytes: i64,
    pub mime_type: String,
    pub ingest_status: String,
    pub extracted_json: Option<String>,
    pub extraction_error: Option<String>,
    pub source: String,
    pub conversation_id: Option<i64>,
    pub workflow_state_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(sqlx::FromRow)]
struct DocumentRow {
    id: i64,
    workspace_id: i64,
    kind: String,
    display_name: String,
    original_filename: String,
    content_hash: String,
    relative_path: String,
    size_bytes: i64,
    mime_type: String,
    ingest_status: String,
    extracted_json: Option<String>,
    extraction_error: Option<String>,
    source: String,
    conversation_id: Option<i64>,
    workflow_state_id: Option<i64>,
    created_at: String,
    updated_at: String,
}

impl From<DocumentRow> for Document {
    fn from(r: DocumentRow) -> Self {
        Self {
            id: r.id,
            workspace_id: r.workspace_id,
            kind: r.kind,
            display_name: r.display_name,
            original_filename: r.original_filename,
            content_hash: r.content_hash,
            relative_path: r.relative_path,
            size_bytes: r.size_bytes,
            mime_type: r.mime_type,
            ingest_status: r.ingest_status,
            extracted_json: r.extracted_json,
            extraction_error: r.extraction_error,
            source: r.source,
            conversation_id: r.conversation_id,
            workflow_state_id: r.workflow_state_id,
            created_at: r.created_at,
            updated_at: r.updated_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentLink {
    pub id: i64,
    pub document_id: i64,
    pub entity_id: i64,
    pub relation_kind: String,
    pub created_at: String,
}

/// Look up a document by its content hash within a workspace. Used by
/// the ingest path to dedup identical drops.
pub async fn find_by_hash(
    pool: &SqlitePool,
    workspace_id: i64,
    content_hash: &str,
) -> Option<Document> {
    sqlx::query_as::<_, DocumentRow>(
        "SELECT id, workspace_id, kind, display_name, original_filename,
                content_hash, relative_path, size_bytes, mime_type,
                ingest_status, extracted_json, extraction_error, source,
                conversation_id, workflow_state_id, created_at, updated_at
         FROM document
         WHERE workspace_id = ?1 AND content_hash = ?2
         ORDER BY id ASC LIMIT 1",
    )
    .bind(workspace_id)
    .bind(content_hash)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()
    .map(Into::into)
}

#[derive(Debug, Clone)]
pub struct InsertDocument<'a> {
    pub workspace_id: i64,
    pub kind: &'a str,
    pub display_name: &'a str,
    pub original_filename: &'a str,
    pub content_hash: &'a str,
    pub relative_path: &'a str,
    pub size_bytes: i64,
    pub mime_type: &'a str,
    pub source: Source,
    pub conversation_id: Option<i64>,
    pub workflow_state_id: Option<i64>,
}

/// Insert a new document row. Caller must have already copied the file
/// bytes into managed storage at `relative_path`.
///
/// v2 Phase 2.5: also wires the row into the sync layer. The insert,
/// the doc.upsert outbox enqueue, and the file-upload queue insert all
/// run in a single transaction so we never end up with partial sync
/// state. The bytes themselves get pushed to R2 by the background
/// sync worker draining `file_upload_queue` — caller is not blocked
/// on the network round-trip.
pub async fn insert(
    pool: &SqlitePool,
    input: InsertDocument<'_>,
) -> anyhow::Result<Document> {
    let mut tx = pool.begin().await?;

    let row: (i64, String) = sqlx::query_as(
        "INSERT INTO document
            (workspace_id, kind, display_name, original_filename,
             content_hash, relative_path, size_bytes, mime_type,
             source, conversation_id, workflow_state_id, cloud_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11,
                 lower(hex(randomblob(16))))
         RETURNING id, cloud_id",
    )
    .bind(input.workspace_id)
    .bind(input.kind)
    .bind(input.display_name)
    .bind(input.original_filename)
    .bind(input.content_hash)
    .bind(input.relative_path)
    .bind(input.size_bytes)
    .bind(input.mime_type)
    .bind(input.source.as_db_str())
    .bind(input.conversation_id)
    .bind(input.workflow_state_id)
    .fetch_one(&mut *tx)
    .await?;
    let id = row.0;
    let cloud_id = row.1;

    // Enqueue the doc.upsert event so the metadata flows to the cloud
    // (and onward to the user's other devices) on the next sync cycle.
    let payload = serde_json::json!({
        "cloudId": cloud_id,
        "workspaceId": input.workspace_id,
        "kind": input.kind,
        "displayName": input.display_name,
        "originalFilename": input.original_filename,
        "contentHash": input.content_hash,
        "sizeBytes": input.size_bytes,
        "mimeType": input.mime_type,
        "source": input.source.as_db_str(),
    })
    .to_string();
    sqlx::query("INSERT INTO sync_outbox (kind, payload) VALUES ('doc.upsert', ?1)")
        .bind(payload)
        .execute(&mut *tx)
        .await?;

    // Queue the bytes for R2 upload. INSERT OR IGNORE so multiple docs
    // pointing at the same content_hash only schedule one upload.
    sqlx::query(
        "INSERT OR IGNORE INTO file_upload_queue
            (content_hash, relative_path, mime_type, size_bytes)
         VALUES (?1, ?2, ?3, ?4)",
    )
    .bind(input.content_hash)
    .bind(input.relative_path)
    .bind(input.mime_type)
    .bind(input.size_bytes)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    get(pool, id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("document {id} disappeared after insert"))
}

/// Fetch a document by id.
pub async fn get(pool: &SqlitePool, id: i64) -> anyhow::Result<Option<Document>> {
    let row = sqlx::query_as::<_, DocumentRow>(
        "SELECT id, workspace_id, kind, display_name, original_filename,
                content_hash, relative_path, size_bytes, mime_type,
                ingest_status, extracted_json, extraction_error, source,
                conversation_id, workflow_state_id, created_at, updated_at
         FROM document
         WHERE id = ?1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(Into::into))
}

/// Update a document's kind. Used when the LLM disambiguates a freshly
/// dropped file ("you dropped a PO" — kind goes from 'file' → 'po').
pub async fn set_kind(pool: &SqlitePool, id: i64, kind: &str) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE document SET kind = ?1, updated_at = CURRENT_TIMESTAMP
         WHERE id = ?2",
    )
    .bind(kind)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Update the extraction status + payload after Slice 3's extractor
/// runs against a document. JSON is the structured fields the extractor
/// produced; the shape is pack-specific (each pack defines its own).
pub async fn set_extracted(
    pool: &SqlitePool,
    id: i64,
    status: IngestStatus,
    extracted_json: Option<&str>,
    error: Option<&str>,
) -> anyhow::Result<()> {
    sqlx::query(
        "UPDATE document
         SET ingest_status = ?1,
             extracted_json = ?2,
             extraction_error = ?3,
             updated_at = CURRENT_TIMESTAMP
         WHERE id = ?4",
    )
    .bind(status.as_db_str())
    .bind(extracted_json)
    .bind(error)
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListFilter {
    pub workspace_id: Option<i64>,
    pub kind: Option<String>,
    pub conversation_id: Option<i64>,
    pub workflow_state_id: Option<i64>,
    pub entity_id: Option<i64>,
    pub limit: Option<i64>,
}

/// List documents matching a filter. Returns most-recently-ingested first.
pub async fn list(pool: &SqlitePool, filter: ListFilter) -> Vec<Document> {
    let limit = filter.limit.unwrap_or(100).clamp(1, 500);

    if let Some(entity_id) = filter.entity_id {
        let rows = sqlx::query_as::<_, DocumentRow>(
            "SELECT d.id, d.workspace_id, d.kind, d.display_name,
                    d.original_filename, d.content_hash, d.relative_path,
                    d.size_bytes, d.mime_type, d.ingest_status,
                    d.extracted_json, d.extraction_error, d.source,
                    d.conversation_id, d.workflow_state_id,
                    d.created_at, d.updated_at
             FROM document d
             INNER JOIN document_link dl ON dl.document_id = d.id
             WHERE dl.entity_id = ?1
               AND (?2 IS NULL OR d.workspace_id = ?2)
               AND (?3 IS NULL OR d.kind = ?3)
             ORDER BY d.id DESC LIMIT ?4",
        )
        .bind(entity_id)
        .bind(filter.workspace_id)
        .bind(filter.kind.as_deref())
        .bind(limit)
        .fetch_all(pool)
        .await
        .unwrap_or_default();
        return rows.into_iter().map(Into::into).collect();
    }

    let rows = sqlx::query_as::<_, DocumentRow>(
        "SELECT id, workspace_id, kind, display_name, original_filename,
                content_hash, relative_path, size_bytes, mime_type,
                ingest_status, extracted_json, extraction_error, source,
                conversation_id, workflow_state_id, created_at, updated_at
         FROM document
         WHERE (?1 IS NULL OR workspace_id = ?1)
           AND (?2 IS NULL OR kind = ?2)
           AND (?3 IS NULL OR conversation_id = ?3)
           AND (?4 IS NULL OR workflow_state_id = ?4)
         ORDER BY id DESC LIMIT ?5",
    )
    .bind(filter.workspace_id)
    .bind(filter.kind.as_deref())
    .bind(filter.conversation_id)
    .bind(filter.workflow_state_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    rows.into_iter().map(Into::into).collect()
}

/// Create a document → entity link. Idempotent: re-linking with the
/// same (doc, entity, relation_kind) is a no-op.
pub async fn link_to_entity(
    pool: &SqlitePool,
    document_id: i64,
    entity_id: i64,
    relation_kind: &str,
) -> anyhow::Result<()> {
    sqlx::query(
        "INSERT OR IGNORE INTO document_link
            (document_id, entity_id, relation_kind)
         VALUES (?1, ?2, ?3)",
    )
    .bind(document_id)
    .bind(entity_id)
    .bind(relation_kind)
    .execute(pool)
    .await?;
    Ok(())
}

/// Fetch all entity links for a document.
pub async fn links_for_document(
    pool: &SqlitePool,
    document_id: i64,
) -> Vec<DocumentLink> {
    sqlx::query_as::<_, (i64, i64, i64, String, String)>(
        "SELECT id, document_id, entity_id, relation_kind, created_at
         FROM document_link
         WHERE document_id = ?1
         ORDER BY id ASC",
    )
    .bind(document_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default()
    .into_iter()
    .map(|(id, document_id, entity_id, relation_kind, created_at)| DocumentLink {
        id,
        document_id,
        entity_id,
        relation_kind,
        created_at,
    })
    .collect()
}

/// Delete a document. Cascade drops document_link rows; file bytes
/// stay on disk (could be GC'd later by a sweep job).
pub async fn delete(pool: &SqlitePool, id: i64) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM document WHERE id = ?1")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}
