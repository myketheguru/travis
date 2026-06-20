/**
 * v2 Phase 4 — the "While You Were Away" feed.
 *
 * Pulls recent workflow_run rows from the cloud and renders them as a
 * vertical timeline. Each item carries the schedule that fired (or
 * "manual"), the started_at time, the LLM's free-text output, and any
 * error state.
 *
 * The whole point of this surface is the *opening it* moment: a user
 * comes back to their computer in the morning, opens Travis, and sees
 * the work it did overnight. That feeling is what unlocks Pro→Max
 * upgrades.
 */
import { useEffect, useState } from "react";
import { motion } from "framer-motion";
import {
  cloudWorkflowRunNow,
  cloudWorkflowRuns,
  cloudWorkflowSchedules,
  type WorkflowRun,
  type WorkflowSchedule,
} from "../lib/cloud";

function fmtRelative(iso: string): string {
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return iso;
  const diffSec = Math.max(0, Math.round((Date.now() - t) / 1000));
  if (diffSec < 60) return `${diffSec}s ago`;
  const min = Math.round(diffSec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.round(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const d = Math.round(hr / 24);
  return `${d}d ago`;
}

export function WhileYouWereAway({ onClose }: { onClose: () => void }) {
  const [runs, setRuns] = useState<WorkflowRun[] | null>(null);
  const [schedules, setSchedules] = useState<WorkflowSchedule[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [running, setRunning] = useState(false);
  const [quickPrompt, setQuickPrompt] = useState("");

  async function refresh() {
    try {
      const [r, s] = await Promise.all([
        cloudWorkflowRuns(),
        cloudWorkflowSchedules(),
      ]);
      setRuns(r);
      setSchedules(s);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  }

  useEffect(() => {
    void refresh();
    const t = setInterval(refresh, 30_000);
    return () => clearInterval(t);
  }, []);

  async function handleRunNow() {
    if (!quickPrompt.trim() || running) return;
    setRunning(true);
    setError(null);
    try {
      await cloudWorkflowRunNow({ prompt: quickPrompt.trim() });
      setQuickPrompt("");
      await refresh();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setRunning(false);
    }
  }

  const completedCount = runs?.filter((r) => r.status === "completed").length ?? 0;
  const failedCount = runs?.filter((r) => r.status === "failed").length ?? 0;
  const activeSchedules = schedules?.filter((s) => s.is_active === 1).length ?? 0;

  return (
    <main className="relative h-full w-full overflow-y-auto px-6 py-8">
      <div className="max-w-2xl mx-auto">
        <div className="flex items-start justify-between mb-8">
          <div>
            <div className="text-bone-3 text-[11px] tracking-[0.18em] uppercase mb-2">
              // while you were away
            </div>
            <h1 className="text-3xl font-light tracking-tight text-bone">
              Travis got to work.
            </h1>
            {(completedCount > 0 || activeSchedules > 0) && (
              <p className="text-bone-3 text-sm mt-2">
                {completedCount} completed
                {failedCount > 0 ? `, ${failedCount} failed` : ""}
                {activeSchedules > 0 ? ` · ${activeSchedules} active` : ""}
              </p>
            )}
          </div>
          <button
            onClick={onClose}
            className="text-bone-3 hover:text-bone-2 text-xs px-3 py-1.5 rounded-md border border-ink-3 hover:border-ink-3/80 transition-colors"
          >
            Close
          </button>
        </div>

        <motion.div
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.35 }}
          className="rounded-2xl border border-pulse/30 bg-pulse/[0.04] p-4 mb-7"
        >
          <p className="text-bone-2 text-xs mb-2 font-medium">
            Run a one-off now
          </p>
          <div className="flex gap-2">
            <input
              value={quickPrompt}
              onChange={(e) => setQuickPrompt(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") void handleRunNow();
              }}
              placeholder="Summarize what's left from yesterday's notes"
              className="flex-1 bg-ink-2/40 border border-ink-3 rounded-lg px-3 py-2 text-sm text-bone placeholder:text-bone-3 focus:outline-none focus:border-pulse-2"
            />
            <button
              onClick={handleRunNow}
              disabled={running || !quickPrompt.trim()}
              className="px-4 py-2 rounded-lg bg-bone text-ink text-sm font-medium hover:bg-white disabled:opacity-40 disabled:cursor-not-allowed transition-all"
            >
              {running ? "Running…" : "Run"}
            </button>
          </div>
          <p className="text-bone-3 text-[10px] mt-2 leading-relaxed">
            Travis will run this in the cloud right now and add the result below.
          </p>
        </motion.div>

        {error && (
          <div className="mb-6 px-4 py-3 rounded-lg border border-warn/30 bg-warn/5 text-warn text-xs leading-relaxed">
            {error}
          </div>
        )}

        {runs === null && (
          <div className="text-bone-3 text-sm text-center py-12">
            Loading the last few runs…
          </div>
        )}

        {runs && runs.length === 0 && (
          <div className="text-center py-12">
            <div className="text-bone-3 text-sm leading-relaxed max-w-xs mx-auto">
              No runs yet. Set up a schedule and Travis will keep working in
              the background while you do other things.
            </div>
          </div>
        )}

        {runs && runs.length > 0 && (
          <div className="flex flex-col gap-3">
            {runs.map((r, i) => (
              <motion.div
                key={r.id}
                initial={{ opacity: 0, y: 8 }}
                animate={{ opacity: 1, y: 0 }}
                transition={{ duration: 0.3, delay: i * 0.04 }}
                className="rounded-xl border border-ink-3 bg-ink-2/30 p-4"
              >
                <div className="flex items-center justify-between mb-2">
                  <div className="flex items-center gap-2 min-w-0">
                    <span
                      className="h-1.5 w-1.5 rounded-full shrink-0"
                      style={{
                        background:
                          r.status === "completed"
                            ? "var(--color-pulse-2)"
                            : r.status === "failed"
                            ? "var(--color-warn)"
                            : "var(--color-pulse)",
                        boxShadow:
                          r.status === "completed"
                            ? "0 0 6px rgba(110,196,232,0.6)"
                            : r.status === "failed"
                            ? "0 0 6px rgba(255,184,107,0.6)"
                            : "0 0 6px rgba(124,92,255,0.6)",
                      }}
                    />
                    <span className="text-bone-2 text-xs truncate">
                      {r.schedule_name ?? (r.source === "manual" ? "Manual" : "Workflow")}
                    </span>
                  </div>
                  <span className="text-bone-3 text-[10px] font-mono shrink-0">
                    {fmtRelative(r.started_at)}
                  </span>
                </div>

                {r.status === "completed" && r.result_text && (
                  <div className="text-bone text-sm leading-relaxed whitespace-pre-wrap">
                    {r.result_text}
                  </div>
                )}
                {r.status === "running" && (
                  <div className="text-bone-3 text-xs italic">Running…</div>
                )}
                {r.status === "failed" && (
                  <div className="text-warn text-xs leading-relaxed">
                    {r.error_message ?? "Failed without an error message."}
                  </div>
                )}

                {(r.input_tokens > 0 || r.output_tokens > 0) && (
                  <div className="mt-3 pt-3 border-t border-ink-3/60 flex items-center gap-3 text-[10px] font-mono text-bone-3">
                    <span>
                      <span className="text-bone-2">{r.input_tokens}</span> in
                    </span>
                    <span>
                      <span className="text-bone-2">{r.output_tokens}</span> out
                    </span>
                    {r.cost_usd_cents > 0 && (
                      <span>
                        <span className="text-bone-2">
                          ${(r.cost_usd_cents / 100).toFixed(2)}
                        </span>
                      </span>
                    )}
                  </div>
                )}
              </motion.div>
            ))}
          </div>
        )}
      </div>
    </main>
  );
}
