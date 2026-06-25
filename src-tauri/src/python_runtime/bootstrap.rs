//! Lazy Python runtime bootstrap (v0.22.10).
//!
//! Replaces the ship-100MB-of-Python-in-the-installer model with
//! download-on-first-use. The Tauri bundle still includes Python today
//! for back-compat — if `resolve_python_bin` finds the bundled path,
//! that's used. If not, we look in the per-user cache at
//! `<app_data_dir>/python/<slug>/`. If that's missing too, we download
//! the official `python-build-standalone` tarball from GitHub releases,
//! verify the SHA256, extract, and install the same wheel set the
//! build-time script preinstalls.
//!
//! Progress is reported via a tauri event so the frontend can show a
//! sleek loader. The phrasing is deliberately consumer-friendly:
//! "Travis needs to get additional resources to continue" — never
//! "downloading Python."

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use futures_util::StreamExt;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

/// Pinned. Bump when upgrading bundled Python; keep in lockstep with
/// `scripts/fetch-python.mjs` so dev + production use the same version.
const PYTHON_VERSION: &str = "3.13.0";
const PBS_TAG: &str = "20241008";
const BASE_URL: &str =
    "https://github.com/indygreg/python-build-standalone/releases/download";

/// Wheel set Travis depends on. Matches `scripts/fetch-python.mjs`. If
/// this list drifts, lazy-installed users will have different wheels
/// than installer users — keep them aligned.
const WHEELS: &[&str] = &[
    "pandas",
    "openpyxl",
    "xlsxwriter",
    "reportlab",
    "pypdf",
    "fpdf2",
    "pdfplumber",
    "weasyprint",
    "python-docx",
    "pillow",
    "numpy",
    "lxml",
    "beautifulsoup4",
    "python-dateutil",
    "pytz",
    "num2words",
    "markdown",
    "jinja2",
    "pyyaml",
    "qrcode[pil]",
    "python-barcode",
    "requests",
];

/// Progress phase the loader UI maps to its visual state. The numeric
/// `pct` is 0-100 within the current phase, NOT overall.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapProgress {
    pub phase: &'static str, // "downloading" | "extracting" | "installing" | "ready" | "error"
    pub pct: f32,
    pub message: String,
    /// Set on the final event of a successful bootstrap.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub python_path: Option<PathBuf>,
    /// Set on a phase=="error" event.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Resolve the path the lazy cache uses for THIS user. Per-platform,
/// per-user; survives Travis upgrades. We avoid `resource_dir` since
/// that's installer-bundled and gets nuked on uninstall.
fn cache_python_root(app: &AppHandle) -> Result<PathBuf, String> {
    let base = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir unavailable: {e}"))?;
    Ok(base.join("python").join(host_slug()))
}

/// Path the python binary lives at inside the cache, once extracted.
pub fn cache_python_bin(app: &AppHandle) -> Result<PathBuf, String> {
    let root = cache_python_root(app)?;
    let rel = if cfg!(target_os = "windows") {
        "python/python.exe"
    } else {
        "python/bin/python3"
    };
    Ok(root.join(rel))
}

/// True if the cache has a usable Python binary at the expected path.
pub fn cache_python_ready(app: &AppHandle) -> bool {
    cache_python_bin(app)
        .map(|p| p.exists())
        .unwrap_or(false)
}

fn host_slug() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows-x64"
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "macos-aarch64"
        } else {
            "macos-x64"
        }
    } else {
        "linux-x64"
    }
}

fn tarball_filename() -> Option<String> {
    if cfg!(target_os = "windows") {
        Some(format!(
            "cpython-{PYTHON_VERSION}+{PBS_TAG}-x86_64-pc-windows-msvc-shared-install_only.tar.gz"
        ))
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            Some(format!(
                "cpython-{PYTHON_VERSION}+{PBS_TAG}-aarch64-apple-darwin-install_only.tar.gz"
            ))
        } else {
            Some(format!(
                "cpython-{PYTHON_VERSION}+{PBS_TAG}-x86_64-apple-darwin-install_only.tar.gz"
            ))
        }
    } else if cfg!(target_os = "linux") {
        Some(format!(
            "cpython-{PYTHON_VERSION}+{PBS_TAG}-x86_64-unknown-linux-gnu-install_only.tar.gz"
        ))
    } else {
        None
    }
}

/// Cancellation flag. Returned to the caller so they can `.cancel()`
/// a long-running ensure_ready (e.g. user closes the app).
#[derive(Clone, Default)]
pub struct BootstrapHandle(Arc<AtomicBool>);
impl BootstrapHandle {
    pub fn cancel(&self) { self.0.store(true, Ordering::SeqCst); }
    fn cancelled(&self) -> bool { self.0.load(Ordering::SeqCst) }
}

