#!/usr/bin/env node
/**
 * Fetch Pyodide wheels at build time so Travis ships with them pre-
 * bundled. With this in place, `loadPackagesFromImports` and
 * `micropip.install` resolve to local URLs — no jsdelivr CDN round
 * trip on first `import pandas` (the dominant cold-start tax in
 * v0.16.2-v0.16.4).
 *
 * Two pools:
 *
 *  1. **In-lock wheels** — packages declared in Pyodide's
 *     `pyodide-lock.json`. We resolve transitive deps via the lock's
 *     `depends` field, then download each unique wheel from
 *     `https://cdn.jsdelivr.net/pyodide/v<ver>/full/<file_name>` into
 *     `node_modules/pyodide/` where vite's static-copy already mirrors
 *     them to `dist/pyodide-bundle/node_modules/pyodide/`. Pyodide's
 *     own loader picks them up automatically because `indexURL`
 *     points at that path.
 *
 *  2. **Pure-Python wheels NOT in lock** — packages we install via
 *     `micropip` (reportlab, pypdf, python-docx, num2words, fpdf2,
 *     xlsxwriter, qrcode, markdown). We query PyPI's JSON API for the
 *     newest `py3-none-any.whl` (or `py2.py3-none-any.whl`), download
 *     into `node_modules/pyodide-extra-wheels/`, and emit a
 *     `pyodide-extra-wheels.json` manifest the interpreter reads at
 *     runtime to call `micropip.install("/pyodide-bundle/extra-wheels/<file>")`.
 *
 * Idempotent: re-running is cheap because each wheel is skipped if
 * already present + size-matched.
 *
 * Run via `npm run fetch:wheels` or automatically before any build
 * via the `prebuild` script hook in package.json.
 */
import { createWriteStream, existsSync, mkdirSync, statSync, writeFileSync, readFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { pipeline } from "node:stream/promises";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "..");
const pyodideDir = join(repoRoot, "node_modules", "pyodide");
// All extras live next to Pyodide's own wheels so a single vite
// static-copy glob captures them. The manifest is read by the
// interpreter at runtime to know which extras exist locally.
const extraManifestPath = join(pyodideDir, "pyodide-extras.json");

// ---------------------------------------------------------------------------
// Manifest — what we want bundled.
//
// Tier 1 (used today): pandas/openpyxl for spreadsheets, reportlab/pypdf
// for invoices, pillow for image embedding, num2words for amount-in-words.
//
// Tier 2 (likely-need for a versatile ops assistant): python-docx for Word,
// jinja2 for templating, pyyaml for config, lxml/bs4 for HTML/XML, fpdf2/
// xlsxwriter as alternates, qrcode for invoice QRs, markdown for note
// generation, pyodide-http so `requests`-style code works.
//
// Tier 3 (skipped — too heavy or niche): scipy (~30MB), matplotlib (~10MB
// + fonts), babel, phonenumbers, sympy. Re-evaluate if a recurring user
// flow demands them.
// ---------------------------------------------------------------------------
const IN_LOCK_PACKAGES = [
  "pandas",
  "pillow",
  "lxml",
  "beautifulsoup4",
  "pyyaml",
  "jinja2",
  "python-dateutil",
  "pytz",
  "pyodide-http",
  "micropip",
];

// openpyxl + et_xmlfile aren't in Pyodide's lock (pure-Python; lives
// on PyPI only). They sit alongside the other extras so micropip can
// install both from local URLs.
const PYPI_EXTRAS = [
  "openpyxl",
  "et_xmlfile",     // openpyxl dep
  "reportlab",
  "pypdf",
  "python-docx",
  "num2words",
  "fpdf2",
  "xlsxwriter",
  "qrcode",
  "markdown",
];

const pyodideVersion = JSON.parse(
  readFileSync(join(pyodideDir, "package.json"), "utf8"),
).version;

console.log(`pyodide version: ${pyodideVersion}`);

const lock = JSON.parse(
  readFileSync(join(pyodideDir, "pyodide-lock.json"), "utf8"),
);

// ---------------------------------------------------------------------------
// Resolve transitive dependencies for in-lock packages.
// ---------------------------------------------------------------------------
function resolveDeps(seeds) {
  const visited = new Set();
  const queue = [...seeds];
  while (queue.length) {
    const name = queue.shift();
    if (visited.has(name)) continue;
    const pkg = lock.packages[name];
    if (!pkg) {
      console.warn(`  ! ${name} not in pyodide-lock; skipping`);
      continue;
    }
    visited.add(name);
    for (const dep of pkg.depends || []) {
      if (!visited.has(dep)) queue.push(dep);
    }
  }
  return visited;
}

const inLockResolved = resolveDeps(IN_LOCK_PACKAGES);
console.log(
  `resolved ${inLockResolved.size} in-lock packages (seeds + transitive)`,
);

