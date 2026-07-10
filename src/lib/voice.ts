/**
 * Voice output (v0.22.13).
 *
 * MVP uses the browser's SpeechSynthesis API — the OS's built-in
 * text-to-speech voice. Works on every platform Travis runs on, no
 * extra download, no bundled binary.
 *
 * A follow-up slice will layer Piper (open-source neural TTS with a
 * consistent voice regardless of OS) as an optional upgrade for users
 * who want Travis to sound the same on every machine. Piper adds
 * ~90 MB per user of lazy-downloaded assets; we're validating the
 * "does Travis speaking help?" UX question with the free path first
 * before asking users to pay that install cost.
 */

const STORAGE_KEY = "travis_voice_output";
const STORAGE_VOICE_KEY = "travis_voice_output_voice";

export interface VoiceState {
  /** User has opted in to hearing Travis speak. Default false. */
  enabled: boolean;
  /** Preferred voice URI. When null, we pick the first English voice
   *  that isn't a novelty. */
  preferredVoiceUri: string | null;
}

export function readVoiceState(): VoiceState {
  if (typeof localStorage === "undefined")
    return { enabled: false, preferredVoiceUri: null };
  return {
    enabled: localStorage.getItem(STORAGE_KEY) === "on",
    preferredVoiceUri: localStorage.getItem(STORAGE_VOICE_KEY),
  };
}

export function writeVoiceEnabled(on: boolean): void {
  if (typeof localStorage === "undefined") return;
  try {
    if (on) localStorage.setItem(STORAGE_KEY, "on");
    else localStorage.removeItem(STORAGE_KEY);
  } catch {
    /* ignore */
  }
}

export function writeVoicePreferredUri(uri: string | null): void {
  if (typeof localStorage === "undefined") return;
  try {
    if (uri) localStorage.setItem(STORAGE_VOICE_KEY, uri);
    else localStorage.removeItem(STORAGE_VOICE_KEY);
  } catch {
    /* ignore */
  }
}

/** List available voices. Handles the async-load quirk where the
 *  first getVoices() call can return an empty list until voices load. */
export function listVoices(): Promise<SpeechSynthesisVoice[]> {
  if (typeof speechSynthesis === "undefined") return Promise.resolve([]);
  const immediate = speechSynthesis.getVoices();
  if (immediate.length > 0) return Promise.resolve(immediate);
  return new Promise((resolve) => {
    const handler = () => {
      speechSynthesis.removeEventListener("voiceschanged", handler);
      resolve(speechSynthesis.getVoices());
    };
    speechSynthesis.addEventListener("voiceschanged", handler);
    // Fallback after a beat if the event never fires.
    window.setTimeout(() => {
      speechSynthesis.removeEventListener("voiceschanged", handler);
      resolve(speechSynthesis.getVoices());
    }, 1500);
  });
}

/** Pick a sensible default voice. Prefer English, prefer non-novelty
 *  (Fred/Zarvox/etc), prefer "en-US" if available. */
export async function defaultVoice(): Promise<SpeechSynthesisVoice | null> {
  const voices = await listVoices();
  if (voices.length === 0) return null;
  const noveltyRe = /fred|zarvox|whisper|bells|cellos|deranged|bad news|good news|hysterical|junior|kathy|pipe|princess|ralph|superstar|trinoids|bahh/i;
  const english = voices.filter((v) => v.lang.startsWith("en"));
  const cleaned = english.filter((v) => !noveltyRe.test(v.name));
  return cleaned.find((v) => v.lang === "en-US") ?? cleaned[0] ?? english[0] ?? voices[0];
}

/** v0.26 (v2 Shell 11b) — external hook so voice.speak can drive the
 *  speech-scene spheroid without importing the app store from a lib.
 *  App.tsx registers this once at mount. */
let onSpeechAmplitude: ((amp: number) => void) | null = null;
export function setSpeechAmplitudeSink(fn: ((amp: number) => void) | null) {
  onSpeechAmplitude = fn;
}

/** v0.28.26 — play a base64-encoded WAV returned by piper_speak.
 *  Uses Web Audio to run an AnalyserNode-driven envelope so the
 *  spheroid reacts to the *actual* Piper waveform instead of the
 *  synthesized word-boundary pulses we do for speechSynthesis. */
async function playPiperWav(b64: string): Promise<void> {
  const bin = atob(b64);
  const bytes = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i);
  const blob = new Blob([bytes], { type: "audio/wav" });
  const url = URL.createObjectURL(blob);
  const audio = new Audio(url);
  audio.volume = 0.95;
  currentPiperAudio = audio;

  // Web Audio graph for amplitude reactivity. AudioContext is scoped
  // to this utterance so it closes on end/error and doesn't leak.
  const AC = (window.AudioContext || (window as unknown as { webkitAudioContext: typeof AudioContext }).webkitAudioContext);
  let ctx: AudioContext | null = null;
  let analyser: AnalyserNode | null = null;
  let rafId: number | null = null;
  try {
    ctx = new AC();
    const src = ctx.createMediaElementSource(audio);
    analyser = ctx.createAnalyser();
    analyser.fftSize = 512;
    src.connect(analyser);
    analyser.connect(ctx.destination);
    const buf = new Uint8Array(analyser.frequencyBinCount);
    const tick = () => {
      if (!analyser) return;
      analyser.getByteTimeDomainData(buf);
      // Compute RMS as amplitude proxy, normalized ~[0,1].
      let sum = 0;
      for (let i = 0; i < buf.length; i++) {
        const v = (buf[i] - 128) / 128;
        sum += v * v;
      }
      const rms = Math.sqrt(sum / buf.length);
      onSpeechAmplitude?.(Math.min(1, rms * 3.2));
      rafId = window.requestAnimationFrame(tick);
    };
    tick();
  } catch (e) {
    console.warn("[voice] piper analyser init failed:", e);
  }

  return new Promise<void>((resolve) => {
    const cleanup = () => {
      if (rafId != null) window.cancelAnimationFrame(rafId);
      onSpeechAmplitude?.(0);
      if (ctx) {
        try { ctx.close(); } catch { /* ignore */ }
      }
      URL.revokeObjectURL(url);
      if (currentPiperAudio === audio) currentPiperAudio = null;
      resolve();
    };
    audio.onended = cleanup;
    audio.onerror = cleanup;
    void audio.play().catch((e) => {
      console.warn("[voice] audio.play failed:", e);
      cleanup();
    });
  });
}

