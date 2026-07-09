//! Cached WhisperContext — v0.28.14.
//!
//! Previously every transcribe call did `WhisperContext::new_with_params`
//! which reads the model file off disk + parses it, costing 1-3 seconds
//! per call. First-utterance response felt broken because of this.
//! Now: load once, hold in a Mutex, reuse forever. Warm-up is called on
//! app boot so the first user utterance transcribes at full speed.

use std::sync::{Arc, Mutex};

use whisper_rs::{WhisperContext, WhisperContextParameters};

#[derive(Clone, Default)]
pub struct WhisperCache {
    inner: Arc<Mutex<Option<Arc<WhisperContext>>>>,
}

impl WhisperCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get (or load) the cached WhisperContext. Blocks briefly on the
    /// mutex; the actual whisper.cpp init only runs once.
    pub fn get_or_load(&self, model_path: &str) -> Result<Arc<WhisperContext>, String> {
        let mut guard = self.inner.lock().map_err(|e| format!("whisper cache lock: {e}"))?;
        if let Some(ctx) = guard.as_ref() {
            return Ok(ctx.clone());
        }
        let ctx = WhisperContext::new_with_params(model_path, WhisperContextParameters::default())
            .map_err(|e| format!("load whisper: {e}"))?;
        let arc = Arc::new(ctx);
        *guard = Some(arc.clone());
        tracing::info!("[whisper] context cached — subsequent transcribes are warm");
        Ok(arc)
    }

    /// Clear the cache (e.g. if the model file is replaced).
    #[allow(dead_code)]
    pub fn clear(&self) {
        if let Ok(mut g) = self.inner.lock() {
            *g = None;
        }
    }
}