// ---------------------------------------------------------------------------
// Download helper. Streams to disk, skips if size already matches.
// ---------------------------------------------------------------------------
async function downloadTo(url, dest, expectedSize) {
  if (existsSync(dest)) {
    const sz = statSync(dest).size;
    if (!expectedSize || sz === expectedSize) {
      return { skipped: true, size: sz };
    }
    console.warn(`  size mismatch on ${dest} (${sz} vs ${expectedSize}); refetching`);
  }
  // Three tries with 500ms / 2s backoff — covers transient DNS and CDN
  // hiccups without making CI hang on a real outage.
  let lastErr = null;
  for (let attempt = 1; attempt <= 3; attempt++) {
    try {
      const res = await fetch(url);
      if (!res.ok || !res.body) {
        throw new Error(`HTTP ${res.status} ${res.statusText}`);
      }
      mkdirSync(dirname(dest), { recursive: true });
      await pipeline(res.body, createWriteStream(dest));
      return { skipped: false, size: statSync(dest).size };
    } catch (err) {
      lastErr = err;
      if (attempt < 3) {
        const delay = attempt * 500 + 500;
        console.warn(`  attempt ${attempt} for ${url} failed (${err.message}); retrying in ${delay}ms`);
        await new Promise((r) => setTimeout(r, delay));
      }
    }
  }
  throw new Error(`fetch ${url} failed after 3 attempts: ${lastErr.message}`);
}

// ---------------------------------------------------------------------------
// Pull each in-lock wheel from the Pyodide CDN.
// ---------------------------------------------------------------------------
const CDN_BASE = `https://cdn.jsdelivr.net/pyodide/v${pyodideVersion}/full`;
let inLockBytes = 0;
let inLockFetched = 0;
for (const name of inLockResolved) {
  const pkg = lock.packages[name];
  const url = `${CDN_BASE}/${pkg.file_name}`;
  const dest = join(pyodideDir, pkg.file_name);
  try {
    const { skipped, size } = await downloadTo(url, dest, pkg.size_in_bytes);
    inLockBytes += size;
    if (!skipped) inLockFetched++;
    if (!skipped) console.log(`  ✓ ${pkg.file_name} (${(size / 1024).toFixed(0)} KB)`);
  } catch (err) {
    console.error(`  ✗ ${pkg.file_name}: ${err.message}`);
    throw err;
  }
}
console.log(
  `in-lock wheels: ${inLockFetched} fetched, ${inLockResolved.size - inLockFetched} cached, ${(inLockBytes / 1024 / 1024).toFixed(1)} MB total`,
);

// ---------------------------------------------------------------------------
// Pull each PyPI extra. Query PyPI's JSON API for the newest release,
// pick a pure-Python wheel.
// ---------------------------------------------------------------------------

function pickWheel(release) {
  // Prefer the most universal pure-Python wheel: py3-none-any > py2.py3-none-any.
  const preferences = [
    (f) => /-py3-none-any\.whl$/i.test(f.filename),
    (f) => /-py2\.py3-none-any\.whl$/i.test(f.filename),
    (f) => /-none-any\.whl$/i.test(f.filename),
  ];
  for (const pref of preferences) {
    const match = release.find(pref);
    if (match) return match;
  }
  return null;
}

const extraManifest = {};
let extraBytes = 0;
let extraFetched = 0;
for (const name of PYPI_EXTRAS) {
  const meta = await fetch(`https://pypi.org/pypi/${name}/json`).then((r) =>
    r.ok ? r.json() : Promise.reject(new Error(`pypi 404 for ${name}`)),
  );
  const latest = meta.info.version;
  const release = meta.releases[latest] || [];
  const wheel = pickWheel(release);
  if (!wheel) {
    console.error(
      `  ✗ ${name} ${latest}: no pure-python wheel (only sdist or platform-specific). Skipping.`,
    );
    continue;
  }
  const dest = join(pyodideDir, wheel.filename);
  const { skipped, size } = await downloadTo(wheel.url, dest, wheel.size);
  extraBytes += size;
  if (!skipped) extraFetched++;
  if (!skipped) console.log(`  ✓ ${wheel.filename} (${(size / 1024).toFixed(0)} KB)`);
  // The manifest maps the *importable* / *micropip* name to the local URL
  // the interpreter will hand to micropip.install. python-docx is an alias
  // for the import-name "docx" — both forms point to the same wheel so the
  // interpreter's runtime-import-scan finds it either way.
  const localUrl = `/pyodide-bundle/node_modules/pyodide/${wheel.filename}`;
  extraManifest[name] = { version: latest, file: wheel.filename, url: localUrl };
  if (name === "python-docx") extraManifest["docx"] = extraManifest[name];
}
writeFileSync(extraManifestPath, JSON.stringify(extraManifest, null, 2));
console.log(
  `extra wheels: ${extraFetched} fetched, ${PYPI_EXTRAS.length - extraFetched} cached, ${(extraBytes / 1024 / 1024).toFixed(1)} MB total`,
);
console.log(`manifest written: ${extraManifestPath}`);
console.log(
  `BUNDLE GROWTH: +${((inLockBytes + extraBytes) / 1024 / 1024).toFixed(1)} MB to installer`,
);
