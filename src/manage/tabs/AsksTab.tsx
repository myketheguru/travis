import { useCallback, useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import {
  ackFeedback,
  deleteFeedback,
  listFeedback,
  type AppFeedback,
} from "../../lib/feedback";

type FilterId = "open" | "addressed" | "all";

const filters: { id: FilterId; label: string }[] = [
  { id: "open", label: "Pending" },
  { id: "addressed", label: "Addressed" },
  { id: "all", label: "All" },
];

export default function AsksTab() {
  const [filter, setFilter] = useState<FilterId>("open");
  const [items, setItems] = useState<AppFeedback[]>([]);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const f =
        filter === "open"
          ? { addressed: false }
          : filter === "addressed"
          ? { addressed: true }
          : undefined;
      const list = await listFeedback(f);
      setItems(list);
    } finally {
      setLoading(false);
    }
  }, [filter]);

  useEffect(() => {
    load();
  }, [load]);

  const ack = async (id: number) => {
    await ackFeedback(id);
    load();
  };
  const remove = async (id: number) => {
    await deleteFeedback(id);
    load();
  };

  // Group by capability so repeated asks aggregate.
  const grouped = items.reduce<Record<string, AppFeedback[]>>((acc, f) => {
    const k = f.capability;
    (acc[k] ||= []).push(f);
    return acc;
  }, {});
  const groupKeys = Object.keys(grouped).sort(
    (a, b) => grouped[b].length - grouped[a].length,
  );

  return (
    <div className="px-10 py-6 max-w-3xl mx-auto">
      <p className="text-bone-3 text-xs mb-4 leading-relaxed">
        When Travis notices you want something it can't do yet — send an email,
        draft an invoice, schedule a meeting — it logs it here. The more often
        you ask for the same thing, the higher it'll rank for me to build.
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
      ) : groupKeys.length === 0 ? (
        <p className="text-bone-3 text-xs">
          {filter === "open"
            ? "Nothing pending. Travis hasn't spotted any unmet wants yet."
            : "Empty."}
        </p>
      ) : (
        <div className="flex flex-col gap-4">
          <AnimatePresence initial={false} mode="popLayout">
            {groupKeys.map((k) => {
              const group = grouped[k];
              const recent = group[0];
              return (
                <motion.div
                  key={k}
                  layout
                  initial={{ opacity: 0, y: 4 }}
                  animate={{ opacity: 1, y: 0 }}
                  exit={{ opacity: 0, y: -4 }}
                  transition={{ duration: 0.2 }}
                  className="rounded-xl border border-ink-3 bg-ink-2/30 p-4"
                >
                  <div className="flex items-center justify-between mb-2">
                    <span className="text-bone font-medium text-sm">{k}</span>
                    <span className="text-bone-3 text-[10px] font-mono">
                      {group.length} ask{group.length === 1 ? "" : "s"}
                    </span>
                  </div>
                  {group.slice(0, 3).map((f) => (
                    <div
                      key={f.id}
                      className="border-t border-white/[0.04] pt-2 mt-2 flex items-start gap-3"
                    >
                      <div className="flex-1 min-w-0">
                        {f.context && (
                          <p className="text-bone-2 text-xs leading-relaxed line-clamp-2">
                            "{f.context}"
                          </p>
                        )}
                        <span className="text-bone-3 text-[10px] font-mono">
                          {f.createdAt.slice(0, 16).replace("T", " ")}
                          {f.addressedAt && " · addressed"}
                        </span>
                      </div>
                      <div className="flex items-center gap-2">
                        {!f.addressedAt && (
                          <button
                            onClick={() => ack(f.id)}
                            className="text-pulse-2 hover:text-bone text-[11px] underline-offset-4 hover:underline"
                          >
                            ack
                          </button>
                        )}
                        <button
                          onClick={() => remove(f.id)}
                          className="text-bone-3 hover:text-warn text-[11px] underline-offset-4 hover:underline"
                        >
                          delete
                        </button>
                      </div>
                    </div>
                  ))}
                  {group.length > 3 && (
                    <span className="text-bone-3 text-[10px] mt-2 block">
                      and {group.length - 3} more…
                    </span>
                  )}
                  <div className="mt-3 pt-2 border-t border-white/[0.04] flex items-center gap-3 text-[10px] font-mono text-bone-3">
                    <span>most recent: {recent.createdAt.slice(0, 10)}</span>
                  </div>
                </motion.div>
              );
            })}
          </AnimatePresence>
        </div>
      )}
    </div>
  );
}
