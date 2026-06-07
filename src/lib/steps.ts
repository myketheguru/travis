import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

export type StepKind =
  | "tool_call"
  | "action"
  | "code_execution"
  | "thinking"
  | "workflow_op";

export type StepStatus = "running" | "ok" | "failed" | "cancelled";

export interface StepRow {
  id: string;
  conversationId: number;
  parentStepId?: string | null;
  kind: string;
  name: string;
  detail?: string | null;
  status: string;
  summary?: string | null;
  notesJson: string;
  startedAt: string;
  completedAt?: string | null;
  durationMs?: number | null;
}

export interface ParsedStep extends Omit<StepRow, "notesJson"> {
  notes: string[];
}

/// Backend-emitted events on the step-event channel.
export type StepEvent =
  | {
      event: "started";
      stepId: string;
      parentStepId?: string | null;
      conversationId: number;
      kind: StepKind;
      name: string;
      detail?: string | null;
      startedAt: string;
    }
  | {
      event: "note";
      stepId: string;
      text: string;
    }
  | {
      event: "result";
      stepId: string;
      status: StepStatus;
      summary?: string | null;
      error?: string | null;
    }
  | {
      event: "completed";
      stepId: string;
      durationMs: number;
    };

export async function listSteps(conversationId: number): Promise<ParsedStep[]> {
  const rows = await invoke<StepRow[]>("list_steps", { conversationId });
  return rows.map(parseRow);
}

export function parseRow(row: StepRow): ParsedStep {
  let notes: string[] = [];
  try {
    notes = JSON.parse(row.notesJson) as string[];
    if (!Array.isArray(notes)) notes = [];
  } catch {
    notes = [];
  }
  return { ...row, notes };
}

/// Subscribe to live step events. Caller invokes the returned function to
/// unsubscribe.
export async function subscribeSteps(
  onEvent: (event: StepEvent) => void,
): Promise<UnlistenFn> {
  return await listen<StepEvent>("step-event", (event) => {
    onEvent(event.payload);
  });
}

/// Friendly duration label ("1.2s", "340ms", "—").
export function formatDuration(ms?: number | null): string {
  if (ms === null || ms === undefined) return "—";
  if (ms < 1000) return `${ms}ms`;
  if (ms < 10_000) return `${(ms / 1000).toFixed(1)}s`;
  return `${Math.round(ms / 1000)}s`;
}
