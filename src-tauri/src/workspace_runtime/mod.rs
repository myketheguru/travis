//! Workspace runtime abstraction — OpenHands-style.
//!
//! A `Workspace` is *where Travis executes things and reads/writes
//! files*. Today there's only one: [`LocalWorkspace`], rooted at the
//! user's Travis app-data dir. The trait exists so future modes can
//! drop in without rewriting every caller:
//!
//! - **LocalWorkspace** (present) — host filesystem, in-process
//!   Pyodide for Python. Zero isolation; trusted user content only.
//! - **DockerWorkspace** (future) — sandboxed container with full
//!   CPython + arbitrary shell. For risky third-party packs or
//!   untrusted inputs.
//! - **RemoteWorkspace** (future) — server-side execution for the
//!   eventual hosted-Travis SKU.
//!
//! **Distinct from `crate::workspaces` (DB row).** That concept is
//! organisational scope ("Personal" vs "Coaches" vs "DoF"); this is
//! execution surface. The DB workspace decides *what data the user
//! sees*; the runtime workspace decides *where code runs and files
//! live*.
//!
//! ## Scope of v0.16.6
//!
//! Substrate only — trait + LocalWorkspace impl. No callers migrated
//! yet; documents/PDFs/attachments continue using `std::fs` directly.
//! Future slices migrate one caller at a time as the need arises
//! (the bar is "we have a real second implementation to support").
//!
//! See BRAIN.md → "Workspace runtime" for the discipline note.

use async_trait::async_trait;
use std::path::{Path, PathBuf};
use tokio::fs;

/// Read/write/list operations against the underlying file substrate.
/// Async because the future Docker/Remote impls have to do IPC.
#[async_trait]
pub trait Workspace: Send + Sync {
    /// Stable identifier for logs, e.g. "local", "docker", "remote".
    fn kind(&self) -> &'static str;

    /// Read a file at a workspace-relative path. Implementations must
    /// reject any path that would escape the workspace root.
    async fn read_file(&self, path: &str) -> anyhow::Result<Vec<u8>>;

    /// Write a file at a workspace-relative path. Creates parent dirs
    /// as needed. Overwrites if it already exists.
    async fn write_file(&self, path: &str, bytes: &[u8]) -> anyhow::Result<()>;

    /// Delete a file at a workspace-relative path. No-op if missing.
    async fn remove_file(&self, path: &str) -> anyhow::Result<()>;

    /// List entries directly under a workspace-relative directory.
    /// Returns names (not full paths); empty for a missing or empty
    /// directory.
    async fn list_dir(&self, path: &str) -> anyhow::Result<Vec<String>>;

    /// Does a workspace-relative path exist?
    async fn exists(&self, path: &str) -> anyhow::Result<bool>;
}

/// File operations rooted at a host directory — the only implementation
/// today. Every method joins user-supplied paths to `root` and rejects
/// anything that would escape via `..` or absolute components.
pub struct LocalWorkspace {
    root: PathBuf,
}

impl LocalWorkspace {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Resolve a workspace-relative path under `root` and reject paths
    /// that escape (absolute, or any `..` component). Returns an error
    /// rather than a `Result<Option<_>>` so callers can `?` without
    /// branching.
    fn resolve(&self, path: &str) -> anyhow::Result<PathBuf> {
        let candidate = Path::new(path);
        if candidate.is_absolute() {
            anyhow::bail!("workspace path must be relative: {path}");
        }
        for component in candidate.components() {
            if matches!(component, std::path::Component::ParentDir) {
                anyhow::bail!("workspace path escapes root: {path}");
            }
        }
        Ok(self.root.join(candidate))
    }
}

#[async_trait]
impl Workspace for LocalWorkspace {
    fn kind(&self) -> &'static str {
        "local"
    }

    async fn read_file(&self, path: &str) -> anyhow::Result<Vec<u8>> {
        let full = self.resolve(path)?;
        Ok(fs::read(&full).await?)
    }

    async fn write_file(&self, path: &str, bytes: &[u8]) -> anyhow::Result<()> {
        let full = self.resolve(path)?;
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(&full, bytes).await?;
        Ok(())
    }

    async fn remove_file(&self, path: &str) -> anyhow::Result<()> {
        let full = self.resolve(path)?;
        match fs::remove_file(&full).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    async fn list_dir(&self, path: &str) -> anyhow::Result<Vec<String>> {
        let full = self.resolve(path)?;
        let mut out = Vec::new();
        let mut rd = match fs::read_dir(&full).await {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(out),
            Err(e) => return Err(e.into()),
        };
        while let Some(entry) = rd.next_entry().await? {
            if let Some(name) = entry.file_name().to_str() {
                out.push(name.to_string());
            }
        }
        out.sort();
        Ok(out)
    }

    async fn exists(&self, path: &str) -> anyhow::Result<bool> {
        let full = self.resolve(path)?;
        Ok(fs::try_exists(&full).await.unwrap_or(false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        let mut p = std::env::temp_dir();
        p.push(format!("travis-workspace-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    #[tokio::test]
    async fn local_roundtrip() {
        let ws = LocalWorkspace::new(temp_root());
        ws.write_file("a/b/c.txt", b"hello").await.unwrap();
        assert!(ws.exists("a/b/c.txt").await.unwrap());
        let bytes = ws.read_file("a/b/c.txt").await.unwrap();
        assert_eq!(&bytes, b"hello");
        let entries = ws.list_dir("a/b").await.unwrap();
        assert_eq!(entries, vec!["c.txt".to_string()]);
        ws.remove_file("a/b/c.txt").await.unwrap();
        assert!(!ws.exists("a/b/c.txt").await.unwrap());
    }

    #[tokio::test]
    async fn rejects_absolute_path() {
        let ws = LocalWorkspace::new(temp_root());
        let result = ws.read_file("/etc/passwd").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn rejects_parent_traversal() {
        let ws = LocalWorkspace::new(temp_root());
        let result = ws.read_file("../escape.txt").await;
        assert!(result.is_err());
    }
}
