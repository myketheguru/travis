//! Pack templates — v0.14.0 Slice 7.
//!
//! When `run_python` produces a great sample-matching document and
//! Taylor confirms it, Travis saves the styling JSON + working code
//! to `pack_template`. Future requests for the same counterparty
//! reuse the saved code instantly — no styling re-analysis, no fresh
//! code generation. Over time, the LLM writes less and less code
//! because more templates accumulate.

pub mod cmd;
pub mod db;

pub use db::PackTemplate;
