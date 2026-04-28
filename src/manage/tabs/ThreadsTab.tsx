import { useCallback, useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  getThread,
  listConversations,
  resolveConversation,
  type Conversation,
  type Thread,
} from "../../lib/conversation";

const filters: { id: "all" | "awaiting_user" | "open" | "resolved"; label: string }[] = [
  { id: "awaiting_user", label: "Awaiting you" },
  { id: "open", label: "Open" },
  { id: "resolved", label: "Resolved" },
  { id: "all", label: "All" },
];

export default function ThreadsTab() {
  const [filter, setFilter] = useState<(typeof filters)[number]["id"]>("awaiting_user");
  const [items, setItems] = useState<Conversation[]>([]);
  const [loading, setLoading] = useState(true);
  const [openThread, setOpenThread] = useState<Thread | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const list = await listConversations(
        filter === "all" ? undefined : { status: filter },
        100,
      );
      setItems(list);
    } finally {
      setLoading(false);
    }
  }, [filter]);

  useEffect(() => {
    load();
  }, [load]);

  const open = async (id: number) => {
    const t = await getThread(id);
    setOpenThread(t);
  };

  const resolve = async (id: number) => {
    await resolveConversation(id);
    if (openThread?.conversation.id === id) {
      setOpenThread(null);
    }
    load();
  };

  return (
    <div className="px-10 py-6 max-w-3xl mx-auto">
      <p className="text-bone-3 text-xs mb-4 leading-relaxed">
        Every journal note opens a thread. Travis stays on threads where it's
        waiting for you — answer in the overlay or here, or mark as resolved.
      </p>

      <div className="flex items-center gap-2 mb-4">
        {filters.map((f) => (
          <button
            key={f.id}
            onClick={() => setFilter(f.id)}
            className={
              "px-3 py-1.5 rounded-full text-[11px] tracking-wider transition-colors " +
              (filter === f.id
                ? "bg-pulse/20 text-bone border border-pulse/30"
                : "text-bone-3 hover:text-bone-2 border border-ink-3 hover:border-ink-3/80")
            }
          >
            {f.label}
          </button>
        ))}
      </div>

      {loading ? (
        <p className="text-bone-3 text-xs">Loading…</p>
      ) : items.length === 0 ? (
        <p className="text-bone-3 text-xs">No threads here.</p>
      ) : (
        <div className="flex flex-col gap-2">
          <AnimatePresence initial={false} mode="popLayout">
            {items.map((c) => (
              <motion.button
                key={c.id}
                layout
                initial={{ opacity: 0, x: -8 }}
                animate={{ opacity: 1, x: 0 }}
                exit={{ opacity: 0, x: 8 }}
                transition={{ duration: 0.2 }}
                onClick={() => open(c.id)}
                className="text-left flex items-start gap-3 px-3 py-3 rounded-lg border border-white/[0.04] bg-ink-2/30 hover:bg-ink-2/50 transition-colors"
              >
                <span
                  className={
                    "h-2 w-2 rounded-full mt-1.5 flex-shrink-0 " +
                    (c.status === "awaiting_user"
                      ? "bg-pulse-2 shadow-[0_0_8px_rgba(110,196,232,0.7)]"
                      : c.status === "resolved"
                      ? "bg-bone-3/40"
                      : "bg-pulse/60")
                  }
                />
                <div className="flex-1 min-w-0">
                  <div className="text-bone-2 text-sm truncate">
                    {c.title ?? `Thread #${c.id}`}
                  </div>
                  <div className="flex items-center gap-3 mt-1 text-[10px] font-mono text-bone-3">
                    <span className="text-pulse-2/70">{c.status}</span>
                    <span>· #{c.id}</span>
                    <span className="ml-auto opacity-60">
                      {c.updatedAt.slice(0, 16).replace("T", " ")}
                    </span>
                  </div>
                </div>
              </motion.button>
            ))}
          </AnimatePresence>
        </div>
      )}

      <AnimatePresence>
        {openThread && (
          <motion.div
            key="thread-modal"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.25 }}
            className="fixed inset-0 z-30 bg-ink/85 flex items-start justify-center pt-12 px-6 backdrop-blur-md"
            onClick={() => setOpenThread(null)}
          >
            <motion.div
              initial={{ opacity: 0, y: 12 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: 12 }}
              transition={{ duration: 0.3, ease: [0.16, 1, 0.3, 1] }}
              onClick={(e) => e.stopPropagation()}
              className="w-full max-w-2xl max-h-[80vh] flex flex-col rounded-2xl bg-ink-2/95 border border-white/[0.07] overflow-hidden"
            >
              <div className="px-6 py-4 border-b border-white/[0.05] flex items-center justify-between">
                <div>
                  <div className="text-bone text-sm font-medium">
                    {openThread.conversation.title ?? `Thread #${openThread.conversation.id}`}
                  </div>
                  <div className="text-bone-3 text-[10px] font-mono mt-0.5">
                    {openThread.conversation.status} · #{openThread.conversation.id}
                  </div>
                </div>
                <div className="flex items-center gap-3">
                  {openThread.conversation.status !== "resolved" && (
                    <button
                      onClick={() => resolve(openThread.conversation.id)}
                      className="text-pulse-2 hover:text-bone text-xs"
                    >
                      mark resolved
                    </button>
                  )}
                  <button
                    onClick={() => setOpenThread(null)}
                    className="text-bone-3 hover:text-bone-2 text-xs"
                  >
                    close
                  </button>
                </div>
              </div>
              <div className="flex-1 overflow-y-auto px-6 py-4 flex flex-col gap-3">
                {openThread.messages.map((m) => (
                  <div
                    key={m.id}
                    className={
                      "max-w-[85%] rounded-xl px-3 py-2 " +
                      (m.role === "user"
                        ? "self-end bg-pulse/15 border border-pulse/20 text-bone"
                        : m.role === "assistant"
                        ? "self-start bg-ink-3/40 border border-white/[0.04] text-bone-2"
                        : "self-center bg-ink/40 text-bone-3 text-xs italic")
                    }
                  >
                    <div className="text-[10px] tracking-[0.18em] uppercase opacity-60 mb-1">
                      {m.role}
                    </div>
                    <p className="text-sm whitespace-pre-wrap leading-relaxed">{m.content}</p>
                  </div>
                ))}
              </div>
            </motion.div>
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