/** v0.28.26 — Piper capability memo. First `speak` call probes the
 *  Rust side to see if the bundled Piper binary + voice model are
 *  present. Subsequent calls skip the probe. Null means unknown yet;
 *  false means confirmed unavailable (permanently fall back to OS
 *  speechSynthesis this session). */
let piperAvailability: boolean | null = null;

async function ensurePiperProbed(): Promise<boolean> {
  if (piperAvailability !== null) return piperAvailability;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    piperAvailability = Boolean(await invoke<boolean>("piper_available"));
  } catch {
    piperAvailability = false;
  }
  return piperAvailability;
}

/** v0.28.26 — playback state. We hold the active audio element so
 *  a new `speak` can cancel it (mirrors speechSynthesis.cancel()). */
let currentPiperAudio: HTMLAudioElement | null = null;

function cancelPiperPlayback() {
  if (currentPiperAudio) {
    try {
      currentPiperAudio.pause();
      currentPiperAudio.src = "";
    } catch {
      /* ignore */
    }
    currentPiperAudio = null;
  }
}

/** Speak `text` with the current preferences. Cancels any in-flight
 *  utterance. Returns a promise that resolves when speech finishes
 *  (or immediately if voice is disabled). */
export async function speak(text: string): Promise<void> {
  const state = readVoiceState();
  // v0.28.25 gating happens in ChatTurn; if we reach here TTS is on.
  // But keep the belt-and-suspenders check for direct callers.
  if (typeof speechSynthesis === "undefined" && !(await ensurePiperProbed()))
    return;
  if (!state.enabled && piperAvailability === false) return;
  cancelPiperPlayback();
  if (typeof speechSynthesis !== "undefined") speechSynthesis.cancel();

  const trimmed = text.trim();
  if (!trimmed) return;

  // v0.28.26 — try Piper first for a consistent Travis voice everywhere.
  // Fall back silently to the OS speechSynthesis if it isn't available
  // (dev build without predev, unsupported host from fetch-piper.mjs,
  // or a subprocess failure at runtime).
  if (await ensurePiperProbed()) {
    try {
      const { invoke } = await import("@tauri-apps/api/core");
      const b64 = await invoke<string>("piper_speak", { text: trimmed });
      await playPiperWav(b64);
      return;
    } catch (err) {
      console.warn("[voice] piper_speak failed, falling back to speechSynthesis:", err);
      piperAvailability = false; // don't keep retrying this session
    }
  }

  const voices = await listVoices();
  const preferred = state.preferredVoiceUri
    ? voices.find((v) => v.voiceURI === state.preferredVoiceUri)
    : undefined;
  const voice = preferred ?? (await defaultVoice());

  return new Promise((resolve) => {
    const u = new SpeechSynthesisUtterance(trimmed);
    if (voice) u.voice = voice;
    u.rate = 1.05;
    u.pitch = 1.0;
    u.volume = 0.9;

    // v0.26 (v2 Shell 11b) — SpeechSynthesis doesn't expose amplitude,
    // so we synthesize an envelope: pulse to ~0.75 on each word
    // boundary, decay toward 0 between. Gives the spheroid a talking
    // rhythm rather than a flat 'is speaking' state.
    let decayTimer: number | null = null;
    const stopEnvelope = () => {
      if (decayTimer != null) {
        window.clearInterval(decayTimer);
        decayTimer = null;
      }
      onSpeechAmplitude?.(0);
    };
    let currentAmp = 0;
    const decayStep = () => {
      currentAmp = Math.max(0, currentAmp - 0.06);
      onSpeechAmplitude?.(currentAmp);
      if (currentAmp <= 0.001 && decayTimer != null) {
        window.clearInterval(decayTimer);
        decayTimer = null;
      }
    };
    u.onboundary = () => {
      // Small random variance so pulses don't feel mechanical.
      currentAmp = 0.65 + Math.random() * 0.2;
      onSpeechAmplitude?.(currentAmp);
      if (decayTimer == null) {
        decayTimer = window.setInterval(decayStep, 45);
      }
    };
    u.onstart = () => {
      currentAmp = 0.4;
      onSpeechAmplitude?.(currentAmp);
    };
    u.onend = () => {
      stopEnvelope();
      resolve();
    };
    u.onerror = () => {
      stopEnvelope();
      resolve();
    };
    speechSynthesis.speak(u);
  });
}

/** Cancel any in-flight speech. */
export function cancelSpeech(): void {
  if (typeof speechSynthesis !== "undefined") speechSynthesis.cancel();
}

// v0.28 — barge-in wiring. The native VAD fires this event whenever it
// detects speech-start while Travis is speaking. Registering here so
// TTS gets interrupted the moment the user starts talking, matching
// natural conversation feel.
if (typeof window !== "undefined") {
  window.addEventListener("travis:piper-stop", () => {
    cancelSpeech();
  });
}
