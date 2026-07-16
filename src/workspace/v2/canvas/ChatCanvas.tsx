/**
 * ChatCanvas — v0.27.2 rewrite.
 *
 * Renders BOTH user and assistant messages chronologically. Latest
 * message is centered + full opacity; older messages scale + fade as
 * they age. Scrolling shifts focus naturally.
 *
 * Optimistic composer: when pendingComposerSubmit is set, we show the
 * user's message immediately (before the DB round-trip) so the canvas
 * feels alive. The optimistic bubble is replaced by the real DB row
 * on the next poll.
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { motion } from "framer-motion";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useAppStore } from "../../../stores/app";
import { useFocalContent } from "../useFocalContent";
import { parseRichResponse } from "../../../lib/richResponse";
import { RichResponseRenderer } from "../../../chat/cards/RichResponseRenderer";
import { MarkdownBody } from "../../../chat/MarkdownBody";
import { VoiceMessageCard } from "./VoiceMessageCard";
import type { ConversationMessage } from "../../../lib/conversation";

interface InlineAudio {
  audioPath: string;
  durationMs: number;
  transcript: string;
}

interface RenderMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  pending?: boolean;
  optimistic?: boolean;
  audio?: InlineAudio;
}

export function ChatCanvas() {
  const activity = useAppStore((s) => s.activity);
  const voiceTranscribing = useAppStore((s) => s.voiceTranscribing);
  const pendingComposerSubmit = useAppStore((s) => s.pendingComposerSubmit);
  const pendingVoiceAudio = useAppStore((s) => s.pendingVoiceAudio);
  const voiceAudioLinkedMessageId = useAppStore((s) => s.voiceAudioLinkedMessageId);
  const setPendingVoiceAudio = useAppStore((s) => s.setPendingVoiceAudio);
  const setVoiceAudioLinkedMessageId = useAppStore((s) => s.setVoiceAudioLinkedMessageId);
  const { allMessages } = useFocalContent();
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const lastPendingRef = useRef<string | null>(null);

  const optimistic = useOptimisticSubmit(
    pendingComposerSubmit,
    pendingVoiceAudio,
    allMessages,
    lastPendingRef,
  );

  // v0.28.63 — once the DB user message that the audio was linked to
  // is present in the current thread, VoiceMessageCard will render
  // for it via its numeric id. Clear the persistent pending state so
  // the in-flight card and the real card don't double up.
  useEffect(() => {
    if (voiceAudioLinkedMessageId === null) return;
    const found = allMessages.some(
      (m) => m.id === voiceAudioLinkedMessageId,
    );
    if (found) {
      setPendingVoiceAudio(null);
      setVoiceAudioLinkedMessageId(null);
    }
  }, [
    voiceAudioLinkedMessageId,
    allMessages,
    setPendingVoiceAudio,
    setVoiceAudioLinkedMessageId,
  ]);

  const rendered: RenderMessage[] = useMemo(() => {
    const base: RenderMessage[] = allMessages
      .filter((m) => m.role === "user" || m.role === "assistant" || m.role === "system")
      .map((m) => ({
        id: String(m.id),
        role: m.role as RenderMessage["role"],
        content: m.content,
      }));
    if (optimistic) base.push(optimistic);
    // v0.28.61 — voice-transcribing user bubble now carries the audio
    // metadata inline (if capture already emitted it) so the audio
    // player appears the moment recording ends, not after whisper +
    // journal round-trip. Skip if the optimistic composer submit
    // already replaced it.
    if (voiceTranscribing && !optimistic) {
      base.push({
        id: "__voice_transcribing__",
        role: "user",
        content: pendingVoiceAudio?.transcript ?? "",
        optimistic: true,
        audio: pendingVoiceAudio ?? undefined,
      });
    }
    // v0.28.61 — killed the "..." pending-assistant bubble. Users
    // reported it as a permanent eyesore during the 5-15s LLM turn.
    // The composer's own thinking-state indicator (glow border +
    // spinner, shipped in v0.28.44) already signals that Travis is
    // working. When streaming lands, tokens fill the assistant bubble
    // progressively instead of being preceded by dots.
    return base;
  }, [allMessages, optimistic, voiceTranscribing, pendingVoiceAudio]);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
  }, [rendered.length, activity]);

  if (rendered.length === 0) {
    return <EmptyChatCanvas />;
  }

  return (
    <div
      ref={scrollRef}
      className="absolute inset-0 overflow-y-auto scroll-smooth"
      style={{ paddingBottom: "160px", paddingTop: "18vh" }}
    >
      <div className="max-w-3xl mx-auto px-6 flex flex-col gap-6">
        {rendered.map((m, i) => (
          <MessageBlock
            key={m.id}
            message={m}
            focusLevel={levelFor(i, rendered.length)}
          />
        ))}
        <div style={{ height: "18vh" }} />
      </div>
    </div>
  );
}

/**
 * Synthesize a user bubble the instant Composer fires
 * pendingComposerSubmit. Cleared once the matching user message
 * appears in the polled thread.
 */
