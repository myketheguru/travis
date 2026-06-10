//! Tauri commands for document ingest + retrieval.
//!
//! `ingest_document` is the door — the frontend (drag-drop, attach
//! button, or LLM tool call) hands a file path here. We hash, dedup,
//! copy to managed storage, persist the row, and return the resulting
//! `Document` so the caller can wire it into the active workflow.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use tauri::{AppHandle, Manager, State};

use super::db::{self, Document, InsertDocument, ListFilter, Source};
use super::{extract, storage};
use crate::AppState;

/// Resolve the storage root for the current install. Returns Err with
/// a user-readable message if the app data dir can't be resolved or
/// the directory can't be created.
fn resolve_storage_root(app: &AppHandle) -> Result<PathBuf, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("could not resolve app data dir: {e}"))?;
    storage::storage_root(&data_dir).map_err(|e| format!("could not prepare storage dir: {e}"))
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IngestParams {
    /// Absolute path to the file on disk to ingest.
    pub file_path: String,
    /// Optional document kind; defaults to "file". Packs can hint
    /// (e.g., "po", "wo", "signed_sheet"); the LLM can override later
    /// via `set_document_kind`.
    pub kind: Option<String>,
    /// Optional display name. Defaults to the file's basename.
    pub display_name: Option<String>,
    /// Optional conversation to attach this drop to.
    pub conversation_id: Option<i64>,
    /// Optional workflow state to attribute this drop to.
    pub workflow_state_id: Option<i64>,
}

/// Ingest a file into Travis's managed document store.
///
/// Steps: open the file → hash bytes → dedup against existing rows in
/// the same workspace → copy to content-addressed storage if new →
/// insert document row → return the `Document`.
#[tauri::command]
pub async fn ingest_document(
    app: AppHandle,
    state: State<'_, AppState>,
    params: IngestParams,
) -> Result<Document, String> {
    let src_path = PathBuf::from(&params.file_path);
    if !src_path.exists() {
        return Err(format!("file not found: {}", src_path.display()));
    }

    let metadata = tokio::fs::metadata(&src_path)
        .await
        .map_err(|e| format!("could not stat file: {e}"))?;
    if !metadata.is_file() {
        return Err(format!("not a regular file: {}", src_path.display()));
    }

    let size_bytes = metadata.len() as i64;
    let extension = storage::extension_of(&src_path);
    let mime_type = storage::mime_from_extension(extension.as_deref()).to_string();

    let hash = storage::hash_file(&src_path)
        .await
        .map_err(|e| format!("could not hash file: {e}"))?;

    let workspace_id = state.workspace.read().await.active_id;

    // Dedup: same workspace + same hash → return the existing row.
    if let Some(existing) = db::find_by_hash(&state.db.pool, workspace_id, &hash).await {
        // Refresh conversation/workflow attribution if the caller
        // supplied new ones — the same PDF dropped in a new context
        // should be re-attributable. (Best-effort; failures don't
        // break the ingest path.)
        if params.conversation_id.is_some() || params.workflow_state_id.is_some() {
            let _ = sqlx::query(
                "UPDATE document
                 SET conversation_id = COALESCE(?1, conversation_id),
                     workflow_state_id = COALESCE(?2, workflow_state_id),
                     updated_at = CURRENT_TIMESTAMP
                 WHERE id = ?3",
            )
            .bind(params.conversation_id)
            .bind(params.workflow_state_id)
            .bind(existing.id)
            .execute(&state.db.pool)
            .await;
        }
        return Ok(existing);
    }

    let storage_root = resolve_storage_root(&app)?;
    let relative = storage::copy_into_storage(
        &src_path,
        &storage_root,
        &hash,
        extension.as_deref(),
    )
    .await
    .map_err(|e| format!("could not copy into storage: {e}"))?;

    let display_name = params
        .display_name
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| basename(&src_path));
    let original_filename = basename(&src_path);

    let kind = params
        .kind
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "file".to_string());

    let doc = db::insert(
        &state.db.pool,
        InsertDocument {
            workspace_id,
            kind: &kind,
            display_name: &display_name,
            original_filename: &original_filename,
            content_hash: &hash,
            relative_path: &relative.to_string_lossy(),
            size_bytes,
            mime_type: &mime_type,
            source: Source::UserDropped,
            conversation_id: params.conversation_id,
            workflow_state_id: params.workflow_state_id,
        },
    )
    .await
    .map_err(|e| format!("could not record document: {e}"))?;

    // Fire-and-forget extraction in the background. The frontend gets
    // the document back immediately (ingest succeeded); the extractor
    // refines extracted_json + ingest_status asynchronously. Failure
    // here is logged but doesn't propagate — the document is still
    // valid and re-runnable via extract_document.
    let pool = state.db.pool.clone();
    let http = state.http.clone();
    let storage_root_clone = storage_root.clone();
    let doc_id = doc.id;
    let app_clone = app.clone();
    tauri::async_runtime::spawn(async move {
        match extract::run_extraction(&pool, http, &storage_root_clone, doc_id).await {
            Ok(()) => {
                use tauri::Emitter;
                let _ = app_clone.emit("document-extracted", doc_id);
            }
            Err(e) => tracing::warn!("background extraction failed for doc {doc_id}: {e}"),
        }
    });

    Ok(doc)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExtractParams {
    pub document_id: i64,
    /// If true, re-extract even if status is already 'extracted'. Used
    /// when the user corrects the doc's kind and wants a fresh pass.
    #[serde(default)]
    pub force: bool,
}

