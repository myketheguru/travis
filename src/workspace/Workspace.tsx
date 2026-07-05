/**
 * Workspace — Shell 8.
 *
 * Wraps the primary chat surface (AskTab) with the workspace shell
 * chrome: attention strip on top, Clear button for on-demand fresh
 * state, and (in future slices) canvas rendering of resurrected cards
 * with restore badges.
 *
 * This is the surface the user "jumps into" — the entry point that
 * replaces Manage's ask tab as the workspace-first landing. It does
 * NOT remove the other Manage tabs yet (they're still accessible via
 * the sidebar); this shell composes over the ask tab first so we can
 * ship the workspace experience without a destructive migration.
 */
import { useCallback } from "react";
import { motion, AnimatePresence } from "framer-motion";
import AskTab from "../manage/tabs/AskTab";
import { AttentionStrip } from "./AttentionStrip";
import { SuggestionRail } from "./SuggestionRail";
import type { AttentionItem } from "./useAttentionItems";
import { useCardLifecycle } from "../stores/cardLifecycle";
import { useAppStore } from "../stores/app";
import type { Suggestion } from "./useSuggestions";

export function Workspace() {
  const clearAll = useCardLifecycle((s) => s.clearAll);
  const clearedAt = useCardLifecycle((s) => s.clearedAt);
  const resurrectedIds = useCardLifecycle((s) => s.resurrectedIds);
  const pinnedIds = useCardLifecycle((s) => s.pinnedIds);
  const setPendingComposerText = useAppStore((s) => s.setPendingComposerText);

  const handleClear = useCallback(() => {
    // No confirm — Clear is meant to feel instant. User can undo by
    // asking Travis to bring things back (Shell 7 resurrection).
    clearAll();
  }, [clearAll]);

  const handleSuggestion = useCallback(
    (s: Suggestion) => {
      // Push the suggestion's prompt into the composer via the app
      // store bridge; AskTab picks it up + focuses the input.
      setPendingComposerText(s.prompt);
    },
    [setPendingComposerText],
  );

  // v0.24 (task 311) — click an attention chip -> compose a prompt
  // that asks Travis to surface that item as a card. LLM fetches
  // details + emits the appropriate rich response part.
  const handleAttentionItem = useCallback(
    (item: AttentionItem) => {
      let prompt: string;
      switch (item.kind) {
        case "t2t_pending":
          prompt = `Open the incoming Travis-to-Travis query "${item.label}" and show me the card.`;
          break;
        case "t2t_drafted":
          prompt = `Open the T2T draft "${item.label}" so I can review + approve.`;
          break;
        case "workflow_awaiting_approval":
          prompt = `Show me what needs approval — "${item.label}".`;
          break;
        case "workflow_running":
          prompt = `Give me a status on "${item.label}".`;
          break;
        default:
          prompt = `Show me "${item.label}".`;
      }
      setPendingComposerText(prompt);
    },
    [setPendingComposerText],
  );

  const hasActiveState =
    (clearedAt && ageInMs(clearedAt) < 60_000) ||
    resurrectedIds.length > 0 ||
    pinnedIds.length > 0;

  return (
    <div className="flex flex-col h-full min-h-0">
      {/* Shell chrome: attention strip + workspace controls. */}
      <div
        className="shrink-0 border-b"
        style={{
          borderColor: "rgba(255, 255, 255, 0.04)",
          background:
            "linear-gradient(180deg, rgba(255,255,255,0.015), transparent)",
        }}
      >
        <div className="flex items-center gap-2 pr-3">
          <div className="flex-1 min-w-0">
            <AttentionStrip onItemClick={handleAttentionItem} />
          </div>

          <AnimatePresence>
            {hasActiveState && (
              <motion.div
                key="workspace-controls"
                initial={{ opacity: 0, x: 8 }}
                animate={{ opacity: 1, x: 0 }}
                exit={{ opacity: 0, x: 8 }}
                transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
                className="shrink-0 flex items-center gap-1.5"
              >
                {pinnedIds.length > 0 && (
                  <span
                    className="text-[10px] uppercase tracking-wider font-mono px-2 py-1 rounded-full"
                    style={{
                      background: "rgba(189, 158, 255, 0.10)",
                      color: "rgb(189, 158, 255)",
                      border: "1px solid rgba(189, 158, 255, 0.35)",
                    }}
                  >
                    {pinnedIds.length} pinned
                  </span>
                )}
                {resurrectedIds.length > 0 && (
                  <span
                    className="text-[10px] uppercase tracking-wider font-mono px-2 py-1 rounded-full"
                    style={{
                      background: "rgba(255, 179, 92, 0.10)",
                      color: "rgb(255, 179, 92)",
                      border: "1px solid rgba(255, 179, 92, 0.35)",
                    }}
                  >
                    {resurrectedIds.length} restored
                  </span>
                )}
                <button
                  onClick={handleClear}
                  title="Clear workspace — everything unpinned archives immediately. Reversible: ask Travis to bring anything back."
                  className="text-[10px] uppercase tracking-wider font-mono px-2.5 py-1 rounded-full transition-all"
                  style={{
                    background: "rgba(255, 255, 255, 0.04)",
                    color: "rgba(236, 236, 241, 0.7)",
                    border: "1px solid rgba(255, 255, 255, 0.1)",
                  }}
                  onMouseEnter={(e) => {
                    e.currentTarget.style.background =
                      "rgba(255, 100, 100, 0.10)";
                    e.currentTarget.style.color = "rgba(255, 180, 180, 0.95)";
                    e.currentTarget.style.borderColor =
                      "rgba(255, 100, 100, 0.35)";
                  }}
                  onMouseLeave={(e) => {
                    e.currentTarget.style.background =
                      "rgba(255, 255, 255, 0.04)";
                    e.currentTarget.style.color = "rgba(236, 236, 241, 0.7)";
                    e.currentTarget.style.borderColor =
                      "rgba(255, 255, 255, 0.1)";
                  }}
                >
                  clear
                </button>
              </motion.div>
            )}

            {!hasActiveState && (
              <motion.button
                key="workspace-clear-inactive"
                initial={{ opacity: 0 }}
                animate={{ opacity: 0.4 }}
                exit={{ opacity: 0 }}
                whileHover={{ opacity: 0.85 }}
                transition={{ duration: 0.22, ease: [0.22, 1, 0.36, 1] }}
                onClick={handleClear}
                title="Clear workspace"
                className="shrink-0 text-[10px] uppercase tracking-wider font-mono px-2.5 py-1 rounded-full transition-all"
                style={{
                  background: "rgba(255, 255, 255, 0.03)",
                  color: "rgba(236, 236, 241, 0.7)",
                  border: "1px solid rgba(255, 255, 255, 0.08)",
                }}
              >
                clear
              </motion.button>
            )}
          </AnimatePresence>
        </div>
      </div>

      {/* Ambient suggestion rail (Shell 9) — proposes 3-5 next moves
          based on time of day + patterns. Click a chip -> AskTab
          composer fills with the prompt. */}
      <SuggestionRail onSuggestionClick={handleSuggestion} />

      {/* Primary canvas: for now the existing chat surface. Shell 8+
          will progressively morph this into a card-canvas layout as
          card kinds get their own components. */}
      <div className="flex-1 min-h-0">
        <AskTab />
      </div>
    </div>
  );
}

function ageInMs(iso: string): number {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return Infinity;
  return Date.now() - t;
}
