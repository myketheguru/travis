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
import { useEffect, useMemo, useRef } from "react";
import { motion } from "framer-motion";
import { useAppStore } from "../../../stores/app";
import { useFocalContent } from "../useFocalContent";
import { parseRichResponse } from "../../../lib/richResponse";
import { RichResponseRenderer } from "../../../chat/cards/RichResponseRenderer";
import { MarkdownBody } from "../../../chat/MarkdownBody";
import { VoiceMessageCard } from "./VoiceMessageCard";
import type { ConversationMessage } from "../../../lib/conversation";

interface RenderMessage {
  id: string;
  role: "user" | "assistant" | "system";
  content: string;
  pending?: boolean;
  optimistic?: boolean;
}

export function ChatCanvas() {
  const activity = useAppStore((s) => s.activity);
  const voiceTranscribing = useAppStore((s) => s.voiceTranscribing);
  const pendingComposerSubmit = useAppStore((s) => s.pendingComposerSubmit);
  const { allMessages } = useFocalContent();
  const scrollRef = useRef<HTMLDivElement | null>(null);
  const lastPendingRef = useRef<string | null>(null);

  const optimistic = useOptimisticSubmit(
    pendingComposerSubmit,
    allMessages,
    lastPendingRef,
  );

  const rendered: RenderMessage[] = useMemo(() => {
    const base: RenderMessage[] = allMessages
      .filter((m) => m.role === "user" || m.role === "assistant" || m.role === "system")
      .map((m) => ({
        id: String(m.id),
        role: m.role as RenderMessage["role"],
        content: m.content,
      }));
    if (optimistic) base.push(optimistic);
    // v0.28.17 — voice-transcribing user bubble. Fires between the
    // user tapping the mic to end and whisper returning the transcript.
    // Skip if the optimistic composer submit already replaced it.
    if (voiceTranscribing && !optimistic) {
      base.push({
        id: "__voice_transcribing__",
        role: "user",
        content: "…",
        optimistic: true,
      });
    }
    if (activity === "thinking") {
      base.push({
        id: "__pending_assistant__",
        role: "assistant",
        content: "",
        pending: true,
      });
    }
    return base;
  }, [allMessages, optimistic, activity, voiceTranscribing]);

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
            {message.pending && (
              <span className="ml-2 opacity-70">· thinking…</span>
            )}
          </div>
          {message.pending ? (
            <div className="flex gap-1.5 mt-1" aria-label="Travis is thinking">
              {[0, 1, 2].map((i) => (
                <motion.span
                  key={i}
                  className="w-1.5 h-1.5 rounded-full"
                  style={{ background: "rgba(189, 158, 255, 0.7)" }}
                  animate={{ opacity: [0.3, 1, 0.3] }}
                  transition={{
                    duration: 1.1,
                    repeat: Infinity,
                    delay: i * 0.15,
                    ease: [0.42, 0, 0.58, 1],
                  }}
                />
              ))}
            </div>
          ) : (
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
