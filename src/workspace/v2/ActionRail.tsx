/**
 * ActionRail — v2 Shell 4.
 *
 * Right-edge HUD showing contextual actions on the current focal card.
 * Actions change based on what the focal is (a map card gets "leave
 * now"; a doc ref gets "open in viewer"; every card gets pin +
 * save-as-note).
 *
 * Video-game reference: spell bar / action bar on the right edge.
 * Contextual and always available for the current focus.
 */
import { motion, AnimatePresence } from "framer-motion";
import type { ConversationMessage } from "../../lib/conversation";
import { useCardLifecycle } from "../../stores/cardLifecycle";
import { useAppStore } from "../../stores/app";
import { parseRichResponse } from "../../lib/richResponse";

interface Props {
  focal: ConversationMessage | null;
}

export function ActionRail({ focal }: Props) {
  const setPendingComposerText = useAppStore((s) => s.setPendingComposerText);
  const cardId = focal ? `msg:${focal.id}` : null;
  const isPinned = useCardLifecycle((s) =>
    cardId ? s.isPinned(cardId) : false,
  );
  const pin = useCardLifecycle((s) => s.pin);
  const unpin = useCardLifecycle((s) => s.unpin);

  const kindLabel = focal ? focalKindLabel(focal) : null;

  const actions: Action[] = [];
  if (focal && cardId) {
    actions.push({
      key: "pin",
      label: isPinned ? "pinned" : "pin",
      hue: 260,
      active: isPinned,
      onClick: () => (isPinned ? unpin(cardId) : pin(cardId)),
    });
    actions.push({
      key: "iterate",
      label: "iterate",
      hue: 200,
      onClick: () =>
        setPendingComposerText("Refine the previous response — make it better."),
    });
    actions.push({
      key: "note",
      label: "save as note",
      hue: 130,
      onClick: () =>
        setPendingComposerText(
          `Save this as a note titled "${
            kindLabel ? kindLabel + " · " : ""
          }" + a short summary I can find later.`,
        ),
    });
    actions.push({
      key: "explain",
      label: "explain more",
      hue: 30,
      onClick: () =>
        setPendingComposerText(
          "Give me more detail on that last reply — dig deeper.",
        ),
    });
  }

  if (actions.length === 0) return null;

  return (
    <div className="absolute right-3 top-1/2 -translate-y-1/2 z-20 pointer-events-auto">
      <div
        className="flex flex-col gap-1.5 rounded-2xl px-2 py-3"
        style={{
          background: "rgba(0, 0, 0, 0.25)",
          border: "1px solid rgba(255, 255, 255, 0.08)",
          backdropFilter: "blur(8px)",
        }}
      >
        <div
          className="text-[8px] uppercase tracking-[0.24em] font-mono text-center px-1 py-0.5"
          style={{ color: "rgba(236, 236, 241, 0.35)" }}
        >
          Actions
        </div>
        <AnimatePresence initial={false}>
          {actions.map((a, i) => (
            <motion.button
              key={a.key}
              layout
              initial={{ opacity: 0, x: 8 }}
              animate={{ opacity: 1, x: 0 }}
              exit={{ opacity: 0, x: 8 }}
              transition={{
                duration: 0.3,
                ease: [0.22, 1, 0.36, 1],
                delay: i * 0.03,
              }}
              whileHover={{ scale: 1.06 }}
              whileTap={{ scale: 0.94 }}
              onClick={a.onClick}
              className="text-[10px] font-mono px-2.5 py-1.5 rounded-md whitespace-nowrap"
              style={{
                background: a.active
                  ? `hsla(${a.hue}, 70%, 65%, 0.22)`
                  : `hsla(${a.hue}, 60%, 65%, 0.10)`,
                color: a.active
                  ? `hsl(${a.hue}, 70%, 78%)`
                  : `hsla(${a.hue}, 60%, 78%, 0.85)`,
                border: `1px solid hsla(${a.hue}, 60%, 65%, ${
                  a.active ? 0.55 : 0.32
                })`,
              }}
              title={a.label}
            >
              {a.label}
            </motion.button>
          ))}
        </AnimatePresence>
      </div>
    </div>
  );
}

interface Action {
  key: string;
  label: string;
  hue: number;
  active?: boolean;
  onClick: () => void;
}

function focalKindLabel(msg: ConversationMessage): string {
  const rich = parseRichResponse(msg.content);
  if (rich && rich.parts.length > 0) {
    return rich.parts[0].kind;
  }
  return "reply";
}
