import { useCallback, useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { listTasks, setTaskStatus, type Task, type TaskStatus } from "../../lib/domain";

const filters: { id: TaskStatus | "all"; label: string }[] = [
  { id: "all", label: "All" },
  { id: "open", label: "Open" },
  { id: "done", label: "Done" },
  { id: "snoozed", label: "Snoozed" },
  { id: "dropped", label: "Dropped" },
];

export default function TasksTab() {
  const [filter, setFilter] = useState<(typeof filters)[number]["id"]>("open");
  const [tasks, setTasks] = useState<Task[]>([]);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const t = await listTasks(filter === "all" ? undefined : { status: filter });
      setTasks(t);
    } finally {
      setLoading(false);
    }
  }, [filter]);

  useEffect(() => {
    load();
  }, [load]);

  const toggle = async (t: Task) => {
    await setTaskStatus(t.id, t.status === "done" ? "open" : "done");
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
      ) : tasks.length === 0 ? (
        <p className="text-bone-3 text-xs">No tasks for this filter.</p>
      ) : (
        <div className="flex flex-col">
          <AnimatePresence initial={false} mode="popLayout">
            {tasks.map((t) => (
              <motion.button
                key={t.id}
                layout
                initial={{ opacity: 0, x: -8 }}
                animate={{ opacity: 1, x: 0 }}
                exit={{ opacity: 0, x: 8 }}
                transition={{ duration: 0.2 }}
                onClick={() => toggle(t)}
                className="text-left flex items-start gap-3 px-3 py-3 rounded-lg hover:bg-white/[0.03] transition-colors group border-b border-white/[0.03]"
              >
                <span
                  className={
                    "h-4 w-4 rounded-full mt-0.5 flex-shrink-0 transition-all " +
                    (t.status === "done"
                      ? "bg-pulse-2/60 border border-pulse-2/70"
                      : "border border-bone-3/40 group-hover:border-pulse-2/80 group-hover:bg-pulse-2/10")
                  }
                />
                <div className="flex-1 min-w-0">
                  <div
                    className={
                      "text-sm " +
                      (t.status === "done"
                        ? "text-bone-3 line-through decoration-bone-3/40"
                        : "text-bone-2")
                    }
                  >
                    {t.title}
                  </div>
                  {t.description && (
                    <div className="text-bone-3 text-[11px] mt-0.5 line-clamp-2">
                      {t.description}
                    </div>
                  )}
                  <div className="flex items-center gap-3 mt-1.5 text-[10px] font-mono text-bone-3">
                    <span className="opacity-70">{t.source}</span>
                    {t.dueAt && <span>due {t.dueAt}</span>}
                    {t.completedAt && <span>done {t.completedAt.slice(5, 16).replace("T", " ")}</span>}
                    {t.priority !== 0 && (
                      <span className={t.priority > 0 ? "text-warn" : "opacity-50"}>
                        p={t.priority}
                      </span>
                    )}
                    {t.linkKind && (
                      <span className="text-pulse-2/70">
                        {t.linkKind}#{t.linkId}
                      </span>
                    )}
                  </div>
                </div>
              </motion.button>
            ))}
          </AnimatePresence>
        </div>
      )}
    </div>
  );
}
