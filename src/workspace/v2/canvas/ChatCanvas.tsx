/**
 * ChatCanvas — v0.28.70 rearchitecture.
 *
 * Reads exclusively from `chatStore.messagesMap[activeConversationId]`.
 * No `useFocalContent` polling. No parallel "streamingAssistant" slot.
 * No optimistic bubble managed in a sibling ref. One message list,
 * one reducer, one render path.
 *
 * The v0.28.66 lobe-chat message anatomy is preserved (bubbleless
 * assistant, avatar, author header, hover actions, streaming cursor,
 * tool call chips, reasoning block, dashed user bubble, inline
 * voice audio card). All fields that used to live in a parallel
 * store (streaming, reasoning, toolCalls, audio, error) now live on
 * the UIMessage itself.
 */
import { useEffect, useRef, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { convertFileSrc } from "@tauri-apps/api/core";
import { useAppStore } from "../../../stores/app";
import { useChatStore, type UIMessage } from "../../../stores/chatStore";
import { parseRichResponse } from "../../../lib/richResponse";
import { RichResponseRenderer } from "../../../chat/cards/RichResponseRenderer";
import { MarkdownBody } from "../../../chat/MarkdownBody";
import { VoiceMessageCard } from "./VoiceMessageCard";
import { FileCard } from "../../../chat/FileCard";

// v0.28.72 — module-level stable empty array. Returning `?? []` inline
// in a Zustand selector creates a new array reference on every render,
// which useSyncExternalStore diffs as "changed" → infinite render loop
// (the "Maximum update depth exceeded" crash). Ref stability is
// required for object-returning selectors.
const EMPTY_MESSAGES: UIMessage[] = [];

export function ChatCanvas() {
  const activity = useAppStore((s) => s.activity);
  const activeConversationId = useAppStore((s) => s.activeConversationId);
  const messages = useChatStore((s) =>
    activeConversationId !== null
      ? s.messagesMap[activeConversationId] ?? EMPTY_MESSAGES
      : EMPTY_MESSAGES,
  );
  const scrollRef = useRef<HTMLDivElement | null>(null);

  // Auto-scroll on new content — respect user's manual scroll-up.
  const lastLenRef = useRef<number>(0);
  const lastContentLenRef = useRef<number>(0);
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    const totalContent = messages.reduce((n, m) => n + m.content.length, 0);
    const grew =
      messages.length > lastLenRef.current ||
      totalContent > lastContentLenRef.current;
    lastLenRef.current = messages.length;
    lastContentLenRef.current = totalContent;
    if (!grew) return;
    const distanceFromBottom = el.scrollHeight - el.scrollTop - el.clientHeight;
    if (distanceFromBottom < 200 || messages.length === 1) {
      el.scrollTo({
        top: el.scrollHeight,
        behavior: messages.length === 1 ? "auto" : "smooth",
      });
    }
  }, [messages]);

  // Also scroll when activity changes (helps voice → thinking).
  useEffect(() => {
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTo({ top: el.scrollHeight, behavior: "smooth" });
  }, [activity]);

  if (messages.length === 0) {
    return <EmptyChatCanvas />;
  }

  return (
    <div
      ref={scrollRef}
      className="absolute inset-0 overflow-y-auto scroll-smooth"
      style={{ paddingBottom: "160px", paddingTop: "18vh" }}
    >
      <div className="max-w-3xl mx-auto px-6 flex flex-col gap-8">
        <AnimatePresence initial={false}>
          {messages
            .filter(
              (m) =>
                m.role === "user" || m.role === "assistant" || m.role === "system",
            )
            .map((m, i, arr) => (
              <MessageBlock
                key={String(m.id)}
                message={m}
                focusLevel={levelFor(i, arr.length)}
              />
            ))}
        </AnimatePresence>
        <div style={{ height: "18vh" }} />
      </div>
    </div>
  );
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
  message: UIMessage;
  focusLevel: number;
}) {
  if (message.role === "user") {
    return <UserRow message={message} focusLevel={focusLevel} />;
  }
  const rich = !message.streaming && message.content
    ? parseRichResponse(message.content)
    : null;
  return (
    <AssistantRow message={message} focusLevel={focusLevel} rich={rich} />
  );
}

