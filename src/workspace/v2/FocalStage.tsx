/**
 * FocalStage — center-stage focal card in the v2 canvas.
 *
 * The most recent assistant response rendered prominently in the
 * middle of the canvas. Uses the same rich-response renderer the
 * classic surface uses, so cards (Map, DocRef, Thread, T2tConvo,
 * etc.) all show up correctly here.
 *
 * Framer-motion layout enables smooth transitions when the focal
 * changes (Travis emits a new response, or user promotes an orbit
 * card to focal).
 */
import { motion, AnimatePresence } from "framer-motion";
import type { ConversationMessage } from "../../lib/conversation";
import { parseRichResponse } from "../../lib/richResponse";
import { RichResponseRenderer } from "../../chat/cards/RichResponseRenderer";
import { MarkdownBody } from "../../chat/MarkdownBody";

interface Props {
  message: ConversationMessage | null;
  /** True when Travis is currently generating a response — pulses
   *  the border to signal work in flight. */
  pending?: boolean;
}

export function FocalStage({ message, pending }: Props) {
  return (
    <div className="relative w-full max-w-3xl mx-auto flex items-center justify-center min-h-[280px]">
      <AnimatePresence mode="popLayout">
        {message ? (
          <motion.div
            key={String(message.id)}
            layout
            initial={{ opacity: 0, y: 12, scale: 0.98 }}
            animate={{
              opacity: 1,
              y: 0,
              scale: 1,
              boxShadow: pending
                ? "0 12px 60px -18px rgba(124, 92, 255, 0.55)"
                : "0 12px 48px -20px rgba(0, 0, 0, 0.65)",
            }}
            exit={{ opacity: 0, y: -12, scale: 0.98 }}
            transition={{
              duration: 0.42,
              ease: [0.22, 1, 0.36, 1],
              boxShadow: { duration: 1.4, ease: "easeInOut" },
            }}
            className="w-full rounded-2xl px-5 py-4"
            style={{
              background:
                "linear-gradient(180deg, rgba(255,255,255,0.04), rgba(255,255,255,0.015))",
              border: `1px solid ${
                pending
                  ? "rgba(189, 158, 255, 0.55)"
                  : "rgba(255, 255, 255, 0.10)"
              }`,
              backdropFilter: "blur(6px)",
            }}
          >
            <div
              className="text-[10px] tracking-[0.22em] uppercase font-mono mb-2"
              style={{ color: "rgba(236, 236, 241, 0.5)" }}
            >
              // travis · focal
            </div>
            <MessageBody message={message} />
          </motion.div>
        ) : (
          <motion.div
            key="focal-empty"
            initial={{ opacity: 0 }}
            animate={{ opacity: 0.55 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.32, ease: [0.22, 1, 0.36, 1] }}
            className="text-center"
          >
            <div
              className="text-[11px] font-mono uppercase tracking-[0.22em]"
              style={{ color: "rgba(236, 236, 241, 0.4)" }}
            >
              nothing focal yet
            </div>
            <div
              className="text-[13px] mt-2"
              style={{ color: "rgba(236, 236, 241, 0.55)" }}
            >
              Ask Travis something. Their reply lands here.
            </div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

function MessageBody({ message }: { message: ConversationMessage }) {
  const rich = parseRichResponse(message.content);
  if (rich) {
    return <RichResponseRenderer response={rich} />;
  }
  return <MarkdownBody text={message.content} />;
}
