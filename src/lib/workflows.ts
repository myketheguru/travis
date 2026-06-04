import { invoke } from "@tauri-apps/api/core";

export type SlotSource =
  | "user_typed"
  | "extracted"
  | "user_dropped"
  | "graph_resolved";

export interface WorkflowSlotSurface {
  name: string;
  label: string;
  kind: string;
  required: boolean;
  filled: boolean;
  valuePreview?: string | null;
  source?: SlotSource | null;
  resolvedAt?: string | null;
}

export interface WorkflowNextAsk {
  slotName: string;
  label: string;
  kind: string;
  askHint: string;
}

export interface WorkflowSurface {
  id: number;
  conversationId: number;
  recipeName: string;
  displayName: string;
  description: string;
  status: string;
  startedIntent?: string | null;
  startedAt: string;
  lastActivityAt: string;
  finalizeAction: string;
  slots: WorkflowSlotSurface[];
  filledCount: number;
  requiredTotal: number;
  nextAsk?: WorkflowNextAsk | null;
}

/// Fetch the active workflow for a conversation. Returns null when
/// nothing is in flight.
export async function getActiveWorkflow(
  conversationId: number,
): Promise<WorkflowSurface | null> {
  return await invoke<WorkflowSurface | null>("get_active_workflow", {
    conversationId,
  });
}
