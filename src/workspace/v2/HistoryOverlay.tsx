/**
 * HistoryOverlay — v2 Shell 10.
 *
 * ⌘K opens a floating card listing recent conversations. Click one to
 * switch active conversation and close the overlay. Search filter
 * narrows by title / preview.
 *
 * The immersive workspace has no sidebar, but the user still needs to
 * reach past work — this is that lever.
 */
import { useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useAppStore } from "../../stores/app";
import {
  deleteConversation,
  listConversationsForSwitcher,
  type ConversationListItem,
} from "../../lib/conversation";

export function HistoryOverlay() {
  const open = useAppStore((s) => s.historyOverlayOpen);
  const setOpen = useAppStore((s) => s.setHistoryOverlayOpen);
  const setActiveConversationId = useAppStore(
    (s) => s.setActiveConversationId,
  );
  const noteUserActivity = useAppStore((s) => s.noteUserActivity);

  const [query, setQuery] = useState("");
  const [items, setItems] = useState<ConversationListItem[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!open) return;
    let cancelled = false;
    setLoading(true);
    setError(null);
    (async () => {
      try {
        const list = await listConversationsForSwitcher(
          query.trim() || undefined,
          40,
        );
        if (!cancelled) setItems(list);
      } catch (e) {
        if (!cancelled) setError(String(e));
      } finally {
        if (!cancelled) setLoading(false);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [open, query]);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, setOpen]);

  function handleSelect(item: ConversationListItem) {
    setActiveConversationId(item.id);
    noteUserActivity();
    setOpen(false);
    setQuery("");
  }

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          key="history-backdrop"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.24, ease: [0.22, 1, 0.36, 1] }}
          className="fixed inset-0 z-40 flex items-start justify-center pt-24"
          style={{
            background: "rgba(0, 0, 0, 0.55)",
            backdropFilter: "blur(4px)",
          }}
          onClick={() => setOpen(false)}
        >
          <motion.div
            key="history-card"
            initial={{ opacity: 0, scale: 0.98, y: -8 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.98, y: -8 }}
            transition={{ duration: 0.28, ease: [0.22, 1, 0.36, 1] }}
            className="relative rounded-2xl overflow-hidden shadow-2xl flex flex-col"
            style={{
              width: "min(680px, 92vw)",
              maxHeight: "min(560px, 70vh)",
              background: "rgb(12, 12, 16)",
              border: "1px solid rgba(255, 255, 255, 0.10)",
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <div
              className="px-4 py-3 border-b flex items-center gap-2"
              style={{ borderColor: "rgba(255, 255, 255, 0.06)" }}
            >
              <span
                className="text-[10px] uppercase tracking-[0.24em] font-mono shrink-0"
                style={{ color: "rgba(236, 236, 241, 0.4)" }}
              >
                History
              </span>
              <input
                autoFocus
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                placeholder="search past work…"
                className="flex-1 bg-transparent text-[13px] focus:outline-none placeholder:text-white/30"
                style={{ color: "rgba(236, 236, 241, 0.95)" }}
              />
              <span
                className="text-[9px] uppercase tracking-wider font-mono opacity-50"
                style={{ color: "rgba(236, 236, 241, 0.4)" }}
              >
                esc to close
              </span>
            </div>

            <div className="flex-1 min-h-0 overflow-y-auto">
              {loading && items.length === 0 && (
                <div
                  className="px-4 py-4 text-[11px] font-mono opacity-60"
                  style={{ color: "rgba(236, 236, 241, 0.6)" }}
                >
                  loading…
                </div>
              )}
              {error && (
                <div
                  className="px-4 py-2 text-[11px] font-mono"
                  style={{ color: "rgba(255, 130, 130, 0.85)" }}
                >
                  {error}
                </div>
              )}
              {!loading && items.length === 0 && !error && (
                <div
                  className="px-4 py-6 text-[12px] font-mono opacity-60 text-center"
                  style={{ color: "rgba(236, 236, 241, 0.6)" }}
                >
                  {query ? "nothing matched" : "no history yet"}
                </div>
              )}
              {items.map((item, i) => (
                <motion.div
                  key={item.id}
                  initial={{ opacity: 0, y: 4 }}
                  animate={{ opacity: 1, y: 0 }}
                  transition={{
                    duration: 0.24,
                    ease: [0.22, 1, 0.36, 1],
                    delay: Math.min(i * 0.015, 0.12),
                  }}
                  className="w-full text-left px-4 py-2.5 border-b flex items-center gap-2 group"
                  style={{ borderColor: "rgba(255,255,255,0.04)" }}
                >
                  <button
                    onClick={() => handleSelect(item)}
                    className="min-w-0 flex-1 text-left"
                  >
                    <div
                      className="text-[13px] truncate"
                      style={{ color: "rgba(236, 236, 241, 0.92)" }}
                    >
                      {item.title ?? "Untitled thread"}
                    </div>
                    {item.preview && (
                      <div
                        className="text-[10.5px] font-mono mt-0.5 truncate opacity-60"
                        style={{ color: "rgba(236, 236, 241, 0.7)" }}
                      >
                        {item.preview}
                      </div>
                    )}
                  </button>
                  <div
                    className="shrink-0 text-[10px] font-mono opacity-50"
                    style={{ color: "rgba(236, 236, 241, 0.55)" }}
                  >
                    {relativeAge(item.updatedAt)}
                  </div>
                  <button
                    onClick={async (e) => {
                      e.stopPropagation();
                      if (!confirm("Delete this conversation? This can't be undone.")) return;
                      try {
                        await deleteConversation(item.id);
                        setItems((prev) => prev.filter((c) => c.id !== item.id));
                      } catch (err) {
                        setError(err instanceof Error ? err.message : String(err));
                      }
                    }}
                    className="shrink-0 opacity-0 group-hover:opacity-100 transition-opacity h-7 w-7 rounded-md flex items-center justify-center"
                    style={{
                      background: "rgba(239, 68, 68, 0.10)",
                      border: "1px solid rgba(239, 68, 68, 0.30)",
                      color: "rgb(239, 68, 68)",
                    }}
                    title="Delete conversation"
                    aria-label="Delete conversation"
                  >
                    <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                      <path d="M3 6h18M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6" />
                      <path d="M10 11v6M14 11v6" />
                    </svg>
                  </button>
                </motion.div>
              ))}
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function relativeAge(iso: string): string {
  const t = Date.parse(iso);
  if (!Number.isFinite(t)) return "";
  const ms = Date.now() - t;
  const m = Math.floor(ms / 60000);
  if (m < 1) return "just now";
  if (m < 60) return `${m}m`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}h`;
  const d = Math.floor(h / 24);
  if (d < 7) return `${d}d`;
  const w = Math.floor(d / 7);
  if (w < 5) return `${w}w`;
  const mo = Math.floor(d / 30);
  return `${mo}mo`;
}
