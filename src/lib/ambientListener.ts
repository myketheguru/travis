/**
 * Ambient wake-word listener (v0.22.14).
 *
 * Continuous 16 kHz mono audio capture in the frontend + rolling
 * whisper.cpp transcription every ~2s over a ~3s window. When the
 * transcript contains the user's chosen wake phrase (default "hey
 * travis"), fires the wake handler. The handler is expected to
 * capture the next ~5s as the actual command and hand it off.
 *
 * Not the most efficient wake-word detector — a proper engine like
 * openWakeWord or Porcupine would use ~1% CPU vs our ~10-15%. That's
 * intentionally the trade-off for MVP: reuses infra we already ship,
 * supports arbitrary custom names for free (any phrase the user picks
 * just gets regex-matched against the transcript), and no new
 * platform-specific binary bundling. Users only pay the CPU cost
 * when ambient mode is on.
 *
 * A follow-up will layer a real wake-word engine (openWakeWord ONNX
 * inference via the ort crate) for users who want ambient always-on
 * without the CPU cost.
 */
import { speechTranscribe } from "./speechRuntime";

const SAMPLE_RATE = 16000;
const WINDOW_SECS = 3;
const TICK_SECS = 2;

const STORAGE_ENABLED = "travis_ambient";
const STORAGE_NAME = "travis_ambient_name";

export function readAmbientEnabled(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(STORAGE_ENABLED) === "on";
}

export function writeAmbientEnabled(v: boolean): void {
  if (typeof localStorage === "undefined") return;
  try {
    if (v) localStorage.setItem(STORAGE_ENABLED, "on");
    else localStorage.removeItem(STORAGE_ENABLED);
  } catch { /* ignore */ }
}

/** Custom name for the wake word. Defaults to "travis". Users can pick
 *  anything; we normalize to lowercase + strip non-letters at match time. */
export function readAmbientName(): string {
  if (typeof localStorage === "undefined") return "travis";
  return localStorage.getItem(STORAGE_NAME) || "travis";
}

export function writeAmbientName(name: string): void {
  if (typeof localStorage === "undefined") return;
  try {
    const norm = name.trim();
    if (norm) localStorage.setItem(STORAGE_NAME, norm);
    else localStorage.removeItem(STORAGE_NAME);
  } catch { /* ignore */ }
}

function normalize(s: string): string {
  return s.toLowerCase().replace(/[^a-z ]+/g, "").replace(/\s+/g, " ").trim();
}

/** Match either "hey <name>" or bare "<name>" at any word boundary. */
function matchesWake(transcript: string, name: string): boolean {
  const t = normalize(transcript);
  const n = normalize(name);
  if (!n) return false;
  return t.includes("hey " + n) || t.includes(n);
}

export type WakeReason = "wake" | "command";

export interface AmbientHandlers {
  /** Fired the first time the wake phrase is detected in a window. */
  onWake(): void;
  /** Fired with the transcribed command after the post-wake capture window. */
  onCommand(text: string): void;
  /** Optional — for surface UI reactions. */
  onStateChange?(state: AmbientState): void;
  /** Optional — non-fatal errors. */
  onError?(msg: string): void;
}

export type AmbientState =
  | "idle"
  | "listening"        // rolling wake detection
  | "captured"         // wake fired, capturing command
  | "transcribing"    // command captured, running whisper
  | "error";

/**
 * Start the ambient listener. Returns a stop function.
 *
 * State machine:
 *   idle -> listening (mic granted)
 *   listening -> captured (wake matched; next ~5s is the command)
 *   captured -> transcribing (window elapsed, running whisper)
 *   transcribing -> listening (transcript delivered)
 */
export async function startAmbient(
  handlers: AmbientHandlers,
): Promise<() => void> {
  let state: AmbientState = "idle";
  const setState = (s: AmbientState) => {
    state = s;
    handlers.onStateChange?.(state);
  };

  let stopped = false;
  let audioContext: AudioContext | null = null;
  let mediaStream: MediaStream | null = null;
  let source: MediaStreamAudioSourceNode | null = null;
  let processor: ScriptProcessorNode | null = null;
  const rolling: Float32Array[] = [];
  const rollingMaxSamples = Math.floor(SAMPLE_RATE * WINDOW_SECS);
  let rollingTotal = 0;
  let tickTimer: number | null = null;
  let capturePromise: Promise<void> | null = null;

  const cleanup = () => {
    processor?.disconnect();
    source?.disconnect();
    mediaStream?.getTracks().forEach((t) => t.stop());
    audioContext?.close().catch(() => {});
    processor = null;
    source = null;
    mediaStream = null;
    audioContext = null;
    if (tickTimer !== null) {
      window.clearInterval(tickTimer);
      tickTimer = null;
    }
    rolling.length = 0;
    rollingTotal = 0;
  };

  try {
    mediaStream = await navigator.mediaDevices.getUserMedia({
      audio: {
        channelCount: 1,
        echoCancellation: true,
        noiseSuppression: true,
        autoGainControl: true,
      },
    });
    audioContext = new AudioContext({ sampleRate: SAMPLE_RATE });
    source = audioContext.createMediaStreamSource(mediaStream);
    processor = audioContext.createScriptProcessor(4096, 1, 1);
    processor.onaudioprocess = (e) => {
      const chunk = new Float32Array(e.inputBuffer.getChannelData(0));
      rolling.push(chunk);
      rollingTotal += chunk.length;
      while (rollingTotal > rollingMaxSamples && rolling.length > 1) {
        const dropped = rolling.shift()!;
        rollingTotal -= dropped.length;
      }
    };
    source.connect(processor);
    processor.connect(audioContext.destination);
    setState("listening");
  } catch (e) {
    handlers.onError?.(e instanceof Error ? e.message : String(e));
    setState("error");
    cleanup();
    return () => {};
  }

  function snapshotRolling(): Float32Array {
    const total = rollingTotal;
    const merged = new Float32Array(total);
    let offset = 0;
    for (const c of rolling) {
      merged.set(c, offset);
      offset += c.length;
    }
    return merged;
  }

  async function transcribeWindow(samples: Float32Array): Promise<string> {
    if (samples.length < SAMPLE_RATE / 2) return ""; // <0.5s = don't bother
    try {
      return await speechTranscribe(samples);
    } catch (e) {
      handlers.onError?.(e instanceof Error ? e.message : String(e));
      return "";
    }
  }

  async function captureCommand(): Promise<void> {
    // Reset rolling buffer to capture the fresh 5-second command window.
    rolling.length = 0;
    rollingTotal = 0;
    setState("captured");
    // Wait 5s while onaudioprocess fills the buffer with the command.
    await new Promise((r) => setTimeout(r, 5000));
    if (stopped) return;
    setState("transcribing");
    const commandSamples = snapshotRolling();
    const text = await transcribeWindow(commandSamples);
    const trimmed = text.trim();
    if (trimmed) handlers.onCommand(trimmed);
    if (!stopped) setState("listening");
  }

  tickTimer = window.setInterval(() => {
    if (state !== "listening") return;
    if (capturePromise) return; // already capturing
    const samples = snapshotRolling();
    void (async () => {
      const transcript = await transcribeWindow(samples);
      if (stopped) return;
      if (state !== "listening") return;
      const name = readAmbientName();
      if (matchesWake(transcript, name)) {
        handlers.onWake();
        capturePromise = captureCommand();
        try { await capturePromise; }
        finally { capturePromise = null; }
      }
    })();
  }, TICK_SECS * 1000);

  return () => {
    stopped = true;
    cleanup();
    setState("idle");
  };
}
