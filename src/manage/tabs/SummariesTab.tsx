import { useCallback, useEffect, useState } from "react";
import { motion } from "framer-motion";
import { invoke } from "@tauri-apps/api/core";
import { useAppStore } from "../../stores/app";

type Summary = {
  id: number;
  kind: "daily" | "weekly";
  periodStart: string;
  periodEnd: string;
  content: string;
  sourceCount: number;
  model: string | null;
  provider: string | null;
  createdAt: string;
};

function todayIso(): string {
  const d = new Date();
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

function mondayOfWeek(): string {
  const d = new Date();
  const dow = (d.getDay() + 6) % 7;
  d.setDate(d.getDate() - dow);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
}

export default function SummariesTab() {
  const [summaries, setSummaries] = useState<Summary[]>([]);
  const [date, setDate] = useState(todayIso());
  const [weekStart, setWeekStart] = useState(mondayOfWeek());
  const [busy, setBusy] = useState<"daily" | "weekly" | null>(null);
  const [error, setError] = useState<string | null>(null);
  const setActivity = useAppStore((s) => s.setActivity);

  const load = useCallback(async () => {
    try {
      const list = await invoke<Summary[]>("list_summaries", { kind: undefined, limit: 30 });
      setSummaries(list);
    } catch {
      setSummaries([]);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const generate = async (which: "daily" | "weekly") => {
    setBusy(which);
    setError(null);
    setActivity("thinking");
    try {
      if (which === "daily") {
        await invoke<Summary>("generate_daily_summary", { date });
      } else {
        await invoke<Summary>("generate_weekly_summary", { weekStart });
      }
      await load();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setActivity("idle");
      setBusy(null);
    }
  };

  return (
    <div className="px-10 py-6 max-w-3xl mx-auto flex flex-col gap-7">
      <div className="grid grid-cols-2 gap-4">
        <div className="rounded-xl border border-ink-3 bg-ink-2/30 p-4 flex flex-col gap-3">
          <div className="text-bone-3 text-[10px] tracking-[0.18em] uppercase">Daily summary</div>
          <input
            type="date"
            value={date}
            onChange={(e) => setDate(e.target.value)}
            className="bg-ink-2/70 border border-ink-3 rounded px-2 py-1.5 text-bone text-sm font-mono focus:outline-none focus:border-pulse/60"
          />
          <button
            onClick={() => generate("daily")}
            disabled={busy === "daily"}
            className="px-4 py-1.5 rounded-full bg-bone/95 text-ink text-xs font-medium hover:bg-bone disabled:opacity-30 transition-colors"
          >
            {busy === "daily" ? "Generating…" : "Generate"}
          </button>
        </div>

        <div className="rounded-xl border border-ink-3 bg-ink-2/30 p-4 flex flex-col gap-3">
          <div className="text-bone-3 text-[10px] tracking-[0.18em] uppercase">Weekly (week starts)</div>
          <input
            type="date"
            value={weekStart}
            onChange={(e) => setWeekStart(e.target.value)}
            className="bg-ink-2/70 border border-ink-3 rounded px-2 py-1.5 text-bone text-sm font-mono focus:outline-none focus:border-pulse/60"
          />
          <button
            onClick={() => generate("weekly")}
            disabled={busy === "weekly"}
            className="px-4 py-1.5 rounded-full bg-bone/95 text-ink text-xs font-medium hover:bg-bone disabled:opacity-30 transition-colors"
          >
            {busy === "weekly" ? "Generating…" : "Generate"}
          </button>
        </div>
      </div>

      {error && <p className="text-warn text-xs">{error}</p>}

      <div className="flex flex-col gap-3">
        {summaries.length === 0 ? (
          <p className="text-bone-3 text-xs">No summaries yet.</p>
        ) : (
          summaries.map((s) => (
            <motion.div
              key={s.id}
              initial={{ opacity: 0, y: 4 }}
              animate={{ opacity: 1, y: 0 }}
              transition={{ duration: 0.25 }}
              className="rounded-xl border border-ink-3 bg-ink-2/30 p-4"
            >
              <div className="flex items-center gap-3 text-[10px] font-mono text-bone-3 mb-2">
                <span className="text-pulse-2/80 uppercase">{s.kind}</span>
                <span>
                  {s.periodStart}
                  {s.kind === "weekly" && ` → ${s.periodEnd}`}
                </span>
                <span className="opacity-60">· {s.sourceCount} source{s.sourceCount === 1 ? "" : "s"}</span>
                {s.model && <span className="ml-auto opacity-60">{s.model}</span>}
              </div>
              <p className="text-bone-2 text-sm leading-relaxed whitespace-pre-wrap">
                {s.content}
              </p>
            </motion.div>
          ))
        )}
      </div>
    </div>
  );
}
