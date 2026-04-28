import { useCallback, useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { invoke } from "@tauri-apps/api/core";

type Reminder = {
  id: number;
  text: string;
  kind: string;
  remindAt: string | null;
  firedAt: string | null;
  dismissedAt: string | null;
  source: string;
  linkKind: string | null;
  linkId: number | null;
  createdAt: string;
  updatedAt: string;
};

type FilterId = "active" | "fired" | "dismissed" | "all";

const filters: { id: FilterId; label: string }[] = [
  { id: "active", label: "Active" },
  { id: "fired", label: "Fired" },
  { id: "dismissed", label: "Dismissed" },
  { id: "all", label: "All" },
];

export default function RemindersTab() {
  const [filter, setFilter] = useState<FilterId>("active");
  const [items, setItems] = useState<Reminder[]>([]);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const f =
        filter === "active"
          ? { fired: false, dismissed: false }
          : filter === "fired"
          ? { fired: true, dismissed: false }
          : filter === "dismissed"
          ? { dismissed: true }
          : undefined;
      const list = await invoke<Reminder[]>("list_reminders", { filter: f });
      setItems(list);
    } finally {
      setLoading(false);
    }
  }, [filter]);

  useEffect(() => {
    load();
  }, [load]);

  const dismiss = async (id: number) => {
    await invoke("dismiss_reminder", { id });
    load();
  };

  return (
    <div className="px-10 py-6 max-w-3xl mx-auto">
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
        <p className="text-bone-3 text-xs">No reminders here.</p>
      ) : (
        <div className="flex flex-col gap-2">
          <AnimatePresence initial={false} mode="popLayout">
            {items.map((r) => (
              <motion.div
                key={r.id}
                layout
                initial={{ opacity: 0, x: -8 }}
                animate={{ opacity: 1, x: 0 }}
                exit={{ opacity: 0, x: 8 }}
                transition={{ duration: 0.2 }}
                className="flex items-start gap-3 px-3 py-3 rounded-lg border border-white/[0.04] bg-ink-2/30"
              >
                <span
                  className={
                    "h-2 w-2 rounded-full mt-1.5 flex-shrink-0 " +
                    (r.dismissedAt
                      ? "bg-bone-3/40"
                      : r.firedAt
                      ? "bg-warn/80"
                      : "bg-pulse-2 shadow-[0_0_8px_rgba(110,196,232,0.6)]")
                  }
                />
                <div className="flex-1 min-w-0">
                  <div className="text-bone-2 text-sm">{r.text}</div>
                  <div className="flex items-center gap-3 mt-1 text-[10px] font-mono text-bone-3">
                    <span>{r.kind}</span>
                    {r.remindAt && <span>at {r.remindAt}</span>}
                    {r.firedAt && <span>fired {r.firedAt.slice(5, 16).replace("T", " ")}</span>}
                    {r.dismissedAt && <span>dismissed</span>}
                    <span className="ml-auto opacity-60">{r.source}</span>
                  </div>
                </div>
                {!r.dismissedAt && (
                  <button
                    onClick={() => dismiss(r.id)}
                    className="text-bone-3 hover:text-bone-2 text-[11px] underline-offset-4 hover:underline"
                  >
                    dismiss
                  </button>
                )}
              </motion.div>
            ))}
          </AnimatePresence>
        </div>
      )}
    </div>
  );
}
