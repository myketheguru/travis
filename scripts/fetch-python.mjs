#!/usr/bin/env node
/**
 * Build-time fetcher for bundled portable CPython. Drops Pyodide.
 *
 * The bundled distribution is indygreg's python-build-standalone:
 *   - Real CPython, not WASM. Subprocess-launchable from Rust.
 *   - Cross-platform: Windows x64, macOS arm64, macOS x64, Linux x64.
 *   - Includes pip. Full PyPI ecosystem works including C extensions.
 *   - "install_only" tarball strips the build artefacts → smaller bundle.
 *
 * Layout written under `src-tauri/resources/python/<target>/`:
 *   bin/python   (or python.exe on Windows)
 *   Lib/site-packages/...
 *   ...
 *
 * Tauri's resources bundling copies this into the installer.
 * `python_runtime` (Rust) resolves it via `app.path().resolve(...)`
 * at startup and spawns it per run_python call.
 *
 * The fetcher is idempotent: re-running with the runtime already on
 * disk is a no-op. Re-targeting a different platform overwrites the
 * old one (CI is fine; on dev you can only bundle for the host).
 *
 * Run via `npm run fetch:python` or automatically before tauri build
 * via the `tauri-cli` build hooks once we wire those in.
 */
import { createWriteStream, existsSync, mkdirSync, rmSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { pipeline } from "node:stream/promises";
import { spawnSync } from "node:child_process";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "..");
const resourcesRoot = join(repoRoot, "src-tauri", "resources");
const pythonRoot = join(resourcesRoot, "python");

// Pinned to a specific release tag for reproducibility. Bump when
// upgrading the bundled Python version; see python-build-standalone
// releases for current tags.
const PYTHON_VERSION = "3.13.0";
const PBS_TAG = "20241008";
const BASE_URL = `https://github.com/indygreg/python-build-standalone/releases/download/${PBS_TAG}`;

// One target triple per platform. Detect host; build script for other
// platforms is invoked separately by CI matrix.
function targetForHost() {
  if (process.platform === "win32") {
    return {
      slug: "windows-x64",
      file: `cpython-${PYTHON_VERSION}+${PBS_TAG}-x86_64-pc-windows-msvc-shared-install_only.tar.gz`,
      pyBin: "python/python.exe",
    };
  }
  if (process.platform === "darwin") {
    const arch = process.arch === "arm64" ? "aarch64" : "x86_64";
    return {
      slug: `macos-${process.arch}`,
      file: `cpython-${PYTHON_VERSION}+${PBS_TAG}-${arch}-apple-darwin-install_only.tar.gz`,
      pyBin: "python/bin/python3",
    };
  }
  if (process.platform === "linux") {
    return {
      slug: "linux-x64",
      file: `cpython-${PYTHON_VERSION}+${PBS_TAG}-x86_64-unknown-linux-gnu-install_only.tar.gz`,
      pyBin: "python/bin/python3",
    };
  }
  throw new Error(`unsupported platform: ${process.platform}`);
}

const target = targetForHost();
const targetDir = join(pythonRoot, target.slug);
const pyBinPath = join(targetDir, target.pyBin);

console.log(`target: ${target.slug}`);
console.log(`tarball: ${target.file}`);

