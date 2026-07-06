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

/** Speak `text` with the current preferences. Cancels any in-flight
 *  utterance. Returns a promise that resolves when speech finishes
 *  (or immediately if voice is disabled). */
export async function speak(text: string): Promise<void> {
  if (typeof speechSynthesis === "undefined") return;
  const state = readVoiceState();
  if (!state.enabled) return;
  speechSynthesis.cancel();

  const trimmed = text.trim();
  if (!trimmed) return;

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
