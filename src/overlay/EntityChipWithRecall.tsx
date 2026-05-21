import { useState, useRef, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import { AnimatePresence, motion } from "framer-motion";

/// Recall payload from the `recall_entity` Tauri command —
/// "what Travis remembers about this entity" on hover (BRAIN.md
/// Phase 4.5 #9). Mirrors the Rust RecallSummary struct.
type Recall = {
  entityId: number;
  displayName: string;
  kind: string;
  mentionsCount: number;
  lastSeen: string;
  confidence: string;
  claims: {
    predicate: string;
    value: string;
    confidence: string;
    source: string;
    contested: boolean;
  }[];
  recentSnippets: { occurredAt: string; snippet: string }[];
  related: {
    entityId: number;
    displayName: string;
    kind: string;
    coMentionCount: number;
  }[];
};

const recallCache = new Map<number, Recall>();

/// Capture chip that fetches a recall summary on hover and renders it
/// in a small popover. Lazy: doesn't network until the user actually
/// hovers (with a 350ms delay). Results cached for the session — the
/// chip's data is read-mostly within a turn.
export function EntityChipWithRecall({
  entityId,
  displayName,
  kind,
  mentionsCount,
}: {
  entityId: number;
  displayName: string;
  kind: string;
  mentionsCount: number;
}) {
  const [open, setOpen] = useState(false);
  const [recall, setRecall] = useState<Recall | null>(
    recallCache.get(entityId) ?? null,
  );
  const [loading, setLoading] = useState(false);
  const hoverTimer = useRef<number | null>(null);

  const onEnter = () => {
    if (hoverTimer.current) window.clearTimeout(hoverTimer.current);
    hoverTimer.current = window.setTimeout(() => {
      setOpen(true);
      if (!recall && !loading) {
        setLoading(true);
        invoke<Recall>("recall_entity", { entityId })
          .then((r) => {
            recallCache.set(entityId, r);
            setRecall(r);
          })
          .catch(() => {})
          .finally(() => setLoading(false));
      }
    }, 350);
  };

  const onLeave = () => {
    if (hoverTimer.current) window.clearTimeout(hoverTimer.current);
    setOpen(false);
  };

  useEffect(
    () => () => {
      if (hoverTimer.current) window.clearTimeout(hoverTimer.current);
    },
    [],
  );

  const lastSeen = recall?.lastSeen
    ? recall.lastSeen.split(/[T ]/)[0]
    : undefined;

  return (
    <span
      className="relative inline-flex items-center gap-1 rounded-full border border-pulse/20 bg-pulse/[0.04] px-2 py-0.5 text-[10px] text-bone-3 cursor-default"
      onMouseEnter={onEnter}
      onMouseLeave={onLeave}
    >
      <span className="text-pulse-2/80" aria-hidden>→</span>
      <span className="text-bone-2">{displayName}</span>
      <span className="text-bone-3/70">({kind.split(":")[0]})</span>
      <AnimatePresence>
        {open && (
          <motion.span
            initial={{ opacity: 0, y: 4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: 4 }}
            transition={{ duration: 0.15 }}
            className="absolute z-30 top-full left-0 mt-1.5 w-72 rounded-lg border border-pulse/30 bg-ink/95 backdrop-blur-sm shadow-xl p-3 text-[11px] text-bone-2"
            data-no-drag
          >
            <div className="flex items-baseline justify-between gap-2 mb-1">
              <span className="text-bone font-medium text-xs">
                {displayName}
              </span>
              <span className="text-bone-3 text-[10px]">
                {kind} · {mentionsCount} mention{mentionsCount === 1 ? "" : "s"}
              </span>
            </div>
            {recall && (
              <div className="text-bone-3 text-[10px] mb-2">
                confidence: <span className="text-pulse-2">{recall.confidence}</span>
                {lastSeen && <> · last seen {lastSeen}</>}
              </div>
            )}
            {loading && !recall && (
              <div className="text-bone-3 text-[10px]">Loading…</div>
            )}
            {recall && recall.claims.length > 0 && (
              <div className="mb-2 flex flex-col gap-1">
                {recall.claims.map((c, i) => (
                  <div key={i} className="leading-snug">
                    <span className="text-bone-3 mr-1">{c.predicate}:</span>
                    <span className="text-bone-2">{c.value}</span>
                    {c.contested && (
                      <span className="ml-1 text-warn text-[9px]">[contested]</span>
                    )}
                  </div>
                ))}
              </div>
            )}
            {recall && recall.recentSnippets.length > 0 && (
              <div className="mb-2">
                <div className="text-bone-3 text-[9px] uppercase tracking-wider mb-0.5">
                  Recent
                </div>
                {recall.recentSnippets.slice(0, 2).map((s, i) => (
                  <div key={i} className="text-bone-3 text-[10px] italic mb-0.5">
                    "{s.snippet}"
                  </div>
                ))}
              </div>
            )}
            {recall && recall.related.length > 0 && (
              <div className="text-bone-3 text-[10px]">
                Often with:{" "}
                {recall.related
                  .slice(0, 3)
                  .map((r) => `${r.displayName} ×${r.coMentionCount}`)
                  .join(", ")}
              </div>
            )}
          </motion.span>
        )}
      </AnimatePresence>
    </span>
  );
}
