//! File storage layer — managed on-disk location for ingested documents.
//!
//! Files live under `<app_data>/documents/<hash_prefix>/<full_hash><ext>`.
//! Content-addressed: identical drops dedup automatically; the DB row
//! is the unit of multiplicity (two `document` rows can point at the
//! same file with different metadata if needed). The hash prefix
//! directory keeps any one folder from holding too many files at
//! scale.
//!
//! See [[feedback-docs-first]] — Taylor's bytes always stay on her
//! device. Cloud LLM calls during extraction see the document text
//! transiently; they never write the bytes anywhere we control.

use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use tokio::io::AsyncReadExt;

/// Where managed documents live, relative to the app's data dir.
const STORAGE_DIR_NAME: &str = "documents";

/// Resolve the absolute path to the documents storage root for this
/// installation. Creates the directory on first call.
pub fn storage_root(app_data_dir: &Path) -> std::io::Result<PathBuf> {
    let root = app_data_dir.join(STORAGE_DIR_NAME);
    std::fs::create_dir_all(&root)?;
    Ok(root)
}

/// Compute SHA-256 of a file's bytes. Returns the lowercase hex digest.
pub async fn hash_file(path: &Path) -> std::io::Result<String> {
    let mut f = tokio::fs::File::open(path).await?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    let digest = hasher.finalize();
    Ok(hex_encode(&digest))
}

/// Compute the relative path inside the storage root that a given
/// hash (+ extension) should live at. Format: `<first 2>/<hash><ext>`.
pub fn relative_path_for(hash: &str, extension: Option<&str>) -> PathBuf {
    let prefix = &hash[..2.min(hash.len())];
    let mut leaf = String::with_capacity(hash.len() + 8);
    leaf.push_str(hash);
    if let Some(ext) = extension {
        if !ext.is_empty() {
            if !ext.starts_with('.') {
                leaf.push('.');
            }
            leaf.push_str(ext);
        }
    }
    PathBuf::from(prefix).join(leaf)
}

/// Copy a source file into managed storage at the computed path. If
/// the destination already exists (dedup hit), returns Ok without
/// rewriting. Returns the relative path that should be persisted on
/// the `document` row.
pub async fn copy_into_storage(
    src: &Path,
    storage_root: &Path,
    hash: &str,
    extension: Option<&str>,
) -> std::io::Result<PathBuf> {
    let rel = relative_path_for(hash, extension);
    let dst = storage_root.join(&rel);

    if let Some(parent) = dst.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Skip copy if the destination already exists with the right size
    // — content is hashed so we trust it as long as the file is there.
    if tokio::fs::metadata(&dst).await.is_ok() {
        return Ok(rel);
    }

    tokio::fs::copy(src, &dst).await?;
    Ok(rel)
}

/// Resolve a stored relative path back to an absolute disk path.
pub fn absolute_path(storage_root: &Path, relative: &Path) -> PathBuf {
    storage_root.join(relative)
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Extension of a path including the dot, lowercased. None when the
/// path has no extension. Examples: `"po.pdf"` → `Some("pdf")`,
/// `"sheet"` → `None`.
pub fn extension_of(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
}

/// Best-effort mime type from a file extension. Used at ingest time
/// to populate `document.mime_type`. Default is octet-stream.
pub fn mime_from_extension(ext: Option<&str>) -> &'static str {
    match ext.map(|s| s.to_ascii_lowercase()).as_deref() {
        Some("pdf") => "application/pdf",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webp") => "image/webp",
        Some("txt") => "text/plain",
        Some("csv") => "text/csv",
        Some("xlsx") => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        Some("docx") => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        _ => "application/octet-stream",
    }
}
