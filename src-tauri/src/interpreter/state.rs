//! In-process state for in-flight Pyodide executions.
//!
//! When `run_python` is called from Rust, we generate a unique
//! request id, emit an event for the interpreter window, and need to
//! park the calling task until the matching result event arrives.
//! This struct holds the channels per-request so the result listener
//! (in `lib.rs::setup`) can route them.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};

use super::cmd::RunPythonResult;

/// Maps request_id → channel that the calling task is waiting on.
#[derive(Clone)]
pub struct InterpreterState {
    pending: Arc<Mutex<HashMap<String, oneshot::Sender<RunPythonResult>>>>,
    /// Tracks whether Pyodide has reported ready via `interpreter-ready`
    /// event. Calls that come in before ready hang briefly.
    ready: Arc<tokio::sync::watch::Sender<bool>>,
    ready_rx: tokio::sync::watch::Receiver<bool>,
}

impl InterpreterState {
    pub fn new() -> Self {
        let (tx, rx) = tokio::sync::watch::channel(false);
        Self {
            pending: Arc::new(Mutex::new(HashMap::new())),
            ready: Arc::new(tx),
            ready_rx: rx,
        }
    }

    /// Register a pending request and return the receiver to await.
    pub async fn register(&self, request_id: String) -> oneshot::Receiver<RunPythonResult> {
        let (tx, rx) = oneshot::channel();
        let mut guard = self.pending.lock().await;
        guard.insert(request_id, tx);
        rx
    }

    /// Called from the result-event listener: delivers the result to
    /// the awaiting caller.
    pub async fn deliver(&self, result: RunPythonResult) {
        let mut guard = self.pending.lock().await;
        if let Some(tx) = guard.remove(&result.request_id) {
            let _ = tx.send(result);
        }
    }

    /// Mark the interpreter ready (or not).
    pub fn set_ready(&self, ready: bool) {
        let _ = self.ready.send(ready);
    }

    /// Block until the interpreter is ready, with a timeout.
    pub async fn wait_ready(&self, max_wait_secs: u64) -> bool {
        if *self.ready_rx.borrow() {
            return true;
        }
        let mut rx = self.ready_rx.clone();
        let deadline = std::time::Duration::from_secs(max_wait_secs);
        tokio::select! {
            _ = async {
                while rx.changed().await.is_ok() {
                    if *rx.borrow() {
                        break;
                    }
                }
            } => {
                *self.ready_rx.borrow()
            }
            _ = tokio::time::sleep(deadline) => {
                false
            }
        }
    }
}
