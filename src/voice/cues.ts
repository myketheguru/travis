/**
 * cues — v0.28.2.
 *
 * Short synthesized cue tones for the voice pipeline. Uses Web Audio
 * OscillatorNodes so no assets need bundling. The bells are calm two-
 * tone glides in the ~600-900 Hz range so they don't stand out
 * harshly against Travis's other UI.
 *
 * Design intent (Siri/Alexa parlance):
 *   wake      — Travis is listening, go ahead
 *   heard     — Travis got your utterance, transcribing
 *   done      — Travis finished responding, over to you
 *   error     — Something couldn't be done (kept muted)
 */

type CueName = "wake" | "heard" | "done" | "error";

let ctx: AudioContext | null = null;

function getCtx(): AudioContext | null {
  if (typeof window === "undefined") return null;
  if (ctx && ctx.state !== "closed") return ctx;
  try {
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const Ctor: typeof AudioContext =
      window.AudioContext ??
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      (window as any).webkitAudioContext;
    ctx = new Ctor();
    return ctx;
  } catch {
    return null;
  }
}

interface Tone {
  freq: number;
  durMs: number;
  gain?: number;
  waveform?: OscillatorType;
}

const SEQUENCES: Record<CueName, Tone[]> = {
  // Two-tone ascending: 660 -> 880 Hz, ~120ms each
  wake: [
    { freq: 660, durMs: 100 },
    { freq: 880, durMs: 130 },
  ],
  // Single soft tone: 720 Hz
  heard: [{ freq: 720, durMs: 90 }],
  // Two-tone descending: 880 -> 660
  done: [
    { freq: 880, durMs: 100 },
    { freq: 660, durMs: 130 },
  ],
  // Low soft blip
  error: [{ freq: 320, durMs: 140, gain: 0.05 }],
};

/**
 * Play a named cue. Best-effort — silent on failure, never throws.
 * The user should NEVER see or hear an error from this file itself.
 */
export function playCue(name: CueName): void {
  try {
    const c = getCtx();
    if (!c) return;
    // Resume if suspended (autoplay policy).
    if (c.state === "suspended") {
      void c.resume();
    }
    const tones = SEQUENCES[name];
    let t = c.currentTime + 0.005;
    for (const tone of tones) {
      const dur = tone.durMs / 1000;
      const osc = c.createOscillator();
      osc.type = tone.waveform ?? "sine";
      osc.frequency.setValueAtTime(tone.freq, t);
      const gain = c.createGain();
      const peak = tone.gain ?? 0.12;
      gain.gain.setValueAtTime(0, t);
      gain.gain.linearRampToValueAtTime(peak, t + 0.01);
      gain.gain.linearRampToValueAtTime(peak, t + dur - 0.02);
      gain.gain.exponentialRampToValueAtTime(0.0001, t + dur);
      osc.connect(gain);
      gain.connect(c.destination);
      osc.start(t);
      osc.stop(t + dur + 0.01);
      t += dur;
    }
  } catch {
    // Cues are cosmetic; never propagate an error.
  }
}
