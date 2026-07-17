/**
 * ChatCanvas — v0.28.66 lobe-chat-style rewrite.
 *
 * Message anatomy adopted from lobehub/lobe-chat verbatim (source
 * refs in the merge concept):
 *   - User: bubble (right-aligned, dashed-purple for optimistic,
 *     solid for persisted). Travis identity retained here.
 *   - Assistant: BUBBLELESS. Avatar + author label + timestamp +
 *     free-flowing markdown content on backdrop. Hover-reveal
 *     action strip (Copy / Regenerate / Fork / Save) that fades in
 *     on row hover at 200ms motionEaseOut.
 *   - Streaming: live in-flight bubble driven by store's
 *     `streamingAssistant`, updates as journal://assistant-chunk
 *     events flow. Cursor blinks at the tail while streaming;
 *     disappears when journal://assistant-done clears the slot.
 *   - Model chip: HIDDEN. Travis Cloud is the only surface — users
 *     never see the underlying provider or model name.
 *
 * Travis-native features preserved:
 *   - Voice audio card inside user bubble (persistent, decoupled
 *     from canvas mount lifecycle).
 *   - Rich response cards rendered inline in assistant content.
 *   - Focal-item fade/scale on message rows.
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
  optimistic?: boolean;
  streaming?: boolean;
  audio?: InlineAudio;
  createdAt?: string;
}

export function ChatCanvas() {
  const activity = useAppStore((s) => s.activity);
  const voiceTranscribing = useAppStore((s) => s.voiceTranscribing);
  const pendingComposerSubmit = useAppStore((s) => s.pendingComposerSubmit);
  const pendingVoiceAudio = useAppStore((s) => s.pendingVoiceAudio);
  const voiceAudioLinkedMessageId = useAppStore((s) => s.voiceAudioLinkedMessageId);
  const setPendingVoiceAudio = useAppStore((s) => s.setPendingVoiceAudio);
  const setVoiceAudioLinkedMessageId = useAppStore((s) => s.setVoiceAudioLinkedMessageId);
  const streamingAssistant = useAppStore((s) => s.streamingAssistant);
  const activeConversationId = useAppStore((s) => s.activeConversationId);
  const { allMessages } = useFocalContent();
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const lastPendingRef = useRef<string | null>(null);

  const optimistic = useOptimisticSubmit(
    pendingComposerSubmit,
    pendingVoiceAudio,
    allMessages,
    lastPendingRef,
  );

  // v0.28.63 → v0.28.66 — clear the persistent voice-audio state
  // once the DB user message that carries the linked audio appears
  // in the current thread. VoiceMessageCard (numeric-id path) takes
  // over rendering from InlineVoiceBubble at that instant.
  useEffect(() => {
    if (voiceAudioLinkedMessageId === null) return;
    if (allMessages.some((m) => m.id === voiceAudioLinkedMessageId)) {
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
        createdAt: m.createdAt,
      }));
    if (optimistic) base.push(optimistic);
    if (voiceTranscribing && !optimistic && pendingVoiceAudio) {
      base.push({
        id: "__voice_transcribing__",
        role: "user",
        content: pendingVoiceAudio.transcript,
        optimistic: true,
        audio: pendingVoiceAudio,
      });
    }
    // v0.28.66 — live streaming assistant bubble. Renders whenever
    // the store has a streamingAssistant slot for the current
    // conversation AND the persisted row hasn't landed yet (checked
    // by comparing content prefix to avoid a flash-of-duplicate).
    if (
      streamingAssistant &&
      streamingAssistant.conversationId === (activeConversationId ?? -1) &&
      streamingAssistant.content.length > 0
    ) {
      const already = allMessages.some(
        (m) =>
          m.role === "assistant" &&
          m.content.startsWith(streamingAssistant.content.slice(0, 40)),
      );
      if (!already) {
        base.push({
          id: "__streaming_assistant__",
          role: "assistant",
          content: streamingAssistant.content,
          streaming: true,
        });
      }
    }
    return base;
  }, [
    allMessages,
    optimistic,
    voiceTranscribing,
    pendingVoiceAudio,
    streamingAssistant,
    activeConversationId,
  ]);

  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
  }, [rendered.length, activity]);

  // Auto-scroll during streaming so new tokens stay in view. Skip
  // if the user has scrolled up manually (delta > 200 from bottom
  // means they're reading history — don't yank them down).
  useEffect(() => {
    if (!streamingAssistant) return;
    const el = scrollRef.current;
    if (!el) return;
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    if (distanceFromBottom < 200) {
      el.scrollTo({ top: el.scrollHeight, behavior: "auto" });
    }
  }, [streamingAssistant?.content, streamingAssistant]);

  if (rendered.length === 0) {
    return <EmptyChatCanvas />;
  }

  return (
    <div
      ref={scrollRef}
      className="absolute inset-0 overflow-y-auto scroll-smooth"
      style={{ paddingBottom: "160px", paddingTop: "18vh" }}
    >
      <div className="max-w-3xl mx-auto px-6 flex flex-col gap-8">
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
    audio: pendingAudio ?? undefined,
  };
}

function levelFor(index: number, total: number): number {
  if (total === 0) return 0.5;
  const dist = total - 1 - index;
  if (dist === 0) return 1;
  if (dist === 1) return 0.82;
  if (dist === 2) return 0.68;
  if (dist === 3) return 0.55;
  return 0.42;
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

  if (isUser) {
    return <UserRow message={message} focusLevel={focusLevel} />;
  }
  return (
    <AssistantRow
      message={message}
      focusLevel={focusLevel}
      rich={rich}
    />
  );
}

/**
 * User message row — Travis's dashed-purple bubble identity, right
 * aligned. Retains inline voice audio card (v0.28.61+) and
 * VoiceMessageCard for persisted voice messages.
 */
