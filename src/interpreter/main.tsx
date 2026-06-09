/**
 * Hidden interpreter window entry point.
 *
 * This window hosts the Pyodide Python runtime (CPython compiled to
 * WASM) in a sandboxed environment isolated from the main app. The
 * main process sends `run-python-request` Tauri events here; we
 * execute the requested code with mounted input files and emit
 * `run-python-result` back.
 *
 * Pyodide is loaded once at window startup and stays warm for the
 * lifetime of the app. Per-conversation sessions are kept in a Map so
 * variables persist across `run_python` calls within the same
 * conversation (matching Claude.ai's stateful behavior).
 */

import { createRoot } from "react-dom/client";
import { useEffect, useRef, useState } from "react";
import { listen, emit } from "@tauri-apps/api/event";
// Pyodide's TS types
import type { PyodideInterface } from "pyodide";

// v0.16.2 — lazy loading. Bootstrap only loads Pyodide runtime +
// micropip (~3-5s cold start). Packages load on demand via
// `loadPackagesFromImports(code)` per run_python call, then stay
// cached in the Pyodide instance for the rest of the session.
//
// v0.16.5 — all wheels we use today plus likely-need extras for a
// versatile assistant are pre-bundled by scripts/fetch-pyodide-
// wheels.mjs into the local Pyodide indexURL. That means:
//   - In-lock packages (pandas, numpy, openpyxl, pillow, lxml, …)
//     load from disk, not from jsdelivr.
//   - Pure-Python extras NOT in Pyodide's lock (reportlab, pypdf,
//     python-docx, num2words, fpdf2, xlsxwriter, qrcode, markdown)
//     are resolved via /pyodide-bundle/.../pyodide-extras.json and
//     installed with micropip using a local URL.
//
// Net effect: first `import pandas` drops from 8-12s (CDN) to ~1-2s
// (disk + decompress). Offline-by-default for everything we ship.
//
// Pure-Python wheels NOT in Pyodide's lock — we lazily call
// `micropip.install(localUrl)` when user code imports them. The
// EXTRAS_MANIFEST is fetched at startup from the bundled JSON file;
// the in-code fallback list below mirrors what fetch-pyodide-
// wheels.mjs vendors so the heuristic still works if the manifest
// somehow fails to load.
const EXTRAS_FALLBACK_NAMES = [
  "reportlab",
  "pypdf",
  "python-docx",
  "docx",        // python-docx's import name
  "num2words",
  "fpdf2",
  "fpdf",        // fpdf2's import name
  "xlsxwriter",
  "qrcode",
  "markdown",
];

// Packages we proactively warm in the background after ready. Two
// pools — in-lock ones go through loadPackage, extras go through
// micropip from the local URL in the manifest. By the time the first
// run_python call lands these are all cached, so a typical invoice
// flow (pandas → openpyxl → reportlab → num2words) pays zero cold
// cost.
const BACKGROUND_PRELOAD_LOCK = ["pandas", "Pillow", "jinja2"];
const BACKGROUND_PRELOAD_EXTRAS = [
  "openpyxl",
  "reportlab",
  "pypdf",
  "num2words",
];

interface ExtraManifestEntry {
  version: string;
  file: string;
  url: string;
}

// Loaded from /pyodide-bundle/node_modules/pyodide/pyodide-extras.json
// at bootstrap. Keys are micropip / import names; values point at a
// local wheel URL.
let EXTRAS_MANIFEST: Record<string, ExtraManifestEntry> = {};

interface RunPythonRequest {
  requestId: string;
  conversationId: number;
  code: string;
  purpose: string;
  // Base64-encoded file bytes keyed by filename
  inputFiles: Record<string, string>;
  // Extra micropip packages to install before running this code
  extraLibraries: string[];
  timeoutSecs: number;
}

interface RunPythonResult {
  requestId: string;
  stdout: string;
  stderr: string;
  // Base64-encoded output file bytes keyed by filename
  generatedFiles: Record<string, string>;
  executionMs: number;
  error: string | null;
}

