/**
 * Push-to-talk voice input (v0.22.12).
 *
 * Hold to record, release to transcribe. Runs entirely local via
 * whisper.cpp. The first press kicks off the model download if it's
 * not cached — the shared ResourceLoader overlay handles the loader
 * UX so this button just says "getting ready" while it waits.
 *
 * Ambient "Hey Travis" wake-word listening lands separately in a
 * follow-up (task 306). This is the manual-invoke MVP.
 */
import { useEffect, useRef, useState } from "react";
import { speechRuntimeEnsure, speechTranscribe } from "../lib/speechRuntime";
import { useAppStore } from "../stores/app";

type State =
  | { kind: "idle" }
  | { kind: "requesting" }
  | { kind: "recording"; started: number }
  | { kind: "transcribing" }
  | { kind: "error"; message: string };

interface Props {
  onTranscript(text: string): void;
  /** Disable while another op is in flight so we don't stack requests. */
  disabled?: boolean;
}

const SAMPLE_RATE = 16000;
const MIN_HOLD_MS = 200; // avoid accidental clicks

export function VoiceInputButton({ onTranscript, disabled }: Props) {
  const [state, setState] = useState<State>({ kind: "idle" });
  const audioContextRef = useRef<AudioContext | null>(null);
  const streamRef = useRef<MediaStream | null>(null);
  const samplesRef = useRef<Float32Array[]>([]);
  const processorRef = useRef<ScriptProcessorNode | null>(null);
  const sourceRef = useRef<MediaStreamAudioSourceNode | null>(null);
  // v0.26 (v2 Shell 11b) — pipe live RMS amplitude into the store so
  // the SpeechScene spheroid can pulse with what the mic hears.
  const setSpeechAmplitude = useAppStore((s) => s.setSpeechAmplitude);

  const cleanup = () => {
    processorRef.current?.disconnect();
    sourceRef.current?.disconnect();
    if (streamRef.current) {
      streamRef.current.getTracks().forEach((t) => t.stop());
      streamRef.current = null;
    }
    if (audioContextRef.current) {
      audioContextRef.current.close().catch(() => {});
      audioContextRef.current = null;
    }
    samplesRef.current = [];
    processorRef.current = null;
    sourceRef.current = null;
    // Zero the amplitude signal so the spheroid can fade out cleanly.
    setSpeechAmplitude(0);
  };

  useEffect(() => cleanup, []);

  // v0.26 (v2 Shell 12) — global wake shortcut. App.tsx dispatches
  // 'travis:wake' when Ctrl+Alt+Space is pressed system-wide. Auto-arm
  // the mic if we're currently idle. If a recording is in flight, the
  // event is a no-op (beginRecording bails). We don't need this to be
  // press-and-hold-aware — user tap-starts, taps again (or hits mic)
  // to stop.
  useEffect(() => {
    function onWake() {
      if (state.kind === "idle" || state.kind === "error") {
        void beginRecording();
      }
    }
    window.addEventListener("travis:wake", onWake as EventListener);
    return () =>
      window.removeEventListener("travis:wake", onWake as EventListener);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [state.kind]);

  async function beginRecording() {
    if (disabled) return;
    if (state.kind !== "idle" && state.kind !== "error") return;
    setState({ kind: "requesting" });
    try {
      // Kick the model bootstrap early — it's a no-op if cached.
      speechRuntimeEnsure().catch(() => {});
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          channelCount: 1,
          echoCancellation: true,
          noiseSuppression: true,
          autoGainControl: true,
        },
      });
      streamRef.current = stream;
      const ctx = new AudioContext({ sampleRate: SAMPLE_RATE });
      audioContextRef.current = ctx;
      const source = ctx.createMediaStreamSource(stream);
      sourceRef.current = source;
      // ScriptProcessorNode is deprecated but the AudioWorklet path
      // requires a separate worklet file. For MVP the deprecated node
      // works fine and doesn't affect first-run friction.
      const processor = ctx.createScriptProcessor(4096, 1, 1);
      processorRef.current = processor;
      processor.onaudioprocess = (e) => {
        const input = e.inputBuffer.getChannelData(0);
        // Copy — otherwise the underlying buffer gets recycled.
        samplesRef.current.push(new Float32Array(input));
        // Compute RMS of this ~85ms buffer (4096 samples @ 48kHz or
        // similar) and pipe it to the store. Mapped to [0..1] with
        // a soft compressor — RMS rarely exceeds 0.5 on speech.
        let sumSq = 0;
        for (let i = 0; i < input.length; i++) sumSq += input[i] * input[i];
        const rms = Math.sqrt(sumSq / input.length);
        const scaled = Math.min(1, rms * 5.5);
        setSpeechAmplitude(scaled);
      };
      source.connect(processor);
      processor.connect(ctx.destination);
      setState({ kind: "recording", started: Date.now() });
    } catch (e) {
      cleanup();
      setState({
        kind: "error",
        message: e instanceof Error ? e.message : String(e),
      });
    }
  }

  async function endRecording() {
    if (state.kind !== "recording") return;
    const held = Date.now() - state.started;
    // Below the click threshold — probably an accidental tap. Cancel.
    if (held < MIN_HOLD_MS) {
      cleanup();
      setState({ kind: "idle" });
      return;
    }
    setState({ kind: "transcribing" });
    try {
      const total = samplesRef.current.reduce((n, c) => n + c.length, 0);
      const merged = new Float32Array(total);
      let offset = 0;
      for (const chunk of samplesRef.current) {
        merged.set(chunk, offset);
        offset += chunk.length;
      }
      cleanup();
      const text = await speechTranscribe(merged);
      const trimmed = text.trim();
      if (trimmed.length > 0) onTranscript(trimmed);
      setState({ kind: "idle" });
    } catch (e) {
      cleanup();
      setState({
        kind: "error",
        message: e instanceof Error ? e.message : String(e),
      });
    }
  }

  const label = (() => {
    switch (state.kind) {
      case "requesting":
        return "getting ready…";
      case "recording":
        return "listening";
      case "transcribing":
        return "transcribing…";
      case "error":
        return "mic error";
      default:
        return "hold to speak";
    }
  })();

  const isActive = state.kind === "recording";
  const isBusy = state.kind === "requesting" || state.kind === "transcribing";

  return (
    <button
      onMouseDown={beginRecording}
      onMouseUp={endRecording}
      onMouseLeave={endRecording}
      onTouchStart={(e) => {
        e.preventDefault();
        void beginRecording();
      }}
      onTouchEnd={(e) => {
        e.preventDefault();
        void endRecording();
      }}
      disabled={disabled}
      title={label}
      className="shrink-0 h-9 w-9 flex items-center justify-center rounded-full transition-colors disabled:opacity-40 disabled:cursor-not-allowed"
      style={{
        background: isActive
          ? "rgba(255, 107, 107, 0.20)"
          : isBusy
            ? "rgba(124, 92, 255, 0.15)"
            : "rgba(255, 255, 255, 0.03)",
        border: `1px solid ${
          isActive
            ? "rgba(255, 107, 107, 0.55)"
            : "rgba(255, 255, 255, 0.10)"
        }`,
      }}
    >
      {isActive ? (
        <span
          className="block h-2 w-2 rounded-full"
          style={{
            background: "rgb(255, 107, 107)",
            boxShadow: "0 0 10px rgba(255, 107, 107, 0.7)",
            animation: "pulse 1.2s ease-in-out infinite",
          }}
        />
      ) : isBusy ? (
        <span
          className="block h-1.5 w-1.5 rounded-full bg-pulse-2 animate-pulse"
        />
      ) : (
        <svg
          viewBox="0 0 20 20"
          width="14"
          height="14"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.6"
          strokeLinecap="round"
          strokeLinejoin="round"
          style={{ color: "rgba(236, 236, 241, 0.7)" }}
        >
          <rect x="7" y="3" width="6" height="10" rx="3" />
          <path d="M4 10a6 6 0 0 0 12 0M10 16v2" />
        </svg>
      )}
    </button>
  );
}
