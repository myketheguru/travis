import { invoke } from "@tauri-apps/api/core";

export type EventRow = {
  id: number;
  kind: string;
  refKind: string | null;
  refId: number | null;
  payloadJson: string | null;
  createdAt: string;
};

export type DetectedPattern = {
  id: number;
  sequenceJson: string;
  occurrences: number;
  lastSeen: string;
  suggestedAt: string | null;
  dismissedAt: string | null;
  automatedAt: string | null;
  createdAt: string;
};

export const listEvents = (limit?: number, kind?: string) =>
  invoke<EventRow[]>("list_events", { limit, kind });

export const listPatterns = () => invoke<DetectedPattern[]>("list_patterns");

export const detectPatterns = () => invoke<number>("detect_patterns");

export const dismissPattern = (id: number) =>
  invoke<void>("dismiss_pattern", { id });
