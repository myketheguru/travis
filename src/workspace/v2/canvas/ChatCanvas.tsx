/**
 * ChatCanvas — v2 Shell 14.
 *
 * The conversation IS the canvas. Vertical column of assistant + user
 * messages. Latest is center-stage, largest, 100% opacity. Older ones
 * scale + fade as they age. Scrolling shifts focus — the message
 * closest to the vertical center becomes the "focused" one at full
 * opacity, adjacent messages soften.
 *
 * This replaces FocalStage + OrbitalStack + AskTab-chat-below in the
 * v2 canvas. AskTab stays mounted invisibly to keep the submit
 * pipeline working; ChatCanvas reads its messages via useFocalContent.
 */
import { useEffect, useRef } from "react";
import { motion } from "framer-motion";
import { useAppStore } from "../../../stores/app";
import { useFocalContent } from "../useFocalContent";
import { parseRichResponse } from "../../../lib/richResponse";
import { RichResponseRenderer } from "../../../chat/cards/RichResponseRenderer";
import { MarkdownBody } from "../../../chat/MarkdownBody";
import type { ConversationMessage } from "../../../lib/conversation";

export function ChatCanvas() {
  const activity = useAppStore((s) => s.activity);
  const { focal, orbits } = useFocalContent();
  const scrollRef = useRef<HTMLDivElement | null>(null);

  // Autoscroll the focal message into view whenever it changes.
  useEffect(() => {
    if (!focal || !scrollRef.current) return;
    const el = scrollRef.current.querySelector<HTMLElement>(
      `[data-msg-id="${focal.id}"]`,
    );
    el?.scrollIntoView({ behavior: "smooth", block: "center" });
  }, [focal?.id]);

  // Combined ordered list — user might see: [oldest orbit, ..., newest orbit, focal]
  // orbits comes back newest-first (see useFocalContent); reverse for chronological.
  const chronological = [...orbits].reverse();

  // Empty state
  if (!focal && chronological.length === 0) {
    return <EmptyChatCanvas />;
  }

  return (
    <div
      ref={scrollRef}
      className="absolute inset-0 overflow-y-auto scroll-smooth"
      style={{
        // Reserve room at bottom for the pinned composer (140px).
        paddingBottom: "160px",
        paddingTop: "20vh",
      }}
    >
      <div className="max-w-3xl mx-auto px-6 flex flex-col gap-8">
        {chronological.map((m, i) => (
          <MessageBlock
            key={m.id}
            message={m}
            focusLevel={levelFor(i, chronological.length)}
          />
        ))}
        {focal && (
          <MessageBlock
            key={focal.id}
            message={focal}
            focusLevel={1}
            pending={activity === "thinking"}
          />
        )}
        {/* Trailing spacer so the focal can center visually. */}
        <div style={{ height: "30vh" }} />
      </div>
    </div>
  );
}

/**
 * levelFor — map the message's position in the orbits stack to an
 * opacity + scale factor. Newest orbit gets 0.7; oldest ~0.2.
 */
function levelFor(index: number, total: number): number {
  if (total === 0) return 0.5;
  // Newer messages (higher index) get more focus.
  const normalized = (index + 1) / total; // 1/n … 1
  return 0.2 + normalized * 0.5;
}

function MessageBlock({
  message,
  focusLevel,
  pending,
}: {
  message: ConversationMessage;
  focusLevel: number;
  pending?: boolean;
}) {
  const isUser = message.role === "user";
  const rich = !isUser ? parseRichResponse(message.content) : null;

  return (
    <motion.div
      data-msg-id={message.id}
      layout
      animate={{
        opacity: focusLevel,
        scale: 0.9 + focusLevel * 0.1,
      }}
      transition={{ duration: 0.4, ease: [0.22, 1, 0.36, 1] }}
      className="w-full"
    >
      {isUser ? (
        <div className="flex justify-end">
          <div
            className="rounded-2xl px-4 py-2.5 max-w-[80%]"
            style={{
              background: "rgba(124, 92, 255, 0.10)",
              border: "1px solid rgba(124, 92, 255, 0.30)",
              color: "rgba(236, 236, 241, 0.95)",
              fontSize: 15 + focusLevel * 3, // 15px … 18px
              lineHeight: 1.5,
            }}
          >
            {message.content}
          </div>
        </div>
      ) : (
        <div>
          <div
            className="text-[10px] uppercase tracking-[0.22em] font-mono mb-2"
            style={{ color: `rgba(236, 236, 241, ${0.35 * focusLevel})` }}
          >
            Travis
            {pending && <span className="ml-2 opacity-70">· thinking…</span>}
          </div>
          <div
            style={{
              fontSize: 15 + focusLevel * 3,
              lineHeight: 1.55,
              color: `rgba(236, 236, 241, ${0.75 + focusLevel * 0.2})`,
            }}
          >
            {rich ? (
              <RichResponseRenderer response={rich} />
            ) : (
              <MarkdownBody text={message.content} />
            )}
          </div>
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