function UserRow({
  message,
  focusLevel,
}: {
  message: UIMessage;
  focusLevel: number;
}) {
  const timeLabel = message.createdAt ? formatTime(message.createdAt) : null;
  const isOptimistic = typeof message.id === "string";
  return (
    <motion.div
      data-msg-id={String(message.id)}
      layout
      initial={{ opacity: 0, y: 12 }}
      animate={{
        opacity: focusLevel,
        y: 0,
        scale: 0.96 + focusLevel * 0.04,
      }}
      exit={{ opacity: 0, y: -6 }}
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
            border: isOptimistic
              ? "1px dashed rgba(189, 158, 255, 0.42)"
              : "1px solid rgba(124, 92, 255, 0.30)",
            borderBottomRightRadius: "6px",
            color: "rgba(236, 236, 241, 0.98)",
            fontSize: 14 + focusLevel * 2,
            lineHeight: 1.55,
          }}
        >
          {typeof message.id === "number" && !message.audio ? (
            // Persisted row without eager audio — VoiceMessageCard
            // will fetch by id if this was actually a voice turn.
            <VoiceMessageCard
              messageId={message.id}
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

function AssistantRow({
  message,
  focusLevel,
  rich,
}: {
  message: UIMessage;
  focusLevel: number;
  rich: ReturnType<typeof parseRichResponse>;
}) {
  const timeLabel = message.createdAt ? formatTime(message.createdAt) : null;
  const hasReasoning = !!message.reasoning && message.reasoning.length > 0;
  const hasToolCalls =
    !!message.toolCalls && message.toolCalls.length > 0;
  const streaming = !!message.streaming;
  // v0.28.72 — parse `doc#N` markers out of assistant content and
  // render FileCards below the text. Was standard in the old ChatTurn
  // (line 132-138); the ChatCanvas rewrite lost this handling and
  // users saw "doc#1" as raw text instead of a downloadable card.
  const docMatches = !streaming
    ? Array.from(message.content.matchAll(/doc#(\d+)/g)).map((m) =>
        Number(m[1]),
      )
    : [];
  const docIds = Array.from(new Set(docMatches));
  const displayContent =
    docIds.length > 0
      ? message.content.replace(/doc#\d+/g, "").replace(/[ \t]+\n/g, "\n").trim()
      : message.content;

  return (
    <motion.div
      data-msg-id={String(message.id)}
      layout
      initial={{ opacity: 0, y: 12 }}
      animate={{
        opacity: focusLevel,
        y: 0,
        scale: 0.96 + focusLevel * 0.04,
      }}
      exit={{ opacity: 0, y: -6 }}
      transition={{ duration: 0.34, ease: [0.22, 1, 0.36, 1] }}
      className="w-full group"
    >
      <div className="flex gap-3">
        <TravisAvatar streaming={streaming} />
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

          {hasToolCalls && <ToolCallsInline calls={message.toolCalls!} />}
          {hasReasoning && <ReasoningBlock text={message.reasoning!} />}

          <div
            style={{
              fontSize: 14 + focusLevel * 2,
              lineHeight: 1.6,
              color: `rgba(236, 236, 241, ${0.85 + focusLevel * 0.12})`,
            }}
          >
            {streaming ? (
              message.content.length > 0 ? (
                <div>
                  <MarkdownBody text={message.content} />
                  <StreamingCursor />
                </div>
              ) : (
                <StreamingCursor />
              )
            ) : rich ? (
              <RichResponseRenderer response={rich} messageId={String(message.id)} />
            ) : (
              <MarkdownBody text={displayContent} />
            )}
          </div>

          {/* v0.28.72 — doc#N cards rendered below the text. Clicking
              opens the document viewer overlay (viewerDocumentId store
              field routes through App.tsx to the DocumentViewer). */}
          {docIds.length > 0 && (
            <div className="flex flex-wrap gap-2 mt-2">
              {docIds.map((id) => (
                <DocCardClickable key={id} documentId={id} />
              ))}
            </div>
          )}

          {message.error && (
            <div
              className="rounded-lg px-3 py-2 mt-1"
              style={{
                background: "rgba(248, 113, 113, 0.06)",
                border: "1px solid rgba(248, 113, 113, 0.28)",
                fontSize: 12,
                color: "rgba(248, 113, 113, 0.92)",
                fontFamily: "ui-monospace, monospace",
              }}
            >
              {message.aborted ? "Turn cancelled." : message.error}
            </div>
          )}

          {!streaming && typeof message.id === "number" && (
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
 * v0.28.72 — wraps FileCard with a click handler that pops the
 * document viewer overlay. Uses `viewerDocumentId` on the app store
 * which App.tsx already routes through DocumentViewer.
 */
function DocCardClickable({ documentId }: { documentId: number }) {
  const setViewer = useAppStore((s) => s.setViewerDocumentId);
  return (
    <div
      onClick={() => setViewer(documentId)}
      style={{ cursor: "pointer" }}
      className="hover:opacity-90 transition-opacity duration-150"
    >
      <FileCard documentId={documentId} />
    </div>
  );
}

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
        animation:
          "travis-cursor-blink 1.4s cubic-bezier(0.22, 1, 0.36, 1) infinite",
      }}
    />
  );
}

function ToolCallsInline({
  calls,
}: {
  calls: Array<{ id: string; name: string }>;
}) {
  return (
    <div className="flex flex-wrap gap-1.5 mb-1">
      {calls.map((c) => (
        <div
          key={c.id}
          className="inline-flex items-center gap-2 px-2.5 py-1 rounded-lg"
          style={{
            background: "rgba(189, 158, 255, 0.06)",
            border: "1px solid rgba(189, 158, 255, 0.24)",
          }}
        >
          <span
            style={{
              width: 6,
              height: 6,
              borderRadius: "50%",
              background: "rgb(74, 222, 128)",
              boxShadow: "0 0 5px rgb(74, 222, 128)",
              animation:
                "travis-cursor-blink 1.6s cubic-bezier(0.22, 1, 0.36, 1) infinite",
            }}
          />
          <span
            style={{
              fontFamily: "ui-monospace, monospace",
              fontSize: 10.5,
              letterSpacing: "0.14em",
              textTransform: "uppercase",
              color: "rgba(189, 158, 255, 0.92)",
            }}
          >
            {c.name}
          </span>
        </div>
      ))}
    </div>
  );
}

function ReasoningBlock({ text }: { text: string }) {
  const [expanded, setExpanded] = useState(true);
  return (
    <div
      className="rounded-xl px-3 py-2 mb-1"
      style={{
        background: "rgba(255, 255, 255, 0.02)",
        border: "1px solid rgba(255, 255, 255, 0.05)",
      }}
    >
      <button
        onClick={() => setExpanded((v) => !v)}
        style={{
          fontFamily: "ui-monospace, monospace",
          fontSize: 9.5,
          letterSpacing: "0.22em",
          textTransform: "uppercase",
          color: "rgba(210, 155, 100, 0.75)",
          marginBottom: expanded ? 4 : 0,
          background: "transparent",
          border: "none",
          padding: 0,
          cursor: "pointer",
          display: "flex",
          alignItems: "center",
          gap: 6,
        }}
      >
        <span>thinking</span>
        <span style={{ opacity: 0.5, fontSize: 10 }}>
          {expanded ? "▾" : "▸"}
        </span>
      </button>
      {expanded && (
        <div
          style={{
            fontSize: 13,
            lineHeight: 1.55,
            color: "rgba(236, 236, 241, 0.62)",
            fontStyle: "italic",
          }}
        >
          {text}
        </div>
      )}
    </div>
  );
}

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
        <svg
          viewBox="0 0 24 24"
          width="11"
          height="11"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.75"
        >
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
        <svg
          viewBox="0 0 24 24"
          width="11"
          height="11"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.75"
        >
          <path d="M21 12a9 9 0 11-3.5-7.1M21 4v6h-6" />
        </svg>
      </ActionButton>
      <ActionButton
        label="Fork"
        onClick={() => {
          window.dispatchEvent(new CustomEvent("travis:fork-from-message"));
        }}
      >
        <svg
          viewBox="0 0 24 24"
          width="11"
          height="11"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.75"
        >
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
        (e.currentTarget as HTMLButtonElement).style.color =
          "rgb(189, 158, 255)";
        (e.currentTarget as HTMLButtonElement).style.background =
          "rgba(189, 158, 255, 0.06)";
      }}
      onMouseLeave={(e) => {
        (e.currentTarget as HTMLButtonElement).style.color =
          "rgba(236, 236, 241, 0.42)";
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
    .toLocaleTimeString([], {
      hour: "numeric",
      minute: "2-digit",
      second: "2-digit",
    })
    .toUpperCase();
}

function InlineVoiceBubble({
  audio,
}: {
  audio: { audioPath: string; durationMs: number; transcript: string };
}) {
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
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="currentColor"
              aria-hidden
            >
              <rect x="6" y="5" width="4" height="14" rx="1" />
              <rect x="14" y="5" width="4" height="14" rx="1" />
            </svg>
          ) : (
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="currentColor"
              aria-hidden
            >
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
                width: `${
                  durationSec === 0
                    ? 0
                    : Math.min(100, (pos / durationSec) * 100)
                }%`,
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
        <div
          className="text-[10px] font-mono uppercase tracking-[0.20em] mt-6"
          style={{ color: "rgba(236, 236, 241, 0.3)" }}
        >
          ⌘/Ctrl · N — new conversation
        </div>
      </motion.div>
    </div>
  );
}
