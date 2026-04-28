import { invoke } from "@tauri-apps/api/core";

export type AppFeedback = {
  id: number;
  capability: string;
  context: string | null;
  sourceKind: string | null;
  sourceId: number | null;
  addressedAt: string | null;
  createdAt: string;
};

export const listFeedback = (filter?: { addressed?: boolean }) =>
  invoke<AppFeedback[]>("list_feedback", { filter });

export const ackFeedback = (id: number) => invoke<void>("ack_feedback", { id });

export const deleteFeedback = (id: number) => invoke<void>("delete_feedback", { id });