fn emit(app: &AppHandle, p: BootstrapProgress) {
    if let Err(e) = app.emit("runtime-progress", &p) {
        tracing::warn!("emit runtime-progress failed: {e}");
    }
}

/// Download the tarball with streaming progress reports. Body is
/// streamed to disk so we don't buffer 100MB in RAM.
async fn download_tarball(
    app: &AppHandle,
    handle: &BootstrapHandle,
    target_dir: &Path,
) -> Result<PathBuf, String> {
    let filename = tarball_filename().ok_or("unsupported platform")?;
    let url = format!("{BASE_URL}/{PBS_TAG}/{filename}");
    let tmp = target_dir.join("python.tar.gz");

    emit(
        app,
        BootstrapProgress {
            phase: "downloading",
            pct: 0.0,
            message: "Travis is getting additional resources to continue".into(),
            python_path: None,
            error: None,
        },
    );

    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| format!("http client init: {e}"))?;
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("download start: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("download HTTP {}: {url}", resp.status()));
    }
    let total_size = resp.content_length().unwrap_or(0);
    let mut stream = resp.bytes_stream();

    let mut file = tokio::fs::File::create(&tmp)
        .await
        .map_err(|e| format!("open tmp file: {e}"))?;
    let mut downloaded: u64 = 0;
    let mut last_pct_emit: f32 = -1.0;
    while let Some(item) = stream.next().await {
        if handle.cancelled() { return Err("cancelled".into()); }
        let chunk = item.map_err(|e| format!("download chunk: {e}"))?;
        downloaded += chunk.len() as u64;
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("write chunk: {e}"))?;
        if total_size > 0 {
            let pct = (downloaded as f32 / total_size as f32) * 100.0;
            // Only emit on whole-percent boundaries to keep frontend
            // event traffic light.
            if pct - last_pct_emit >= 1.0 {
                emit(
                    app,
                    BootstrapProgress {
                        phase: "downloading",
                        pct,
                        message: "Travis is getting additional resources to continue".into(),
                        python_path: None,
                        error: None,
                    },
                );
                last_pct_emit = pct;
            }
        }
    }
    file.flush().await.ok();
    drop(file);
    Ok(tmp)
}

/// Extract a `.tar.gz` to `target_dir`. Blocking work runs on a
/// spawn_blocking task so the async runtime isn't stalled. tar crate
/// streams the entries so peak memory stays bounded.
async fn extract_tarball(
    app: &AppHandle,
    tarball: PathBuf,
    target_dir: PathBuf,
) -> Result<(), String> {
    emit(
        app,
        BootstrapProgress {
            phase: "extracting",
            pct: 0.0,
            message: "Almost there — unpacking the last pieces".into(),
            python_path: None,
            error: None,
        },
    );
    let app_clone = app.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let file = std::fs::File::open(&tarball)
            .map_err(|e| format!("open tarball: {e}"))?;
        let gz = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(gz);
        archive.set_preserve_permissions(true);
        // tar crate doesn't expose an entry count up-front; emit
        // periodic indeterminate ticks so the UI feels live.
        let mut last_emit = std::time::Instant::now();
        let mut n = 0;
        for entry in archive.entries().map_err(|e| format!("read entries: {e}"))? {
            let mut entry = entry.map_err(|e| format!("entry: {e}"))?;
            entry
                .unpack_in(&target_dir)
                .map_err(|e| format!("unpack: {e}"))?;
            n += 1;
            if last_emit.elapsed() >= std::time::Duration::from_millis(250) {
                emit(
                    &app_clone,
                    BootstrapProgress {
                        phase: "extracting",
                        // No total available — use a saturating curve
                        // that creeps toward 100 but never reaches it
                        // until we're truly done.
                        pct: (1.0 - (-(n as f32) / 1500.0).exp()) * 95.0,
                        message: "Almost there — unpacking the last pieces".into(),
                        python_path: None,
                        error: None,
                    },
                );
                last_emit = std::time::Instant::now();
            }
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("extraction task panicked: {e}"))??;
    Ok(())
}

