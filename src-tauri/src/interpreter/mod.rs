//! Code interpreter — Pyodide running in a hidden Tauri webview window.
//!
//! Travis v0.14.0's escape hatch for any document task that doesn't fit
//! a hardcoded template. The LLM writes Python in the moment with
//! access to user-attached documents; the interpreter executes it
//! sandboxed inside a WASM CPython, captures stdout/stderr/outputs,
//! and registers any generated files as Travis documents.
//!
//! Architecture:
//! - The hidden window `interpreter` (declared in tauri.conf.json) loads
//!   `interpreter.html` → `src/interpreter/main.tsx` which boots Pyodide
//!   at app start.
//! - Main process calls `run_python` Tauri command (or invokes via the
//!   LLM tool of the same name).
//! - The command serializes the request, emits a `run-python-request`
//!   event with a unique requestId, and waits for the matching
//!   `run-python-result` event.
//! - On result, output files become `Document` rows with
//!   `source = generated_by_travis`.

pub mod cmd;
pub mod state;

pub use cmd::run_python;
pub use state::InterpreterState;
