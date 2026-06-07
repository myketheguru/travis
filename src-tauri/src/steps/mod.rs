//! Step streaming — v0.14.0 Slice 2.
//!
//! Every tool call, action handler, and code execution emits a stream
//! of typed events the frontend renders inline in the chat surface
//! as named substeps with checkmarks (Claude.ai style). Steps are
//! also persisted to the `step` table so reopening a conversation
//! re-renders the full history.
//!
//! The event flow:
//!   1. `Step::start(...)` — emits Started, persists row with status=running
//!   2. (optional) `step.note(text)` — appends to notes_json + emits Note
//!   3. `step.complete_ok(summary)` or `step.complete_err(error)`
//!      — sets status, persists, emits Result + Completed
//!
//! The Step struct is RAII-style: dropping without completing marks
//! the row as `cancelled` so half-finished work is visible rather
//! than silently lost.

pub mod cmd;
pub mod model;
pub mod stream;

pub use model::{StepEvent, StepKind, StepStatus};
pub use stream::Step;
