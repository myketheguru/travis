//! Travis Cases — v0.14.0 Slice 6.
//!
//! A "case" is a named multi-session work unit that survives across
//! conversations. Taylor's PS 89 invoice #3 reconciliation in the
//! Claude.ai conversation spanned 3 days with mid-stream corrections
//! ("COO says we already billed Transformational 1 and 2"); the
//! 30-minute working-memory TTL is way too short for that.
//!
//! Cases hold:
//! - A rolling 2–4 sentence summary the LLM maintains
//! - Linked artifacts: decisions made, documents touched,
//!   reconciliation tables, generated outputs
//! - A status (open / paused / closed)
//!
//! When a conversation references an active case, that case's summary
//! and recent artifacts get injected into the prompt — same pattern as
//! `initiatives` but more structured.

pub mod cmd;
pub mod db;

pub use db::{Case, CaseArtifact};