// ---------------------------------------------------------------------------
// Fetch + extract CPython
// ---------------------------------------------------------------------------
if (existsSync(pyBinPath)) {
  console.log(`CPython already extracted at ${targetDir} — skipping download`);
} else {
  if (existsSync(targetDir)) {
    // Stale partial extraction — nuke and re-fetch
    rmSync(targetDir, { recursive: true, force: true });
  }
  mkdirSync(targetDir, { recursive: true });

  const url = `${BASE_URL}/${target.file}`;
  const tmpTar = join(targetDir, "cpython.tar.gz");
  console.log(`downloading ${url}`);
  const res = await fetch(url);
  if (!res.ok || !res.body) {
    throw new Error(`fetch ${url} failed: ${res.status} ${res.statusText}`);
  }
  await pipeline(res.body, createWriteStream(tmpTar));
  const tarSize = statSync(tmpTar).size;
  console.log(`downloaded ${(tarSize / 1024 / 1024).toFixed(1)} MB`);

  console.log("extracting…");
  // System tar is available on Windows 10 1803+, macOS, and Linux.
  // Avoids an npm dependency that didn't import cleanly.
  const tarResult = spawnSync(
    "tar",
    ["-xzf", tmpTar, "-C", targetDir],
    { stdio: "inherit", shell: false },
  );
  if (tarResult.status !== 0) {
    throw new Error(`tar extraction failed: exit ${tarResult.status}`);
  }
  rmSync(tmpTar);
  console.log(`extracted to ${targetDir}`);
}

if (!existsSync(pyBinPath)) {
  throw new Error(`python binary not at expected path after extract: ${pyBinPath}`);
}

// ---------------------------------------------------------------------------
// Pre-install wheels into bundled Python's site-packages so users don't
// pay first-import network cost. Matches the Pyodide wheel manifest from
// v0.16.5 (pandas, openpyxl, reportlab, etc.) plus a few more that
// Pyodide blocked due to C extensions.
// ---------------------------------------------------------------------------
const WHEELS = [
  // Spreadsheet / data
  "pandas",
  "openpyxl",
  "xlsxwriter",
  // PDF
  "reportlab",
  "pypdf",
  "fpdf2",
  "pdfplumber",      // C-ext: was unavailable in Pyodide; works native
  // v0.20.16 — HTML/CSS -> PDF. The LLM is far better trained at
  // HTML+CSS than reportlab; render_html_to_pdf gives 90-95% layout
  // fidelity on first try where reportlab gets 60-70%. Pure-Python,
  // no headless browser dep, ~15MB.
  "weasyprint",
  // Word
  "python-docx",
  // Imaging
  "pillow",
  // Numerics
  "numpy",
  // HTML/XML
  "lxml",            // native C-ext; faster than pure-Python
  "beautifulsoup4",
  // Dates/locales/text
  "python-dateutil",
  "pytz",
  "num2words",
  "markdown",
  "jinja2",
  "pyyaml",
  // QR / barcode
  "qrcode[pil]",
  "python-barcode",
  // HTTP — native socket lib, was unusable in Pyodide
  "requests",
  // Embedding / search (used by document processing)
  // "sentence-transformers" later if we move embedding to native too
];

console.log(`installing ${WHEELS.length} wheels into ${target.slug}…`);
const pipResult = spawnSync(
  pyBinPath,
  ["-m", "pip", "install", "--no-warn-script-location", "--upgrade", ...WHEELS],
  { stdio: "inherit" },
);
if (pipResult.status !== 0) {
  throw new Error(`pip install failed with exit code ${pipResult.status}`);
}

// Sanity check: import each top-level package to confirm install.
const importChecks = ["pandas", "openpyxl", "reportlab", "pypdf", "PIL", "numpy", "lxml", "bs4", "docx", "num2words", "qrcode"];
const check = spawnSync(
  pyBinPath,
  ["-c", `import ${importChecks.join(", ")}; print("ok")`],
  { stdio: "inherit" },
);
if (check.status !== 0) {
  throw new Error(`post-install import check failed`);
}

console.log(`\nBundled Python ready: ${pyBinPath}`);
console.log(`Bundle size:`);
const sizeMb = (path) => {
  const r = spawnSync(
    process.platform === "win32" ? "powershell" : "du",
    process.platform === "win32"
      ? ["-Command", `'{0:N1} MB' -f ((Get-ChildItem -Recurse '${path}' | Measure-Object Length -Sum).Sum / 1MB)`]
      : ["-sh", path],
    { encoding: "utf8" },
  );
  return r.stdout?.trim() ?? "?";
};
console.log(`  ${sizeMb(targetDir)}`);