function decodeBase64ToBytes(b64: string): Uint8Array {
  const binary = atob(b64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i++) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

function InterpreterRoot() {
  const [status, setStatus] = useState<string>("loading Pyodide…");
  const [ready, setReady] = useState(false);
  const pyodideRef = useRef<PyodideInterface | null>(null);
  const installedExtras = useRef<Set<string>>(new Set());

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        setStatus("loading Pyodide runtime…");
        // Dynamic import so vite doesn't try to bundle the wasm
        const pyodideModule = await import("pyodide");
        const pyodide = await pyodideModule.loadPyodide({
          // Local bundle — vite-plugin-static-copy puts the wasm,
          // stdlib zip, and wheel files at /pyodide-bundle/ at build time.
          // Works fully offline; no network dependency on first launch.
          indexURL: "/pyodide-bundle/node_modules/pyodide/",
          stdout: () => {
            /* captured per-execution via redirection */
          },
          stderr: () => {
            /* captured per-execution via redirection */
          },
        });
        if (cancelled) return;
        pyodideRef.current = pyodide;

        // v0.16.2 — bootstrap only loads micropip itself (1-2s);
        // everything else is lazy-loaded per run_python call.
        setStatus("loading micropip…");
        await pyodide.loadPackage(["micropip"]);
        if (cancelled) return;

        // v0.16.5 — load the extras manifest produced by
        // scripts/fetch-pyodide-wheels.mjs. Tells us where the
        // bundled PyPI wheels live so we can micropip.install them
        // by local URL instead of round-tripping to pypi.org.
        try {
          const r = await fetch("/pyodide-bundle/node_modules/pyodide/pyodide-extras.json");
          if (r.ok) {
            EXTRAS_MANIFEST = await r.json();
            console.info(
              "loaded extras manifest:",
              Object.keys(EXTRAS_MANIFEST).join(", "),
            );
          } else {
            console.warn("extras manifest not found; PyPI fallback active");
          }
        } catch (e) {
          console.warn("extras manifest fetch failed; PyPI fallback active:", e);
        }

        // Set up shared helpers in Python globals
        await pyodide.runPythonAsync(`
import sys
import io
import os
import json
import base64

# Ensure /inputs and /outputs always exist
os.makedirs('/inputs', exist_ok=True)
os.makedirs('/outputs', exist_ok=True)

def _travis_collect_outputs():
    """Walk /outputs and return {filename: base64_bytes}."""
    out = {}
    if os.path.isdir('/outputs'):
        for root, _, files in os.walk('/outputs'):
            for fn in files:
                full = os.path.join(root, fn)
                rel = os.path.relpath(full, '/outputs')
                with open(full, 'rb') as f:
                    out[rel] = base64.b64encode(f.read()).decode('ascii')
    return out

def _travis_clear_outputs():
    """Remove everything in /outputs before each run."""
    if os.path.isdir('/outputs'):
        for root, _, files in os.walk('/outputs'):
            for fn in files:
                try:
                    os.remove(os.path.join(root, fn))
                except Exception:
                    pass
`);

        setStatus("ready");
        setReady(true);
        // Tell the main process Pyodide is alive. v0.17.1 — re-emit
        // periodically for the first 30s as a backstop against the
        // ready-race regression introduced by v0.16.5's faster
        // bundled-wheel bootstrap: the interpreter window can now be
        // ready in ~3s, BEFORE Rust's setup() finishes attaching its
        // listener (~5s for DB migrations + keychain). The first
        // emit can land in the void; subsequent re-emits hit the
        // listener once it's wired. set_ready() in Rust is
        // idempotent, so repeat firings are harmless.
        await emit("interpreter-ready", { version: pyodideModule.version });
        let reEmitCount = 0;
        const reEmitInterval = setInterval(() => {
          emit("interpreter-ready", { version: pyodideModule.version });
          reEmitCount++;
          if (reEmitCount >= 10) clearInterval(reEmitInterval);
        }, 3000);

        // v0.16.2 — background preload of common heavy packages.
        // The interpreter reports ready BEFORE this runs, so the
        // first run_python call doesn't have to wait. If the agent
        // beats this race, loadPackagesFromImports inside runOne
        // handles it; if this finishes first, the packages are
        // already cached and runOne is instant.
        const idleKick = (work: () => void) => {
          if (
            typeof (window as unknown as { requestIdleCallback?: unknown })
              .requestIdleCallback === "function"
          ) {
            (
              window as unknown as { requestIdleCallback: (cb: () => void) => void }
            ).requestIdleCallback(work);
          } else {
            setTimeout(work, 1000);
          }
        };
        idleKick(() => {
          (async () => {
            try {
              await pyodide.loadPackage(BACKGROUND_PRELOAD_LOCK);
              console.info("background-preloaded lock:", BACKGROUND_PRELOAD_LOCK.join(", "));
            } catch (e) {
              console.warn("background preload (lock) failed:", e);
            }
            try {
              const micropip = pyodide.pyimport("micropip");
              for (const name of BACKGROUND_PRELOAD_EXTRAS) {
                const entry = EXTRAS_MANIFEST[name];
                if (!entry) continue;
                if (installedExtras.current.has(name)) continue;
                await micropip.install(entry.url);
                installedExtras.current.add(name);
              }
              console.info("background-preloaded extras:", BACKGROUND_PRELOAD_EXTRAS.join(", "));
            } catch (e) {
              console.warn("background preload (extras) failed:", e);
            }
          })();
        });
      } catch (err) {
        console.error("Pyodide bootstrap failed", err);
        setStatus(`failed: ${(err as Error).message ?? String(err)}`);
        await emit("interpreter-error", {
          message: (err as Error).message ?? String(err),
        });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!ready) return;
    let unlisten: (() => void) | null = null;
    listen<RunPythonRequest>("run-python-request", async (event) => {
      const req = event.payload;
      const result = await runOne(req);
      await emit("run-python-result", result);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      try {
        unlisten?.();
      } catch {
        /* ignore */
      }
    };
  }, [ready]);

  async function runOne(req: RunPythonRequest): Promise<RunPythonResult> {
    const startedAt = Date.now();
    const pyodide = pyodideRef.current;
    if (!pyodide) {
      return {
        requestId: req.requestId,
        stdout: "",
        stderr: "",
        generatedFiles: {},
        executionMs: 0,
        error: "interpreter not ready",
      };
    }

    // Install any extra libraries requested for this run
    if (req.extraLibraries.length > 0) {
      const micropip = pyodide.pyimport("micropip");
      for (const pkg of req.extraLibraries) {
        if (installedExtras.current.has(pkg)) continue;
        try {
          await micropip.install(pkg);
          installedExtras.current.add(pkg);
        } catch (e) {
          return {
            requestId: req.requestId,
            stdout: "",
            stderr: "",
            generatedFiles: {},
            executionMs: Date.now() - startedAt,
            error: `failed to install ${pkg}: ${(e as Error).message ?? e}`,
          };
        }
      }
    }

    // v0.16.2 — lazy package loading. Two passes:
    //
    // 1. `loadPackagesFromImports(code)` — Pyodide parses the script,
    //    finds `import` statements, and loads any Pyodide-bundled
    //    packages that match (pandas, openpyxl, Pillow, numpy, etc.).
    //    No-op if already loaded; cached for the session.
    //
    // 2. Scan the code for known-PyPI imports (reportlab, pypdf,
    //    python-docx) and `micropip.install` them. Pyodide's auto-
    //    detection can't fetch from PyPI; we have to handle it.
    try {
      await pyodide.loadPackagesFromImports(req.code);
    } catch (e) {
      console.warn("loadPackagesFromImports failed (continuing):", e);
    }
    const codeLower = req.code.toLowerCase();
    const importPattern = /(?:^|\n)\s*(?:from|import)\s+([a-zA-Z0-9_]+)/g;
    const seenImports = new Set<string>();
    let m: RegExpExecArray | null;
    while ((m = importPattern.exec(codeLower)) !== null) {
      seenImports.add(m[1]);
    }
    // v0.16.5 — prefer the bundled extras manifest. If the import name
    // (or its dist-name alias like python-docx for docx) is known, install
    // from the local wheel URL — no PyPI round trip. Falls back to PyPI by
    // bare name if the manifest is missing AND the import is in our
    // legacy known-extras list.
    const aliasToDist: Record<string, string> = {
      docx: "python-docx",
      fpdf: "fpdf2",
    };
    for (const imp of seenImports) {
      const distName = aliasToDist[imp] ?? imp;
      const entry = EXTRAS_MANIFEST[imp] ?? EXTRAS_MANIFEST[distName];
      const isKnownFallback = EXTRAS_FALLBACK_NAMES.includes(imp);
      if (!entry && !isKnownFallback) continue;
      if (installedExtras.current.has(distName)) continue;
      try {
        const micropip = pyodide.pyimport("micropip");
        if (entry) {
          await micropip.install(entry.url);
          console.info(
            `lazy-installed ${distName} from bundled wheel (${entry.file})`,
          );
        } else {
          await micropip.install(distName);
          console.info(`lazy-installed ${distName} from PyPI (manifest miss)`);
        }
        installedExtras.current.add(distName);
      } catch (e) {
        console.warn(`lazy install ${distName} failed:`, e);
        // Don't abort the run — let Python's import raise the real
        // ImportError so the LLM can see it.
      }
    }

    // Mount input files into /inputs/
    const FS = (pyodide as unknown as { FS: { writeFile: (path: string, data: Uint8Array) => void; mkdir: (path: string) => void } }).FS;
    try {
      FS.mkdir("/inputs");
    } catch {
      /* already exists */
    }
    for (const [name, b64] of Object.entries(req.inputFiles)) {
      const bytes = decodeBase64ToBytes(b64);
      const safeName = name.replace(/[^A-Za-z0-9._-]/g, "_");
      try {
        FS.writeFile(`/inputs/${safeName}`, bytes);
      } catch (e) {
        console.warn(`could not mount input ${name}:`, e);
      }
    }

    // Clear any prior outputs
    try {
      await pyodide.runPythonAsync(`_travis_clear_outputs()`);
    } catch {
      /* ignore */
    }

    // Capture stdout/stderr
    let stdout = "";
    let stderr = "";
    pyodide.setStdout({
      batched: (chunk: string) => {
        stdout += chunk;
      },
    });
    pyodide.setStderr({
      batched: (chunk: string) => {
        stderr += chunk;
      },
    });

    let error: string | null = null;
    let timedOut = false;
    const timeoutHandle = setTimeout(() => {
      timedOut = true;
    }, req.timeoutSecs * 1000);

    try {
      await pyodide.runPythonAsync(req.code);
    } catch (e) {
      error = (e as Error).message ?? String(e);
    } finally {
      clearTimeout(timeoutHandle);
    }
    if (timedOut && !error) {
      error = `execution exceeded ${req.timeoutSecs}s timeout`;
    }

    // Collect outputs
    let generatedFiles: Record<string, string> = {};
    try {
      const outputsProxy = await pyodide.runPythonAsync(
        `_travis_collect_outputs()`,
      );
      // Pyodide returns a PyProxy for dicts; convert to JS
      generatedFiles = outputsProxy.toJs({ dict_converter: Object.fromEntries });
      outputsProxy.destroy();
    } catch (e) {
      console.warn("could not collect outputs:", e);
    }

    return {
      requestId: req.requestId,
      stdout,
      stderr,
      generatedFiles,
      executionMs: Date.now() - startedAt,
      error,
    };
  }

  return (
    <div>
      <div>Travis Interpreter</div>
      <div style={{ opacity: 0.6 }}>{status}</div>
    </div>
  );
}

const rootEl = document.getElementById("interp-root");
if (rootEl) {
  createRoot(rootEl).render(<InterpreterRoot />);
}
