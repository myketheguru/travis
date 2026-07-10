//! Piper TTS via subprocess.
//!
//! Runs the bundled piper binary against a text input and returns the
//! generated WAV bytes. All work is fire-and-forget from the frontend's
//! perspective: it invokes `piper_speak`, we write text to piper's
//! stdin, read WAV from stdout, and return base64-encoded bytes.
//!
//! Layout (matches scripts/fetch-piper.mjs):
//!   resources/piper/
//!     piper                       (binary; piper.exe on Windows)
//!     en_US-amy-medium.onnx       (voice model)
//!     en_US-amy-medium.onnx.json  (voice config)
//!
//! If any file is missing (dev build that skipped predev, or an
//! unsupported host platform in fetch-piper.mjs), `synthesize` returns
//! an error and the frontend falls back to speechSynthesis without
//! interrupting the user.

use std::path::PathBuf;
use std::process::Stdio;

use anyhow::{anyhow, Context, Result};
use tauri::{AppHandle, Manager};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const VOICE_MODEL_FILENAME: &str = "en_US-amy-medium.onnx";

fn piper_binary_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "piper.exe"
    } else {
        "piper"
    }
}

fn resolve_from(root: PathBuf) -> Option<(PathBuf, PathBuf)> {
    let bin = root.join("resources").join("piper").join(piper_binary_name());
    let model = root.join("resources").join("piper").join(VOICE_MODEL_FILENAME);
    if bin.exists() && model.exists() {
        Some((bin, model))
    } else {
        None
    }
}

/// Locate the bundled piper binary + voice model, or fall back to the
/// dev-mode layout (running `cargo run` from `src-tauri/`).
fn resolve_piper(app: &AppHandle) -> Option<(PathBuf, PathBuf)> {
    if let Ok(dir) = app.path().resource_dir() {
        if let Some(found) = resolve_from(dir) {
            return Some(found);
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        if let Some(found) = resolve_from(cwd) {
            return Some(found);
        }
    }
    None
}

/// Run piper against `text` and return the produced WAV bytes.
///
/// Contract: piper writes a single WAV to stdout when `--output-raw`
/// is NOT set (default). We consume stdout fully and return it.
pub async fn synthesize(app: &AppHandle, text: &str) -> Result<Vec<u8>> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("empty text"));
    }
    let (bin, model) = resolve_piper(app)
        .ok_or_else(|| anyhow!("piper binary or voice model not found"))?;

    let mut child = Command::new(&bin)
        .arg("--model")
        .arg(&model)
        .arg("--output_file")
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("spawn piper binary at {}", bin.display()))?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(trimmed.as_bytes())
            .await
            .context("write piper stdin")?;
        // Explicitly drop the handle so piper sees EOF and starts
        // producing output.
        drop(stdin);
    }

    let output = child
        .wait_with_output()
        .await
        .context("wait piper subprocess")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "piper exited with {}: {}",
            output.status,
            stderr.trim()
        ));
    }
    if output.stdout.is_empty() {
        return Err(anyhow!("piper produced no output"));
    }
    Ok(output.stdout)
}

/// Cheap probe used by the frontend Settings panel to decide whether
/// to offer the "Travis voice" preference at all vs. hiding it when
/// the bundle didn't ship the assets.
pub fn is_available(app: &AppHandle) -> bool {
    resolve_piper(app).is_some()
}