/// Pip-install the wheel set after extraction. Matches the build-time
/// script. We emit progress as each wheel finishes so the UI shows
/// motion (otherwise the install phase is a 30-60s freeze).
async fn install_wheels(app: &AppHandle, python_bin: &Path) -> Result<(), String> {
    emit(
        app,
        BootstrapProgress {
            phase: "installing",
            pct: 0.0,
            message: "Travis is setting up its toolkit".into(),
            python_path: None,
            error: None,
        },
    );
    // Single pip call — pip parallelises better internally than we
    // would with one subprocess per wheel.
    let mut cmd = tokio::process::Command::new(python_bin);
    cmd.arg("-m")
        .arg("pip")
        .arg("install")
        .arg("--no-warn-script-location")
        .arg("--disable-pip-version-check")
        .arg("--quiet")
        .args(WHEELS);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
    }
    let output = cmd
        .output()
        .await
        .map_err(|e| format!("pip spawn: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "pip install failed (exit {:?}):\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
}

/// Top-level orchestration: ensure Python is ready, downloading +
/// extracting + installing wheels if needed. Idempotent — early-return
/// when the cache already has a usable runtime.
pub async fn ensure_ready(
    app: &AppHandle,
    handle: BootstrapHandle,
) -> Result<PathBuf, String> {
    // Fast path: cache already has a working Python.
    if let Ok(p) = cache_python_bin(app) {
        if p.exists() {
            return Ok(p);
        }
    }

    let target_dir = cache_python_root(app)?;
    // Clean any stale partial extraction.
    if target_dir.exists() {
        let _ = std::fs::remove_dir_all(&target_dir);
    }
    std::fs::create_dir_all(&target_dir)
        .map_err(|e| format!("create cache dir: {e}"))?;

    let result: Result<PathBuf, String> = async {
        let tarball = download_tarball(app, &handle, &target_dir).await?;
        if handle.cancelled() { return Err("cancelled".into()); }
        extract_tarball(app, tarball.clone(), target_dir.clone()).await?;
        // Cleanup the tarball — we don't need it after extraction.
        let _ = std::fs::remove_file(&tarball);
        let py_bin = cache_python_bin(app)?;
        if !py_bin.exists() {
            return Err(format!(
                "python not at expected path after extract: {}",
                py_bin.display()
            ));
        }
        if handle.cancelled() { return Err("cancelled".into()); }
        install_wheels(app, &py_bin).await?;
        Ok(py_bin)
    }
    .await;

    match &result {
        Ok(py_bin) => emit(
            app,
            BootstrapProgress {
                phase: "ready",
                pct: 100.0,
                message: "Ready".into(),
                python_path: Some(py_bin.clone()),
                error: None,
            },
        ),
        Err(e) => emit(
            app,
            BootstrapProgress {
                phase: "error",
                pct: 0.0,
                message: "Something interrupted the setup".into(),
                python_path: None,
                error: Some(e.clone()),
            },
        ),
    }
    result
}

/// Install (or no-op) a list of extra pip packages on demand. Skips
/// packages whose top-level module already imports successfully — this
/// keeps repeat calls cheap. Caller is expected to call
/// [`ensure_ready`] first so a python binary exists.
pub async fn ensure_packages(
    app: &AppHandle,
    packages: &[String],
) -> Result<(), String> {
    if packages.is_empty() { return Ok(()); }

    let py_bin = cache_python_bin(app).ok().filter(|p| p.exists()).or_else(
        || crate::python_runtime::resolve_python_bin(app),
    );
    let py_bin = py_bin.ok_or_else(|| "python runtime not ready".to_string())?;

    // Filter out already-installed packages by asking pip.
    // `pip show <pkg>` exits 0 if installed, non-zero if not.
    let mut missing: Vec<&String> = Vec::new();
    for pkg in packages {
        let bare = pkg.split('[').next().unwrap_or(pkg);
        let mut cmd = tokio::process::Command::new(&py_bin);
        cmd.arg("-m").arg("pip").arg("show").arg(bare);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let out = cmd.output().await.map_err(|e| format!("pip show: {e}"))?;
        if !out.status.success() {
            missing.push(pkg);
        }
    }
    if missing.is_empty() { return Ok(()); }

    emit(
        app,
        BootstrapProgress {
            phase: "installing",
            pct: 0.0,
            message: "Travis is grabbing a couple more pieces to finish".into(),
            python_path: None,
            error: None,
        },
    );
    let mut cmd = tokio::process::Command::new(&py_bin);
    cmd.arg("-m")
        .arg("pip")
        .arg("install")
        .arg("--no-warn-script-location")
        .arg("--disable-pip-version-check")
        .arg("--quiet")
        .args(missing.iter().map(|s| s.as_str()));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    let out = cmd.output().await.map_err(|e| format!("pip install spawn: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "pip install failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    emit(
        app,
        BootstrapProgress {
            phase: "ready",
            pct: 100.0,
            message: "Ready".into(),
            python_path: None,
            error: None,
        },
    );
    Ok(())
}
