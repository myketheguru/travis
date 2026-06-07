//! Document substrate — first-class file storage for Travis.
//!
//! Tauri ingests files via drag-drop or explicit attach; this module
//! is responsible for: hashing them, storing them under the managed
//! app_data path (content-addressed for dedup), and persisting their
//! metadata + entity links to SQLite.
//!
//! Extraction (turning a PDF into structured data) is Slice 3 — this
//! module produces a `Document` row with `ingest_status = 'pending'`,
//! and the extractor walks pending rows.
//!
//! See [[feedback-docs-first]] for the design context.

pub mod cmd;
pub mod db;
pub mod extract;
pub mod storage;
pub mod styling;

pub use db::{Document, DocumentLink, IngestStatus, Source};
