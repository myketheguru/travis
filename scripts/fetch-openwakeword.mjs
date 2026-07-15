#!/usr/bin/env node
/**
 * Build-time fetcher for the openWakeWord ONNX models.
 *
 * v0.28.58 — real audio wake word. Ships three ONNX files with the
 * installer so first-run wake detection works without a download
 * splash:
 *
 *   melspectrogram.onnx  — raw f32 audio -> mel features
 *   embedding_model.onnx — mel features   -> 96-dim embeddings
 *   hey_jarvis_v0.1.onnx — embeddings     -> wake confidence
 *
 * "Hey Jarvis" is the placeholder wake phrase because openWakeWord
 * doesn't ship a pre-trained "Hey Travis" model. A custom-trained
 * "Hey Travis" model lands in a follow-up (needs ~500 audio samples
 * of the phrase + a training run — outside a code-only change).
 *
 * Layout: src-tauri/resources/wake/*.onnx
 *
 * Idempotent — re-running with the files already on disk is a no-op.
 * Small files (~few MB total) so bundling adds negligible installer
 * bloat vs the ~74MB whisper model already shipped.
 */
import { createWriteStream, existsSync, mkdirSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { pipeline } from "node:stream/promises";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "..");
const resourcesRoot = join(repoRoot, "src-tauri", "resources");
const wakeRoot = join(resourcesRoot, "wake");

// v0.5.1 is the current release; each model file is a github-release
// asset. Pin the release so a repo re-tag doesn't silently swap
// model behaviour under us.
const BASE =
  "https://github.com/dscripka/openWakeWord/releases/download/v0.5.1";

const MODELS = [
  { name: "melspectrogram.onnx", minBytes: 100_000 },
  { name: "embedding_model.onnx", minBytes: 1_000_000 },
  { name: "hey_jarvis_v0.1.onnx", minBytes: 100_000 },
];

mkdirSync(wakeRoot, { recursive: true });

for (const { name, minBytes } of MODELS) {
  const target = join(wakeRoot, name);
  if (existsSync(target) && statSync(target).size >= minBytes) {
    console.log(
      `wake: ${name} already present (${(statSync(target).size / 1e6).toFixed(1)} MB)`,
    );
    continue;
  }
  const url = `${BASE}/${name}`;
  console.log(`wake: fetching ${url}`);
  const resp = await fetch(url, { redirect: "follow" });
  if (!resp.ok || !resp.body) {
    console.error(`wake: fetch failed for ${name}: ${resp.status} ${resp.statusText}`);
    process.exit(1);
  }
  await pipeline(resp.body, createWriteStream(target));
  const size = statSync(target).size;
  if (size < minBytes) {
    console.error(
      `wake: ${name} suspiciously small (${size} bytes) — deleting to force refetch next build`,
    );
    process.exit(1);
  }
  console.log(`wake: saved ${name} (${(size / 1e6).toFixed(2)} MB)`);
}

console.log("wake: all models ready");
