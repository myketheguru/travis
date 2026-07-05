/**
 * useAttentionItems — polls the sources that feed the attention strip
 * and returns a normalized array of `AttentionItem`s the strip renders.
 *
 * Sources (Shell 6):
 * - T2T inbox — pending queries from another Travis waiting for a reply
 *   or approval on a draft
 * - Workflow runs — in-flight or recently-finished runs (While You
 *   Were Away signal)
 * - (Future) Reminders due today
 * - (Future) Local drafts awaiting approval
 *
 * Poll interval: 30s. Cheap enough — each fetch is small and cached
 * server-side. If the user is offline / signed out the calls throw and
 * we return an empty list without surfacing an error (attention is
 * ambient, not authoritative).
 */
import { useEffect, useState } from "react";
import {
  cloudWorkflowRuns,
  t2tInbox,
  type WorkflowRun,
  type T2tQuery,
} from "../lib/cloud";

export type AttentionKind =
  | "t2t_pending"
  | "t2t_drafted"
  | "workflow_running"
  | "workflow_awaiting_approval";

export interface AttentionItem {
  /** Stable id — used as the React key + for deep-linking to the source. */
  id: string;
  kind: AttentionKind;
  /** One-line label. Kept under ~60 chars so it fits the strip. */
  label: string;
  /** Optional detail line for tooltip / hover expansion. */
  detail?: string;
  /** ISO timestamp used for sort. Newer first. */
  timestamp: string;
  /** Optional action when the row is clicked. Shell 8 wires this up. */
  href?: string;
}

const POLL_MS = 30_000;

function normalizeT2t(inbox: T2tQuery[]): AttentionItem[] {
  return inbox
    .filter((q) => q.status === "pending" || q.status === "drafted")
    .map((q) => ({
      id: `t2t:${q.id}`,
      kind: (q.status === "drafted" ? "t2t_drafted" : "t2t_pending") as AttentionKind,
      label:
        q.status === "drafted"
          ? `Draft reply ready for ${q.from_name ?? q.from_email ?? "someone"}`
          : `${q.from_name ?? q.from_email ?? "Someone"} asked: ${truncate(q.question, 44)}`,
      detail: q.question,
      timestamp: q.drafted_at ?? q.created_at,
      href: `t2t/query/${q.id}`,
    }));
}

function normalizeWorkflow(runs: WorkflowRun[]): AttentionItem[] {
  return runs
    .filter(
      (r) =>
        r.status === "running" ||
        r.status === "queued" ||
        // Runs that produced proposed actions surface as "awaiting approval".
        (r.status === "succeeded" && (r.result_actions_json?.length ?? 0) > 2),
    )
    .map((r) => ({
      id: `wf:${r.id}`,
      kind:
        r.status === "succeeded"
          ? ("workflow_awaiting_approval" as AttentionKind)
          : ("workflow_running" as AttentionKind),
      label:
        r.status === "succeeded"
          ? `${r.schedule_name ?? "Workflow"} finished — actions to review`
          : `${r.schedule_name ?? "Workflow"} running…`,
      detail: r.result_text ?? undefined,
      timestamp: r.finished_at ?? r.started_at,
      href: `workflow/run/${r.id}`,
    }));
}

function truncate(s: string, n: number): string {
  return s.length <= n ? s : s.slice(0, n - 1).trimEnd() + "…";
}

export function useAttentionItems() {
  const [items, setItems] = useState<AttentionItem[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;

    async function tick() {
      const gathered: AttentionItem[] = [];
      const [inboxResult, runsResult] = await Promise.allSettled([
        t2tInbox(),
        cloudWorkflowRuns(),
      ]);
      if (inboxResult.status === "fulfilled") {
        gathered.push(...normalizeT2t(inboxResult.value));
      }
      if (runsResult.status === "fulfilled") {
        gathered.push(...normalizeWorkflow(runsResult.value));
      }
      gathered.sort((a, b) => b.timestamp.localeCompare(a.timestamp));
      if (!cancelled) {
        setItems(gathered);
        setLoading(false);
      }
    }

    void tick();
    const handle = setInterval(tick, POLL_MS);
    return () => {
      cancelled = true;
      clearInterval(handle);
    };
  }, []);

  return { items, loading };
}
