/**
 * VoiceMessageCard — v0.28.19.
 *
 * Renders a user message that came in via voice: a compact audio
 * player at the top with the WAV file loaded, plus the transcript
 * below in a collapsible section (open by default). Play the audio
 * anytime; collapse the transcript when the row gets long.
 */
import { useEffect, useRef, useState } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { nativeVoice, type VoiceUtterance } from "../../../lib/nativeVoice";

interface Props {
  messageId: number;
  transcriptFallback: string;
}

export function VoiceMessageCard({ messageId, transcriptFallback }: Props) {
  const [meta, setMeta] = useState<VoiceUtterance | null>(null);
  const [expanded, setExpanded] = useState(true);
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [playing, setPlaying] = useState(false);
  const [pos, setPos] = useState(0);

  useEffect(() => {
    let cancelled = false;
    nativeVoice
      .utteranceForMessage(messageId)
      .then((v) => {
        if (!cancelled) setMeta(v ?? null);
      })
      .catch(() => {
        /* soft-fail — falls back to text-only render */
      });
    return () => {
      cancelled = true;
    };
  }, [messageId]);

  if (!meta) return <div className="text-[13.5px] leading-relaxed">{transcriptFallback}</div>;

  const src = convertFileSrc(meta.audioPath);
  const durationSec = Math.max(0, meta.durationMs / 1000);
  const seconds = Math.floor(durationSec);
  const mins = Math.floor(seconds / 60);
  const secs = seconds % 60;
  const durationLabel = `${mins}:${String(secs).padStart(2, "0")}`;

  return (
    <div className="flex flex-col gap-2">
      <div
        className="rounded-2xl px-3 py-2 flex items-center gap-3"
        style={{
          background:
            "linear-gradient(180deg, rgba(189, 158, 255, 0.10), rgba(124, 92, 255, 0.06))",
          border: "1px solid rgba(189, 158, 255, 0.30)",
        }}
      >
        <button
          onClick={() => {
            const el = audioRef.current;
            if (!el) return;
            if (el.paused) {
              void el.play();
            } else {
              el.pause();
            }
          }}
          className="shrink-0 h-9 w-9 rounded-full flex items-center justify-center transition-transform"
          style={{
            background: "rgb(189, 158, 255)",
            color: "rgb(20, 18, 30)",
          }}
          aria-label={playing ? "Pause" : "Play"}
          title={playing ? "Pause" : "Play"}
        >
          {playing ? (
            <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
              <rect x="6" y="5" width="4" height="14" rx="1" />
              <rect x="14" y="5" width="4" height="14" rx="1" />
            </svg>
          ) : (
            <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden>
              <path d="M7 5v14l12-7z" />
            </svg>
          )}
        </button>
        <div className="flex-1 min-w-0">
          <div
            className="h-1.5 rounded-full overflow-hidden"
            style={{ background: "rgba(189, 158, 255, 0.15)" }}
          >
            <div
              className="h-full rounded-full transition-[width] duration-150"
              style={{
                width: `${durationSec === 0 ? 0 : Math.min(100, (pos / durationSec) * 100)}%`,
                background: "rgb(189, 158, 255)",
              }}
            />
          </div>
        </div>
        <span
          className="text-[11px] font-mono tabular-nums"
          style={{ color: "rgba(236, 236, 241, 0.7)" }}
        >
          {durationLabel}
        </span>
        <audio
          ref={audioRef}
          src={src}
          preload="metadata"
          onPlay={() => setPlaying(true)}
          onPause={() => setPlaying(false)}
          onEnded={() => {
            setPlaying(false);
            setPos(0);
          }}
          onTimeUpdate={(e) => setPos((e.target as HTMLAudioElement).currentTime)}
        />
      </div>
      <button
        onClick={() => setExpanded((v) => !v)}
        className="self-start text-[10.5px] uppercase tracking-[0.22em] font-mono"
        style={{ color: "rgba(189, 158, 255, 0.80)" }}
      >
        {expanded ? "hide transcript" : "show transcript"}
      </button>
      {expanded && (
        <div
          className="text-[13.5px] leading-relaxed"
          style={{ color: "rgba(236, 236, 241, 0.92)" }}
        >
          {meta.transcript}
        </div>
      )}
    </div>
  );
}
