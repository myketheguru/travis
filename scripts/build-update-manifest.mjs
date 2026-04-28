#!/usr/bin/env node
//
// Build the Tauri updater's `update.json` manifest from the artifacts
// produced by `npm run tauri build`.
//
// ─── ONE-TIME SETUP ────────────────────────────────────────────────────────
//
// 1. Generate a signing keypair (do this ONCE; back up the private key —
//    losing it means your existing installs can never auto-update again):
//
//       npx tauri signer generate -w "$HOME/.tauri/travis.key"
//
//    This prints a public key to stdout AND writes the private key (password-
//    protected) to ~/.tauri/travis.key. Save the password somewhere safe.
//
// 2. Paste the public key into src-tauri/tauri.conf.json under
//    plugins.updater.pubkey, replacing the placeholder.
//
// 3. Decide where update.json + the installer binaries will be hosted. The
//    current tauri.conf.json points at:
//
//       https://leadtoempower.github.io/travis/update.json
//
//    GitHub Pages on the leadtoempower/travis repo's gh-pages branch is the
//    simplest setup; GitHub Releases also works — just adjust the endpoint.
//
// ─── PER-RELEASE WORKFLOW ──────────────────────────────────────────────────
//
// 1. Bump version in src-tauri/tauri.conf.json AND src-tauri/Cargo.toml.
//
// 2. Build with the signing key in scope:
//
//       export TAURI_SIGNING_PRIVATE_KEY="$(cat $HOME/.tauri/travis.key)"
//       export TAURI_SIGNING_PRIVATE_KEY_PASSWORD="<your password>"
//       npm run tauri build
//
//    (PowerShell:
//       $env:TAURI_SIGNING_PRIVATE_KEY    = Get-Content $HOME\.tauri\travis.key -Raw
//       $env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = "<your password>"
//       npm run tauri build
//    )
//
// 3. Tauri now writes installer bundles AND `<bundle>.sig` files under
//    src-tauri/target/release/bundle/.
//
// 4. Run this script with:
//      - --version       semver string (must match tauri.conf.json)
//      - --release-url   the URL prefix where the binaries will be hosted
//      - --notes         release notes (markdown ok, single string)
//      - --out           where to write update.json (default: dist-update/)
//
//    Example:
//
//       node scripts/build-update-manifest.mjs \
//         --version 0.2.0 \
//         --release-url https://github.com/leadtoempower/travis/releases/download/v0.2.0 \
//         --notes "Adds Outlook OAuth, fixes overlay focus."
//
// 5. Upload BOTH the installer files (the ones referenced in update.json) AND
//    update.json itself to the release-url location. For GitHub Pages, push
//    update.json to the gh-pages branch; for GitHub Releases, attach
//    everything to the release.
//
// On first launch, every running installation hits the configured endpoint,
// notices a newer version + verifies the .sig signature, and (when the user
// clicks Install in Settings → Updates) downloads + installs.

import { existsSync, readFileSync, writeFileSync, mkdirSync, readdirSync } from "node:fs";
import { join, dirname, basename, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "..");

function arg(name, fallback) {
  const idx = process.argv.indexOf(`--${name}`);
  if (idx !== -1 && idx + 1 < process.argv.length) return process.argv[idx + 1];
  return fallback;
}

function die(msg) {
  console.error(`error: ${msg}`);
  process.exit(1);
}

const version = arg("version") ?? readVersion();
const releaseUrl = arg("release-url");
const notes = arg("notes") ?? "";
const outDir = arg("out") ?? join(repoRoot, "dist-update");
const bundleRoot = join(repoRoot, "src-tauri", "target", "release", "bundle");

if (!releaseUrl) die("--release-url is required (URL prefix where binaries will be hosted)");
if (!existsSync(bundleRoot)) {
  die(`bundle dir not found: ${bundleRoot}\nRun \`npm run tauri build\` first.`);
}

function readVersion() {
  try {
    const cfg = JSON.parse(
      readFileSync(join(repoRoot, "src-tauri", "tauri.conf.json"), "utf8"),
    );
    return cfg.version;
  } catch {
    die("could not read version from tauri.conf.json — pass --version explicitly");
  }
}

// Tauri organizes bundle output by platform. We look for the canonical
// installer + its .sig sidecar, by platform.
//
//   Windows: bundle/nsis/Travis_<v>_x64-setup.exe + .sig
//            bundle/msi/Travis_<v>_x64_en-US.msi + .sig
//   macOS:   bundle/macos/Travis.app.tar.gz + .sig (universal)
//            bundle/dmg/Travis_<v>_<arch>.dmg + .sig
//   Linux:   bundle/appimage/Travis_<v>_amd64.AppImage + .sig
//
// We pick one per platform (NSIS / app.tar.gz / AppImage are what the Tauri
// updater consumes; .msi and .dmg can't be auto-installed). For each found
// (binary, sig) pair we add a platforms entry.
const platformPick = [
  {
    key: "windows-x86_64",
    dir: "nsis",
    match: (f) => f.endsWith("-setup.exe"),
  },
  {
    key: "darwin-x86_64",
    dir: "macos",
    match: (f) => f.endsWith(".app.tar.gz"),
  },
  {
    key: "darwin-aarch64",
    dir: "macos",
    match: (f) => f.endsWith(".app.tar.gz"),
  },
  {
    key: "linux-x86_64",
    dir: "appimage",
    match: (f) => f.endsWith(".AppImage"),
  },
];

const platforms = {};
for (const pick of platformPick) {
  const dir = join(bundleRoot, pick.dir);
  if (!existsSync(dir)) continue;
  const files = readdirSync(dir);
  const bin = files.find(pick.match);
  if (!bin) continue;
  const sigName = `${bin}.sig`;
  const sigPath = join(dir, sigName);
  if (!existsSync(sigPath)) {
    console.warn(
      `warn: ${pick.key}: found ${bin} but no signature at ${sigPath} — skipping`,
    );
    continue;
  }
  const signature = readFileSync(sigPath, "utf8").trim();
  platforms[pick.key] = {
    signature,
    url: `${releaseUrl.replace(/\/$/, "")}/${bin}`,
  };
}

if (Object.keys(platforms).length === 0) {
  die(
    "no signed installer artifacts found. Make sure TAURI_SIGNING_PRIVATE_KEY was set when running `npm run tauri build`.",
  );
}

const manifest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms,
};

mkdirSync(outDir, { recursive: true });
const outPath = join(outDir, "update.json");
writeFileSync(outPath, JSON.stringify(manifest, null, 2));

console.log(`wrote ${outPath}`);
console.log(`platforms: ${Object.keys(platforms).join(", ")}`);
console.log(`\nNext: upload update.json AND the binaries below to ${releaseUrl}/`);
for (const [key, p] of Object.entries(platforms)) {
  console.log(`  ${key}: ${basename(p.url)}`);
}