/// Trigger extraction on a specific document. Awaits the extractor —
/// returns the updated document row.
#[tauri::command]
pub async fn extract_document(
    app: AppHandle,
    state: State<'_, AppState>,
    params: ExtractParams,
) -> Result<Document, String> {
    let storage_root = resolve_storage_root(&app)?;

    if params.force {
        // Reset status so the extractor re-runs.
        let _ = sqlx::query(
            "UPDATE document
             SET ingest_status = 'pending', extracted_json = NULL, extraction_error = NULL,
                 updated_at = CURRENT_TIMESTAMP
             WHERE id = ?1",
        )
        .bind(params.document_id)
        .execute(&state.db.pool)
        .await;
    }

    extract::run_extraction(
        &state.db.pool,
        state.http.clone(),
        &storage_root,
        params.document_id,
    )
    .await
    .map_err(|e| e.to_string())?;

    db::get(&state.db.pool, params.document_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "document disappeared after extraction".to_string())
}

/// List documents in the active workspace, filtered.
#[tauri::command]
pub async fn list_documents(
    state: State<'_, AppState>,
    filter: Option<ListFilter>,
) -> Result<Vec<Document>, String> {
    let mut filter = filter.unwrap_or_default();
    if filter.workspace_id.is_none() {
        filter.workspace_id = Some(state.workspace.read().await.active_id);
    }
    Ok(db::list(&state.db.pool, filter).await)
}

/// Fetch a single document by id.
#[tauri::command]
pub async fn get_document(
    state: State<'_, AppState>,
    id: i64,
) -> Result<Option<Document>, String> {
    db::get(&state.db.pool, id).await.map_err(|e| e.to_string())
}

