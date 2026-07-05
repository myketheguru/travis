#!/usr/bin/env node
/**
 * Build-time fetcher for the whisper.cpp base English model.
 *
 * Ships the ~74 MB ggml-base.en.bin with every installer so the first
 * mic press works instantly — no "Travis is getting additional
 * resources" download splash for something users expect to be there.
 *
 * Layout: src-tauri/resources/whisper/ggml-base.en.bin
 *
 * Tauri's resources bundling copies this into the installer.
 * `speech_runtime::bootstrap::ensure_ready` (Rust) checks the bundled
 * resource before falling back to the HuggingFace download URL — dev
 * builds without the resource keep working; installer builds ship
 * with it and skip the download entirely.
 *
 * Idempotent: re-running with the model already on disk is a no-op.
 * SHA-256 verification would be nice but the whisper.cpp project
 * doesn't publish hashes; we trust the HuggingFace CDN.
 */
import { createWriteStream, existsSync, mkdirSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { pipeline } from "node:stream/promises";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "..");
const resourcesRoot = join(repoRoot, "src-tauri", "resources");
const whisperRoot = join(resourcesRoot, "whisper");

// Match speech_runtime::bootstrap::DEFAULT_MODEL exactly.
const MODEL_NAME = "ggml-base.en.bin";
const MODEL_URL = `https://huggingface.co/ggerganov/whisper.cpp/resolve/main/${MODEL_NAME}`;

// Approx 74 MB. If the file on disk is smaller than this floor, treat
// it as a partial download and re-fetch. Guards against interrupted
// builds leaving a bad file on disk.
const MIN_SIZE_BYTES = 60 * 1024 * 1024;

const target = join(whisperRoot, MODEL_NAME);

if (existsSync(target)) {
  const size = statSync(target).size;
  if (size >= MIN_SIZE_BYTES) {
    console.log(`whisper: model already present at ${target} (${(size / 1e6).toFixed(1)} MB)`);
    process.exit(0);
  }
  console.log(`whisper: existing model is too small (${(size / 1e6).toFixed(1)} MB) — refetching`);
}

mkdirSync(whisperRoot, { recursive: true });

console.log(`whisper: fetching ${MODEL_URL}`);
const resp = await fetch(MODEL_URL, { redirect: "follow" });
if (!resp.ok || !resp.body) {
  console.error(`whisper: fetch failed: ${resp.status} ${resp.statusText}`);
  process.exit(1);
}

const total = Number(resp.headers.get("content-length")) || 0;
let received = 0;
let lastLogAt = 0;

const reader = resp.body.getReader();
const nodeStream = new ReadableStream({
  async start(controller) {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      received += value.byteLength;
      const now = Date.now();
      if (now - lastLogAt > 500) {
        const pct = total ? ((received / total) * 100).toFixed(1) : "?";
        process.stdout.write(`\rwhisper: ${(received / 1e6).toFixed(1)} MB (${pct}%)   `);
        lastLogAt = now;
      }
      controller.enqueue(value);
    }
    controller.close();
  },
});

await pipeline(nodeStream, createWriteStream(target));
process.stdout.write("\n");
console.log(`whisper: wrote ${target} (${(statSync(target).size / 1e6).toFixed(1)} MB)`);
