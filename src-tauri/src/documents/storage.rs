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

/// v0.19.2 — friendly mirror layout under the OS Documents dir.
///
/// Hash-addressed storage is great for dedup but terrible for human
/// browsing. We maintain a parallel tree at:
///   <user-documents>/Travis/files/<conversation-slug>/<original-name>
///
/// Each entry is a hard link to the canonical hash file (one inode
/// shared with the storage tree, so no disk duplication). On Windows
/// hardlinks work without admin as long as both paths live on the
/// same volume. On cross-volume failure or any other error we fall
/// back to a plain copy.
///
/// Returns the absolute friendly path so callers can persist it on
/// the document row for "reveal in folder" UX.
pub fn user_facing_root(documents_dir: &Path) -> std::io::Result<PathBuf> {
    let root = documents_dir.join("Travis").join("files");
    std::fs::create_dir_all(&root)?;
    Ok(root)
}

/// Build a filesystem-safe slug from a free-form conversation label.
/// Keeps alphanumerics, hyphen, underscore, and space; everything else
/// collapses to `_`. Empty/whitespace-only input → `"misc"`.
pub fn conversation_slug(label: &str) -> String {
    let cleaned: String = label
        .chars()
        .map(|c| match c {
            c if c.is_alphanumeric() => c,
            '-' | '_' | ' ' | '.' => c,
            _ => '_',
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('_').to_string();
    if trimmed.is_empty() {
        "misc".to_string()
    } else {
        // Cap to keep paths reasonable on Windows (MAX_PATH).
        trimmed.chars().take(80).collect()
    }
}

/// Mirror a hash-stored file into the user-facing folder. Hardlink
/// preferred (zero disk cost), copy on fallback. Returns the friendly
/// absolute path or an error; the caller is expected to log-and-
/// continue, not fail the user's flow.
pub fn mirror_into_user_folder(
    user_facing_root: &Path,
    hash_path: &Path,
    conversation_slug: &str,
    original_filename: &str,
) -> std::io::Result<PathBuf> {
    let conv_dir = user_facing_root.join(conversation_slug);
    std::fs::create_dir_all(&conv_dir)?;
    // Avoid clobbering — if a file with the same name already exists,
    // suffix with a counter ("name (2).ext").
    let target = unique_path(&conv_dir, original_filename);
    // Try hardlink first; fall back to copy on cross-volume / FS-not-
    // supported / permission failures.
    match std::fs::hard_link(hash_path, &target) {
        Ok(()) => Ok(target),
        Err(_) => {
            std::fs::copy(hash_path, &target)?;
            Ok(target)
        }
    }
}

fn unique_path(dir: &Path, basename: &str) -> PathBuf {
    let candidate = dir.join(basename);
    if !candidate.exists() {
        return candidate;
    }
    let stem = Path::new(basename)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file");
    let ext = Path::new(basename)
        .extension()
        .and_then(|s| s.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    for n in 2..=200 {
        let try_name = format!("{stem} ({n}){ext}");
        let try_path = dir.join(&try_name);
        if !try_path.exists() {
            return try_path;
        }
    }
    candidate
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