function UserRow({
  message,
  focusLevel,
}: {
  message: RenderMessage;
  focusLevel: number;
}) {
  const timeLabel = message.createdAt ? formatTime(message.createdAt) : null;
  return (
    <motion.div
      data-msg-id={message.id}
      layout
      initial={{ opacity: 0, y: 12 }}
      animate={{
        opacity: focusLevel,
        y: 0,
        scale: 0.96 + focusLevel * 0.04,
      }}
      transition={{ duration: 0.34, ease: [0.22, 1, 0.36, 1] }}
      className="w-full group"
    >
      <div className="flex flex-col items-end gap-1.5">
        {timeLabel && (
          <div
            className="text-[10px] uppercase tracking-[0.22em] font-mono opacity-0 group-hover:opacity-100 transition-opacity duration-200 pr-1"
            style={{ color: "rgba(236, 236, 241, 0.5)" }}
          >
            {timeLabel}
          </div>
        )}
        <div
          className="rounded-2xl px-4 py-2.5 max-w-[80%]"
          style={{
            background: "rgba(124, 92, 255, 0.12)",
            border: message.optimistic
              ? "1px dashed rgba(189, 158, 255, 0.42)"
              : "1px solid rgba(124, 92, 255, 0.30)",
            borderBottomRightRadius: "6px",
            color: "rgba(236, 236, 241, 0.98)",
            fontSize: 14 + focusLevel * 2,
            lineHeight: 1.55,
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
    </motion.div>
  );
}

/**
 * Assistant message row — lobe-chat anatomy. Avatar + author label +
 * timestamp + content flush left, no bubble. Hover reveals action
 * strip (Copy/Regenerate/Fork/Save). Streaming variant renders a
 * blinking lavender cursor at the tail.
 */
function AssistantRow({
  message,
  focusLevel,
  rich,
}: {
  message: RenderMessage;
  focusLevel: number;
  rich: ReturnType<typeof parseRichResponse>;
}) {
  const timeLabel = message.createdAt ? formatTime(message.createdAt) : null;

  return (
    <motion.div
      data-msg-id={message.id}
      layout
      initial={{ opacity: 0, y: 12 }}
      animate={{
        opacity: focusLevel,
        y: 0,
        scale: 0.96 + focusLevel * 0.04,
      }}
      transition={{ duration: 0.34, ease: [0.22, 1, 0.36, 1] }}
      className="w-full group"
    >
      <div className="flex gap-3">
        <TravisAvatar streaming={message.streaming} />
        <div className="flex-1 min-w-0 flex flex-col gap-1.5">
          <div className="flex items-baseline gap-3">
            <span
              className="text-[10px] uppercase tracking-[0.22em] font-mono"
              style={{ color: `rgba(236, 236, 241, ${0.85 * focusLevel})` }}
            >
              Travis
            </span>
            {timeLabel && (
              <span
                className="text-[10px] tracking-[0.16em] font-mono opacity-0 group-hover:opacity-100 transition-opacity duration-200 tabular-nums"
                style={{ color: "rgba(236, 236, 241, 0.5)" }}
              >
                {timeLabel}
              </span>
            )}
          </div>
          <div
            style={{
              fontSize: 14 + focusLevel * 2,
              lineHeight: 1.6,
              color: `rgba(236, 236, 241, ${0.85 + focusLevel * 0.12})`,
            }}
          >
            {message.streaming ? (
              <div>
                <MarkdownBody text={message.content} />
                <StreamingCursor />
              </div>
            ) : rich ? (
              <RichResponseRenderer response={rich} messageId={message.id} />
            ) : (
              <MarkdownBody text={message.content} />
            )}
          </div>
          {/* Hover-reveal action strip — only on persisted assistant messages */}
          {!message.streaming && /^-?\d+$/.test(message.id) && (
            <div className="opacity-0 group-hover:opacity-100 transition-opacity duration-200 pt-1">
              <MessageActions content={message.content} />
            </div>
          )}
        </div>
      </div>
    </motion.div>
  );
}

/**
 * Silvery-bronze-lavender avatar orb for the assistant — Travis's
 * identity, distilled. Shimmers while streaming.
 */
function TravisAvatar({ streaming }: { streaming?: boolean }) {
  return (
    <div
      className="shrink-0 mt-0.5 relative"
      style={{
        width: 30,
        height: 30,
        borderRadius: "50%",
        background:
          "radial-gradient(circle at 30% 25%, rgba(255,255,255,0.55), transparent 45%)," +
          "radial-gradient(circle at 65% 70%, rgba(210,155,100,0.85), transparent 55%)," +
          "radial-gradient(circle at 40% 55%, rgba(189,158,255,0.95), transparent 60%)",
        boxShadow: streaming
          ? "0 0 12px rgba(189,158,255,0.55), inset 0 0 4px rgba(0,0,0,0.35)"
          : "0 0 8px rgba(189,158,255,0.30), inset 0 0 4px rgba(0,0,0,0.35)",
        animation: streaming ? "travis-avatar-pulse 2.2s ease-in-out infinite" : undefined,
      }}
    >
      <style>{`
        @keyframes travis-avatar-pulse {
          0%, 100% { transform: scale(1); }
          50%      { transform: scale(1.05); }
        }
        @keyframes travis-cursor-blink {
          0%, 45%   { opacity: 1; }
          55%, 100% { opacity: 0.2; }
        }
      `}</style>
    </div>
  );
}

function StreamingCursor() {
  return (
    <span
      style={{
        display: "inline-block",
        width: 8,
        height: 15,
        background: "rgb(189, 158, 255)",
        verticalAlign: -2,
        marginLeft: 2,
        borderRadius: 1,
        boxShadow: "0 0 6px rgba(189, 158, 255, 0.65)",
        animation: "travis-cursor-blink 1.4s cubic-bezier(0.22, 1, 0.36, 1) infinite",
      }}
    />
  );
}

/**
 * Hover-reveal action strip on completed assistant messages.
 * Matches lobe-chat's `[role='menubar']` fade-in pattern.
 */
function MessageActions({ content }: { content: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className="flex gap-1" role="menubar">
      <ActionButton
        label={copied ? "Copied" : "Copy"}
        onClick={() => {
          void navigator.clipboard.writeText(content).then(() => {
            setCopied(true);
            setTimeout(() => setCopied(false), 1500);
          });
        }}
      >
        <svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" strokeWidth="1.75">
          <rect x="9" y="9" width="13" height="13" rx="2" />
          <path d="M5 15V5a2 2 0 012-2h10" />
        </svg>
      </ActionButton>
      <ActionButton
        label="Regenerate"
        onClick={() => {
          window.dispatchEvent(new CustomEvent("travis:regenerate-last"));
        }}
      >
        <svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" strokeWidth="1.75">
          <path d="M21 12a9 9 0 11-3.5-7.1M21 4v6h-6" />
        </svg>
      </ActionButton>
      <ActionButton
        label="Fork"
        onClick={() => {
          window.dispatchEvent(new CustomEvent("travis:fork-from-message"));
        }}
      >
        <svg viewBox="0 0 24 24" width="11" height="11" fill="none" stroke="currentColor" strokeWidth="1.75">
          <path d="M12 20h9M16.5 3.5a2.12 2.12 0 013 3L7 19l-4 1 1-4z" />
        </svg>
      </ActionButton>
    </div>
  );
}

function ActionButton({
  label,
  onClick,
  children,
}: {
  label: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className="inline-flex items-center gap-1.5 px-2 py-1 rounded-md transition-colors duration-150"
      style={{
        background: "transparent",
        color: "rgba(236, 236, 241, 0.42)",
        fontFamily: "ui-monospace, monospace",
        fontSize: 10,
        letterSpacing: "0.14em",
        textTransform: "uppercase",
        border: "none",
        cursor: "pointer",
      }}
      onMouseEnter={(e) => {
        (e.currentTarget as HTMLButtonElement).style.color = "rgb(189, 158, 255)";
        (e.currentTarget as HTMLButtonElement).style.background = "rgba(189, 158, 255, 0.06)";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLButtonElement).style.color = "rgba(236, 236, 241, 0.42)";
        (e.currentTarget as HTMLButtonElement).style.background = "transparent";
      }}
    >
      {children}
      {label}
    </button>
  );
}

function formatTime(iso: string): string {
  const d = new Date(iso);
  if (isNaN(d.getTime())) return "";
  return d
    .toLocaleTimeString([], { hour: "numeric", minute: "2-digit", second: "2-digit" })
    .toUpperCase();
}

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
