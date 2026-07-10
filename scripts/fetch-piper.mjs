#!/usr/bin/env node
/**
 * Build-time fetcher for Piper TTS.
 *
 * Downloads:
 *   1. The piper binary for the current build platform
 *      (linux-x86_64 / macos-aarch64 / windows-amd64)
 *   2. The Amy voice model (en_US-amy-medium ~30 MB) and its config
 *
 * Both land in src-tauri/resources/piper/ so the tauri.conf.json
 * `bundle.resources` glob picks them up.
 *
 * Layout:
 *   src-tauri/resources/piper/
 *     piper                                (or piper.exe on windows)
 *     en_US-amy-medium.onnx
 *     en_US-amy-medium.onnx.json
 *
 * The Rust `voice::piper` module spawns the binary as a subprocess,
 * writes text to stdin, reads WAV bytes from stdout. If the binary or
 * voice model is missing at runtime, the frontend falls back to the
 * browser's speechSynthesis so builds without the resource (e.g. dev
 * runs that skip the prebuild) keep working.
 *
 * Idempotent: re-running with files already present is a no-op.
 */
import {
  createWriteStream,
  existsSync,
  mkdirSync,
  statSync,
  chmodSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { pipeline } from "node:stream/promises";
import { platform, arch } from "node:process";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "..");
const piperRoot = join(repoRoot, "src-tauri", "resources", "piper");

const PIPER_VERSION = "2023.11.14-2";
const PIPER_RELEASE_BASE = `https://github.com/rhasspy/piper/releases/download/${PIPER_VERSION}`;

// Map node's platform+arch to piper's release asset name. Fall back
// gracefully — an unsupported host just skips the binary and the
// frontend uses speechSynthesis at runtime.
function piperAssetForHost() {
  if (platform === "linux") {
    if (arch === "x64") return { url: `${PIPER_RELEASE_BASE}/piper_linux_x86_64.tar.gz`, tar: true, binName: "piper" };
    if (arch === "arm64") return { url: `${PIPER_RELEASE_BASE}/piper_linux_aarch64.tar.gz`, tar: true, binName: "piper" };
  }
  if (platform === "darwin") {
    if (arch === "x64") return { url: `${PIPER_RELEASE_BASE}/piper_macos_x64.tar.gz`, tar: true, binName: "piper" };
    if (arch === "arm64") return { url: `${PIPER_RELEASE_BASE}/piper_macos_aarch64.tar.gz`, tar: true, binName: "piper" };
  }
  if (platform === "win32") {
    if (arch === "x64") return { url: `${PIPER_RELEASE_BASE}/piper_windows_amd64.zip`, tar: false, binName: "piper.exe" };
  }
  return null;
}

// Amy voice model — a warm, natural en_US voice at medium quality.
// Roughly 30 MB. Hosted on HuggingFace, same CDN as whisper.
const VOICE_ONNX_URL =
  "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/amy/medium/en_US-amy-medium.onnx";
const VOICE_JSON_URL =
  "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/amy/medium/en_US-amy-medium.onnx.json";

mkdirSync(piperRoot, { recursive: true });

async function downloadTo(url, dest, label, minBytes = 0) {
  if (existsSync(dest)) {
    const size = statSync(dest).size;
    if (size >= minBytes) {
      console.log(`piper: ${label} already present at ${dest} (${(size / 1e6).toFixed(1)} MB)`);
      return;
    }
    console.log(`piper: ${label} exists but small (${size} B) — refetching`);
  }
  console.log(`piper: fetching ${label} from ${url}`);
  const resp = await fetch(url, { redirect: "follow" });
  if (!resp.ok || !resp.body) {
    throw new Error(`fetch ${label} failed: ${resp.status} ${resp.statusText}`);
  }
  const total = Number(resp.headers.get("content-length")) || 0;
  let received = 0;
  let lastLogAt = 0;
  const reader = resp.body.getReader();
  const stream = new ReadableStream({
    async start(controller) {
      while (true) {
        const { done, value } = await reader.read();
        if (done) break;
        received += value.byteLength;
        const now = Date.now();
        if (now - lastLogAt > 500) {
          const pct = total ? ((received / total) * 100).toFixed(1) : "?";
          process.stdout.write(`\rpiper: ${label} ${(received / 1e6).toFixed(1)} MB (${pct}%)   `);
          lastLogAt = now;
        }
        controller.enqueue(value);
      }
      controller.close();
    },
  });
  await pipeline(stream, createWriteStream(dest));
  process.stdout.write("\n");
  console.log(`piper: wrote ${dest} (${(statSync(dest).size / 1e6).toFixed(1)} MB)`);
}

async function fetchArchiveAndExtract(url, extractDir, binName) {
  const isZip = url.endsWith(".zip");
  const archivePath = join(piperRoot, isZip ? "piper.zip" : "piper.tar.gz");
  const binTarget = join(extractDir, binName);
  if (existsSync(binTarget) && statSync(binTarget).size > 100_000) {
    console.log(`piper: binary already present at ${binTarget}`);
    return binTarget;
  }
  await downloadTo(url, archivePath, "binary", 1_000_000);
  console.log(`piper: extracting ${archivePath}`);
  const { spawnSync } = await import("node:child_process");
  const result = isZip
    ? spawnSync("tar", ["-xf", archivePath, "-C", extractDir], { stdio: "inherit" })
    : spawnSync("tar", ["-xzf", archivePath, "-C", extractDir, "--strip-components=1"], { stdio: "inherit" });
  if (result.status !== 0) {
    throw new Error(`extract failed with status ${result.status}`);
  }
  if (!existsSync(binTarget) && platform !== "win32") {
    // The archive nests some files under a folder; try to locate the
    // binary if the strip didn't put it exactly where expected.
    console.warn(`piper: expected ${binTarget} not found after extract; leaving archive in place for inspection`);
  } else {
    try {
      chmodSync(binTarget, 0o755);
    } catch {
      /* ignore chmod errors on Windows */
    }
  }
  return binTarget;
}

const asset = piperAssetForHost();
if (!asset) {
  console.log(`piper: no prebuilt binary for ${platform}/${arch} — skipping binary fetch (runtime will fall back to speechSynthesis)`);
} else {
  try {
    await fetchArchiveAndExtract(asset.url, piperRoot, asset.binName);
  } catch (e) {
    console.warn(`piper: binary fetch failed — ${e.message}. Runtime will fall back to speechSynthesis.`);
  }
}

// Voice model — always fetch since the frontend can fall back on TTS
// but not on the model, and it's small enough to always ship.
try {
  await downloadTo(
    VOICE_ONNX_URL,
    join(piperRoot, "en_US-amy-medium.onnx"),
    "voice model",
    20_000_000,
  );
  await downloadTo(
    VOICE_JSON_URL,
    join(piperRoot, "en_US-amy-medium.onnx.json"),
    "voice config",
    100,
  );
} catch (e) {
  console.warn(`piper: voice model fetch failed — ${e.message}. Runtime will fall back to speechSynthesis.`);
}

console.log("piper: fetch complete");
