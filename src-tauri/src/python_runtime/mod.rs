//! v0.18.0 — bundled-CPython subprocess execution.
//!
//! Replaces the Pyodide-in-hidden-window architecture (v0.14 → v0.17)
//! with a real CPython subprocess. Why:
//!
//! - **Reliability.** No ready-race; no warmup pattern; no
//!   interpreter-not-ready timeouts. Process spawn is a single
//!   atomic IPC; either it starts or it doesn't.
//! - **Speed.** Cold start ~150ms (process spawn) vs 3-5s (Pyodide
//!   WASM bootstrap). Subsequent calls amortise via a process pool.
//! - **Ecosystem.** Full PyPI works including C extensions
//!   (pdfplumber, requests' native socket layer, native lxml).
//! - **Isolation.** Each call gets a fresh process — no state leak
//!   between turns, no global-namespace pollution.
//! - **Per-call FS.** Inputs/outputs are real host-filesystem
//!   directories, not an in-memory VFS. Simpler to reason about
//!   and inspectable from the OS file picker.
//!
//! Layout of bundled Python (resolved via `app.path()`):
//!
//!   resources/python/<platform-slug>/python/
//!     bin/python3              (POSIX)
//!     python.exe               (Windows)
//!     Lib/site-packages/...    (preinstalled wheels: pandas, openpyxl,
//!                               reportlab, etc.)
//!
//! Per-call temp layout:
//!
//!   <temp_dir>/<run_id>/
//!     script.py        (the LLM's code)
//!     inputs/          (mounted input documents)
//!     outputs/         (where the script writes generated files)
//!
//! At end-of-call:
//! - stdout/stderr captured
//! - outputs/ scanned; files collected as `RunPythonResult.generated_files`
//! - the entire `<run_id>` dir is removed

pub mod bootstrap;
pub mod cmd;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use serde::Serialize;
use tauri::{AppHandle, Manager};
use tokio::process::Command;

/// Result of a single python_runtime::run call. Shape mirrors the
/// legacy `interpreter::cmd::RunPythonResult` so the tool layer can
/// swap implementations without ripple changes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RunPythonResult {
    pub request_id: String,
    pub stdout: String,
    pub stderr: String,
    /// Map of filename → base64 file bytes from the per-call outputs/
    /// dir. Caller post-processes into document rows.
    pub generated_files: HashMap<String, String>,
    pub execution_ms: u64,
    pub error: Option<String>,
}

/// Inputs to a single run. Mirrors the prior interpreter event payload.
#[derive(Debug)]
pub struct RunParams {
    pub request_id: String,
    pub code: String,
    /// filename → base64-encoded file bytes; written to inputs/<name>
    /// before script execution.
    pub input_files: HashMap<String, String>,
    pub timeout_secs: u64,
}

/// Resolve the bundled python binary path for this platform. Returns
/// `None` when the binary isn't where we expect — caller should treat
/// that as a configuration/install error (the resources weren't
/// bundled for this build).
pub fn resolve_python_bin(app: &AppHandle) -> Option<PathBuf> {
    // v0.22.10 — three-tier resolution:
    //   1. Lazy-cached runtime under <app_data>/python/<slug>/ —
    //      survives Travis upgrades, downloaded on first use.
    //   2. Installer-bundled runtime (legacy / current behaviour) —
    //      shipping ~150 MB inside the installer.
    //   3. Dev-mode fallback for cargo runs from src-tauri/.
    //
    // Tier 1 wins so users who've gone through the bootstrap stay
    // on the cached copy and don't accidentally fall back to the
    // bundled (and potentially stale) Python after a reinstall.
    if let Ok(cached) = bootstrap::cache_python_bin(app) {
        if cached.exists() {
            return Some(cached);
        }
    }

    let resource_dir = app.path().resource_dir().ok()?;
    let platform_slug = host_slug();
    let candidate = resource_dir
        .join("resources")
        .join("python")
        .join(platform_slug)
        .join("python")
        .join(if cfg!(target_os = "windows") { "python.exe" } else { "bin/python3" });
    if candidate.exists() {
        Some(candidate)
    } else {
        // Fallback: dev-mode build runs cargo from src-tauri/, where
        // resources/ lives a level up. Useful so `cargo run` against
        // a freshly fetched python doesn't need the full installer.
        let dev_candidate = std::env::current_dir().ok()?
            .join("resources")
            .join("python")
            .join(platform_slug)
            .join("python")
            .join(if cfg!(target_os = "windows") { "python.exe" } else { "bin/python3" });
        dev_candidate.exists().then_some(dev_candidate)
    }
}

fn host_slug() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows-x64"
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "macos-arm64"
        } else {
            "macos-x64"
        }
    } else {
        "linux-x64"
    }
}