function useOptimisticSubmit(
  pending: string | null,
  pendingAudio: InlineAudio | null,
  allMessages: ConversationMessage[],
  lastRef: React.MutableRefObject<string | null>,
): RenderMessage | null {
  if (pending) lastRef.current = pending;
  const seen = lastRef.current;
  if (!seen) return null;
  const alreadyThere = allMessages
    .slice(-6)
    .some((m) => m.role === "user" && m.content.trim() === seen.trim());
  if (alreadyThere) {
    lastRef.current = null;
    return null;
  }
  return {
    id: "__optimistic_user__",
    role: "user",
    content: seen,
    optimistic: true,
    // v0.28.63 — read audio live from store. In v0.28.61 we snapshotted
    // into a useRef so we wouldn't lose it when the journal://user-inserted
    // listener cleared pendingVoiceAudio, but that broke the voice→chat
    // canvas-mode transition: ChatCanvas mounts fresh and the ref starts
    // null. Now pendingVoiceAudio stays in the store until the real
    // linked message appears in the thread (see the useEffect above),
    // so a fresh mount reads the current live value.
    audio: pendingAudio ?? undefined,
  };
}

function levelFor(index: number, total: number): number {
  if (total === 0) return 0.5;
  const dist = total - 1 - index;
  if (dist === 0) return 1;
  if (dist === 1) return 0.75;
  if (dist === 2) return 0.55;
  if (dist === 3) return 0.42;
  return 0.32;
}

function MessageBlock({
  message,
  focusLevel,
}: {
  message: RenderMessage;
  focusLevel: number;
}) {
  const isUser = message.role === "user";
  const rich = !isUser && message.content
    ? parseRichResponse(message.content)
    : null;

  return (
    <motion.div
      data-msg-id={message.id}
      layout
      initial={{ opacity: 0, y: 12 }}
      animate={{
        opacity: focusLevel,
        y: 0,
        scale: 0.94 + focusLevel * 0.06,
      }}
      transition={{ duration: 0.34, ease: [0.22, 1, 0.36, 1] }}
      className="w-full"
    >
      {isUser ? (
        <div className="flex justify-end">
          <div
            className="rounded-2xl px-4 py-2.5 max-w-[80%]"
            style={{
              background: "rgba(124, 92, 255, 0.14)",
              border: message.optimistic
                ? "1px dashed rgba(189, 158, 255, 0.45)"
                : "1px solid rgba(124, 92, 255, 0.32)",
              color: "rgba(236, 236, 241, 0.98)",
              fontSize: 14 + focusLevel * 3,
              lineHeight: 1.5,
            }}
          >
            {/^-?\d+$/.test(message.id) && !message.optimistic ? (
              <VoiceMessageCard
                messageId={Number(message.id)}
                transcriptFallback={message.content}
              />
            ) : message.audio ? (
              <InlineVoiceBubble audio={message.audio} />
            ) : (
              message.content
            )}
          </div>
        </div>
      ) : (
        <div>
          <div
            className="text-[10px] uppercase tracking-[0.22em] font-mono mb-2"
            style={{ color: `rgba(236, 236, 241, ${0.35 * focusLevel})` }}
          >
            Travis
          </div>
          {(
            <div
              style={{
                fontSize: 14 + focusLevel * 3,
                lineHeight: 1.55,
                color: `rgba(236, 236, 241, ${0.75 + focusLevel * 0.2})`,
              }}
            >
              {rich ? (
                <RichResponseRenderer response={rich} messageId={message.id} />
              ) : (
                <MarkdownBody text={message.content} />
              )}
            </div>
          )}
        </div>
      )}
    </motion.div>
  );
}

/**
 * Compact audio player rendered inside the optimistic user bubble.
 * v0.28.61 — decoupled from VoiceMessageCard so we can render the
 * moment the recording ends (no DB round-trip required — the audio
 * path is already local on disk and the transcript comes back from
 * whisper before we even fire the composer submit).
 */
function InlineVoiceBubble({ audio }: { audio: InlineAudio }) {
  const audioRef = useRef<HTMLAudioElement | null>(null);
  const [playing, setPlaying] = useState(false);
  const [pos, setPos] = useState(0);
  const src = convertFileSrc(audio.audioPath);
  const durationSec = Math.max(0, audio.durationMs / 1000);
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
            if (el.paused) void el.play();
            else el.pause();
          }}
          className="shrink-0 h-9 w-9 rounded-full flex items-center justify-center transition-transform"
          style={{ background: "rgb(189, 158, 255)", color: "rgb(20, 18, 30)" }}
          aria-label={playing ? "Pause" : "Play"}
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
      {audio.transcript && (
        <div
          className="text-[13.5px] leading-relaxed"
          style={{ color: "rgba(236, 236, 241, 0.92)" }}
        >
          {audio.transcript}
        </div>
      )}
    </div>
  );
}

function EmptyChatCanvas() {
  return (
    <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
      <motion.div
        initial={{ opacity: 0, y: 8 }}
        animate={{ opacity: 0.6, y: 0 }}
        transition={{ duration: 0.5, ease: [0.22, 1, 0.36, 1] }}
        className="text-center"
      >
        <div
          className="text-[13px] font-mono uppercase tracking-[0.24em]"
          style={{ color: "rgba(236, 236, 241, 0.4)" }}
        >
          canvas
        </div>
        <div
          className="text-[16px] mt-3"
          style={{ color: "rgba(236, 236, 241, 0.6)" }}
        >
          Ask, request, or just start typing.
        </div>
      </motion.div>
    </div>
  );
}
