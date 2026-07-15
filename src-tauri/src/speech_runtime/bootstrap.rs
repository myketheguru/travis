//! Whisper model download + verification.
//!
//! Reuses the "getting additional resources" UX from Python bootstrap
//! by emitting the SAME `runtime-progress` events — so the frontend
//! loader picks up automatically without needing a second overlay.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

use crate::speech_runtime::cache_model_path;

/// v0.28.59 — default model switched from `base.en` (74 MB) to
/// `tiny.en` (39 MB) on Gemini's recommendation and after profiling
/// showed base was the dominant cost of the post-utterance wait.
/// tiny.en runs ~2-3x faster on CPU (well under 500ms for a typical
/// 5-second command) and its accuracy on short command-style
/// utterances is within a rounding error of base for our domain.
/// If a specific user's speech patterns hit tiny's floor, we can
/// expose a Settings-level "prefer accuracy over speed" toggle in a
/// follow-up.
pub const DEFAULT_MODEL: &str = "ggml-tiny.en.bin";

/// Where the whisper.cpp release models live. HuggingFace mirror is
/// the canonical source used by the whisper.cpp project itself.
const MODEL_URL_BASE: &str =
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapProgress {
    pub phase: &'static str, // "downloading" | "ready" | "error"
    pub pct: f32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Default)]
pub struct BootstrapHandle(Arc<AtomicBool>);
impl BootstrapHandle {
    pub fn cancel(&self) { self.0.store(true, Ordering::SeqCst); }
    fn cancelled(&self) -> bool { self.0.load(Ordering::SeqCst) }
}

fn emit(app: &AppHandle, p: BootstrapProgress) {
    // Reuse the shared runtime-progress event channel so the existing
    // ResourceLoader overlay picks it up without needing a second one.
    if let Err(e) = app.emit("runtime-progress", &p) {
        tracing::warn!("emit runtime-progress failed: {e}");
    }
}

pub async fn ensure_ready(
    app: &AppHandle,
    handle: BootstrapHandle,
    model: &str,
) -> Result<PathBuf, String> {
    let target = cache_model_path(app, model)?;
    if crate::speech_runtime::model_ready(app, model) {
        return Ok(target);
    }
    // Prepare parent dir + wipe partial.
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create speech cache: {e}"))?;
    }
    if target.exists() {
        let _ = std::fs::remove_file(&target);
    }

    // v0.25 (task 327) — the installer bundles the model as a Tauri
    // resource. Copy it into the cache dir on first run instead of
    // downloading. Falls through to HuggingFace download only when the
    // resource is missing (dev builds, unbundled sources).
    if let Ok(bundled) = app
        .path()
        .resolve(
            format!("resources/whisper/{model}"),
            tauri::path::BaseDirectory::Resource,
        )
    {
        if bundled.exists() {
            match std::fs::copy(&bundled, &target) {
                Ok(_) => {
                    emit(
                        app,
                        BootstrapProgress {
                            phase: "ready",
                            pct: 100.0,
                            message: "Ready".into(),
                            error: None,
                        },
                    );
                    return Ok(target);
                }
                Err(e) => {
                    tracing::warn!("speech: bundled model copy failed ({e}); falling back to download");
                }
            }
        }
    }

    emit(
        app,
        BootstrapProgress {
            phase: "downloading",
            pct: 0.0,
            message: "Travis is getting additional resources to continue".into(),
            error: None,
        },
    );

    let url = format!("{MODEL_URL_BASE}/{model}");
    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| format!("http client: {e}"))?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("start download: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("HTTP {}: {url}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(0);
    let mut stream = resp.bytes_stream();

    let tmp = target.with_extension("bin.part");
    let mut file = tokio::fs::File::create(&tmp)
        .await
        .map_err(|e| format!("open tmp: {e}"))?;
    let mut downloaded: u64 = 0;
    let mut last_emit_pct: f32 = -1.0;
    while let Some(chunk) = stream.next().await {
        if handle.cancelled() {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err("cancelled".into());
        }
        let bytes = chunk.map_err(|e| format!("chunk: {e}"))?;
        downloaded += bytes.len() as u64;
        file.write_all(&bytes)
            .await
            .map_err(|e| format!("write: {e}"))?;
        if total > 0 {
            let pct = (downloaded as f32 / total as f32) * 100.0;
            if pct - last_emit_pct >= 1.0 {
                emit(
                    app,
                    BootstrapProgress {
                        phase: "downloading",
                        pct,
                        message: "Travis is getting additional resources to continue"
                            .into(),
                        error: None,
                    },
                );
                last_emit_pct = pct;
            }
        }
    }
    file.flush().await.ok();
    drop(file);

    // Atomically move partial into place so `model_ready` sees a
    // complete file even if we crash between here and next boot.
    tokio::fs::rename(&tmp, &target)
        .await
        .map_err(|e| format!("rename tmp: {e}"))?;

    emit(
        app,
        BootstrapProgress {
            phase: "ready",
            pct: 100.0,
            message: "Ready".into(),
            error: None,
        },
    );
    Ok(target)
}