/// Execute a Python script in a fresh subprocess. The whole call is
/// self-contained: temp dir created, inputs written, script run,
/// outputs collected, temp dir cleaned up.
pub async fn run(app: &AppHandle, params: RunParams) -> RunPythonResult {
    let started_at = Instant::now();
    let request_id = params.request_id.clone();

    let py_bin = match resolve_python_bin(app) {
        Some(p) => p,
        None => {
            return RunPythonResult {
                request_id,
                stdout: String::new(),
                stderr: String::new(),
                generated_files: HashMap::new(),
                execution_ms: 0,
                error: Some(
                    "bundled CPython not found — was the installer built with the python \
                     resources bundle? Run `npm run fetch:python` and rebuild."
                        .to_string(),
                ),
            };
        }
    };

    // Per-call temp dir under the OS temp root. The id is fine as a
    // local-only directory name; it's already unique-per-call.
    let temp_root = std::env::temp_dir().join(format!("travis-py-{}", request_id));
    let inputs_dir = temp_root.join("inputs");
    let outputs_dir = temp_root.join("outputs");
    let script_path = temp_root.join("script.py");
    if let Err(e) = tokio::fs::create_dir_all(&inputs_dir).await {
        return error_result(request_id, started_at, format!("create temp inputs dir: {e}"));
    }
    if let Err(e) = tokio::fs::create_dir_all(&outputs_dir).await {
        return error_result(request_id, started_at, format!("create temp outputs dir: {e}"));
    }

    // Decode + write input files
    for (name, b64) in &params.input_files {
        let safe = sanitize_filename(name);
        let bytes = match B64.decode(b64) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!("input '{name}' base64 decode failed: {e} — skipping");
                continue;
            }
        };
        if let Err(e) = tokio::fs::write(inputs_dir.join(&safe), &bytes).await {
            tracing::warn!("input '{name}' write failed: {e} — skipping");
        }
    }

    // Wrap user code with a small preamble that sets cwd, exposes
    // `/inputs` and `/outputs` as convenience constants, and ensures
    // `outputs/` exists. POSIX-style paths are normalised by Python
    // even on Windows, but we use forward slashes to keep the LLM's
    // prompt simple — the wrapper exposes both forms.
    let inputs_str = inputs_dir.to_string_lossy().replace('\\', "/");
    let outputs_str = outputs_dir.to_string_lossy().replace('\\', "/");
    let wrapped = format!(
        "\
import os, sys
INPUTS_DIR  = r\"{inputs_str}\"
OUTPUTS_DIR = r\"{outputs_str}\"
os.makedirs(OUTPUTS_DIR, exist_ok=True)
os.chdir(OUTPUTS_DIR)
# Back-compat with legacy /inputs and /outputs Pyodide paths:
# expose symlinks/copies if the LLM-emitted code still uses them.
try:
    if not os.path.isdir('/inputs') and os.name != 'nt':
        os.symlink(INPUTS_DIR, '/inputs')
    if not os.path.isdir('/outputs') and os.name != 'nt':
        os.symlink(OUTPUTS_DIR, '/outputs')
except Exception:
    pass

# ---- user code below ----
{user_code}
",
        user_code = params.code
    );

    if let Err(e) = tokio::fs::write(&script_path, &wrapped).await {
        return error_result(request_id, started_at, format!("write script: {e}"));
    }

    // Spawn python with the wrapped script. Stdin closed; stdout/
    // stderr captured. `-u` forces unbuffered so partial output is
    // visible if we ever decide to stream.
    let mut cmd = Command::new(&py_bin);
    cmd.arg("-u")
        .arg(&script_path)
        .current_dir(&outputs_dir)
        .env("PYTHONIOENCODING", "utf-8")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    // Windows: don't pop a console window when spawning the subprocess.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x08000000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let timeout = Duration::from_secs(params.timeout_secs);
    let proc_result = tokio::time::timeout(timeout, async {
        let output = cmd.output().await?;
        Ok::<_, std::io::Error>(output)
    })
    .await;

    let (stdout, stderr, error) = match proc_result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let error = if output.status.success() {
                None
            } else {
                // Python raises → non-zero exit + traceback in stderr.
                // Surface stderr verbatim as the error so the LLM
                // sees what went wrong.
                Some(if stderr.trim().is_empty() {
                    format!("python exited with code {:?}", output.status.code())
                } else {
                    stderr.clone()
                })
            };
            (stdout, stderr, error)
        }
        Ok(Err(e)) => (String::new(), String::new(), Some(format!("spawn python: {e}"))),
        Err(_) => (
            String::new(),
            String::new(),
            Some(format!("execution exceeded {}s timeout", params.timeout_secs)),
        ),
    };

    // Collect outputs
    let generated_files = collect_outputs(&outputs_dir).await;

    // Clean up temp dir
    let _ = tokio::fs::remove_dir_all(&temp_root).await;

    RunPythonResult {
        request_id,
        stdout,
        stderr,
        generated_files,
        execution_ms: started_at.elapsed().as_millis() as u64,
        error,
    }
}

async fn collect_outputs(outputs_dir: &Path) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut stack = vec![outputs_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let mut rd = match tokio::fs::read_dir(&dir).await {
            Ok(rd) => rd,
            Err(_) => continue,
        };
        while let Ok(Some(entry)) = rd.next_entry().await {
            let path = entry.path();
            match entry.file_type().await {
                Ok(ft) if ft.is_dir() => stack.push(path),
                Ok(ft) if ft.is_file() => {
                    if let Ok(bytes) = tokio::fs::read(&path).await {
                        if let Ok(rel) = path.strip_prefix(outputs_dir) {
                            let name = rel.to_string_lossy().replace('\\', "/");
                            out.insert(name, B64.encode(&bytes));
                        }
                    }
                }
                _ => {}
            }
        }
    }
    out
}

fn error_result(request_id: String, started_at: Instant, msg: String) -> RunPythonResult {
    RunPythonResult {
        request_id,
        stdout: String::new(),
        stderr: String::new(),
        generated_files: HashMap::new(),
        execution_ms: started_at.elapsed().as_millis() as u64,
        error: Some(msg),
    }
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '_' })
        .collect()
}