/// Resolve the absolute on-disk path for a document — used by the
/// frontend to open the PDF in a viewer.
#[tauri::command]
pub async fn get_document_path(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> Result<String, String> {
    let doc = db::get(&state.db.pool, id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("document {id} not found"))?;
    let storage_root = resolve_storage_root(&app)?;
    Ok(storage::absolute_path(&storage_root, Path::new(&doc.relative_path))
        .to_string_lossy()
        .into_owned())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkParams {
    pub document_id: i64,
    pub entity_id: i64,
    pub relation_kind: Option<String>,
}

/// Link a document to an entity. Idempotent.
#[tauri::command]
pub async fn link_document(
    state: State<'_, AppState>,
    params: LinkParams,
) -> Result<(), String> {
    let kind = params
        .relation_kind
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| "attached_to".to_string());
    db::link_to_entity(&state.db.pool, params.document_id, params.entity_id, &kind)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetKindParams {
    pub document_id: i64,
    pub kind: String,
}

/// Update a document's kind. Used after the user (or LLM) disambiguates
/// a freshly dropped file.
#[tauri::command]
pub async fn set_document_kind(
    state: State<'_, AppState>,
    params: SetKindParams,
) -> Result<(), String> {
    db::set_kind(&state.db.pool, params.document_id, &params.kind)
        .await
        .map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzeStylingParams {
    pub document_id: i64,
    /// If true, re-analyze even if styling_json is already cached.
    #[serde(default)]
    pub force: bool,
}

/// Analyze a document's visual styling features (colours, fonts,
/// layout, signature placement) via Claude vision. Result is cached
/// on the document row for reuse by subsequent code generations.
#[tauri::command]
pub async fn analyze_document_styling(
    app: AppHandle,
    state: State<'_, AppState>,
    params: AnalyzeStylingParams,
) -> Result<serde_json::Value, String> {
    let storage_root = resolve_storage_root(&app)?;
    super::styling::analyze_styling(
        &state.db.pool,
        state.http.clone(),
        &storage_root,
        params.document_id,
        params.force,
    )
    .await
    .map_err(|e| e.to_string())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateExtractionParams {
    pub document_id: i64,
    /// Full JSON to overwrite the document's extracted_json. The
    /// caller is responsible for round-tripping the prior payload and
    /// merging corrections; this command is a straight overwrite.
    pub extracted_json: serde_json::Value,
}

/// Overwrite a document's extracted_json — used when Taylor (or the
/// LLM, on her behalf) corrects fields the extractor got wrong, OR
/// supplies a structured payload for a document whose extraction was
/// skipped (e.g., a Travis-generated PDF being treated as the source
/// of truth for a re-emit).
///
/// Bumps `ingest_status` to `extracted` and clears `extraction_error`.
#[tauri::command]
pub async fn update_document_extraction(
    state: State<'_, AppState>,
    params: UpdateExtractionParams,
) -> Result<Document, String> {
    let json_str = serde_json::to_string(&params.extracted_json)
        .map_err(|e| format!("invalid JSON: {e}"))?;
    db::set_extracted(
        &state.db.pool,
        params.document_id,
        db::IngestStatus::Extracted,
        Some(&json_str),
        None,
    )
    .await
    .map_err(|e| e.to_string())?;
    db::get(&state.db.pool, params.document_id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "document disappeared".to_string())
}

/// Delete a document. Cascade drops document_link rows. File bytes
/// remain on disk for now (a separate sweep job will GC orphaned
/// blobs).
#[tauri::command]
pub async fn delete_document(
    state: State<'_, AppState>,
    id: i64,
) -> Result<(), String> {
    db::delete(&state.db.pool, id).await.map_err(|e| e.to_string())
}

/// Open a document with the OS default viewer (Preview on macOS, Acrobat
/// or browser on Windows, xdg-open on Linux). Works for any file type
/// Travis stores — PDFs, images, spreadsheets — without bundling our
/// own viewer. Uses tauri-plugin-opener which is already wired.
///
/// Taylor's natural muscle is "I want to see this" → double-click. This
/// command is the chat-driven equivalent: she says "show me that
/// invoice" and Travis opens the file.
#[tauri::command]
pub async fn preview_document(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> Result<String, String> {
    use tauri_plugin_opener::OpenerExt;

    let doc = db::get(&state.db.pool, id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("document {id} not found"))?;

    let storage_root = resolve_storage_root(&app)?;
    let abs = storage::absolute_path(&storage_root, Path::new(&doc.relative_path));
    let abs_str = abs.to_string_lossy().into_owned();

    app.opener()
        .open_path(abs_str.clone(), None::<&str>)
        .map_err(|e| format!("could not open viewer for {abs_str}: {e}"))?;
    Ok(abs_str)
}

/// v0.18.3 — reveal a document in the OS file explorer (Finder on
/// macOS, Explorer on Windows, the default file manager on Linux).
/// Different from `preview_document` which opens the file directly
/// in its default viewer; this lets the user see WHERE on disk the
/// file lives, copy it elsewhere, etc.
#[tauri::command]
pub async fn reveal_document_in_folder(
    app: AppHandle,
    state: State<'_, AppState>,
    id: i64,
) -> Result<String, String> {
    use tauri_plugin_opener::OpenerExt;

    let doc = db::get(&state.db.pool, id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("document {id} not found"))?;

    let storage_root = resolve_storage_root(&app)?;
    let abs = storage::absolute_path(&storage_root, Path::new(&doc.relative_path));
    let abs_str = abs.to_string_lossy().into_owned();

    app.opener()
        .reveal_item_in_dir(abs.clone())
        .map_err(|e| format!("could not reveal {abs_str}: {e}"))?;
    Ok(abs_str)
}

fn basename(p: &Path) -> String {
    p.file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| p.to_string_lossy().into_owned())
}

/// Register a Travis-generated PDF (or any file) as a `Document` with
/// `source = 'generated_by_travis'`. Internal helper for the LTE pack's
/// PDF exporters — closes the round-trip loop ([[feedback-docs-first]])
/// so every artifact Travis emits is re-ingestible later.
///
/// Copies the file into managed storage (dedup by hash), inserts the
/// document row with status='skipped' (Travis doesn't need to extract
/// what it just generated), optionally links to an entity, and returns
/// the resulting `Document`.
pub async fn register_generated_document(
    app: &AppHandle,
    state: &AppState,
    source_path: &Path,
    kind: &str,
    display_name: Option<&str>,
    linked_entity_id: Option<i64>,
    conversation_id: Option<i64>,
) -> anyhow::Result<Document> {
    if !source_path.exists() {
        anyhow::bail!("generated file not found: {}", source_path.display());
    }

    let metadata = tokio::fs::metadata(source_path).await?;
    let size_bytes = metadata.len() as i64;
    let extension = storage::extension_of(source_path);
    let mime_type = storage::mime_from_extension(extension.as_deref()).to_string();
    let hash = storage::hash_file(source_path).await?;
    let workspace_id = state.workspace.read().await.active_id;

    if let Some(existing) = db::find_by_hash(&state.db.pool, workspace_id, &hash).await {
        if let Some(entity_id) = linked_entity_id {
            let _ = db::link_to_entity(
                &state.db.pool,
                existing.id,
                entity_id,
                "generated_for",
            )
            .await;
        }
        return Ok(existing);
    }

    let storage_root = app
        .path()
        .app_data_dir()
        .map_err(|e| anyhow::anyhow!("app data dir: {e}"))
        .and_then(|d| storage::storage_root(&d).map_err(Into::into))?;

    let relative = storage::copy_into_storage(
        source_path,
        &storage_root,
        &hash,
        extension.as_deref(),
    )
    .await?;

    let original = source_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("(generated)")
        .to_string();

    let label = display_name.unwrap_or(&original);

    let doc = db::insert(
        &state.db.pool,
        db::InsertDocument {
            workspace_id,
            kind,
            display_name: label,
            original_filename: &original,
            content_hash: &hash,
            relative_path: &relative.to_string_lossy(),
            size_bytes,
            mime_type: &mime_type,
            source: db::Source::GeneratedByTravis,
            conversation_id,
            workflow_state_id: None,
        },
    )
    .await?;

    // Travis-generated docs need no extraction.
    let _ = db::set_extracted(
        &state.db.pool,
        doc.id,
        db::IngestStatus::Skipped,
        None,
        None,
    )
    .await;

    if let Some(entity_id) = linked_entity_id {
        let _ = db::link_to_entity(&state.db.pool, doc.id, entity_id, "generated_for").await;
    }

    db::get(&state.db.pool, doc.id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("generated document disappeared"))
}
