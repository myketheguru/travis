import { invoke } from "@tauri-apps/api/core";
import type { Task } from "./domain";
import type { Thread } from "./conversation";
import type { ProposedAction } from "./actions";

/// Pack-aware bag of named-entity mentions extracted from a journal note.
/// Keys are pluralised entity kinds declared by the enabled packs (for the
/// L2E pack: `coaches`, `schools`, `depts`). Missing keys mean the LLM
/// returned no matches for that bucket — read with `?? []` to be safe.
export type EntityMentions = Record<string, string[]>;

export type ExtractedReminder = {
  text: string;
  remindAt: string | null;
};

export type CapabilityGap = {
  capability: string;
  context: string | null;
};

export type RoutingResult = {
  workspaceSlug: string;
  workspaceName: string;
  routed: boolean;
  confidence: "high" | "medium" | "low" | null;
  rationale: string | null;
};

export type JournalIngestResult = {
  journalEntryId: number;
  conversationId: number;
  thread: Thread;
  intent: "operational" | "conversational" | string;
  response: string | null;
  tasksCreated: Task[];
  tasksCompleted: Task[];
  entities: EntityMentions;
  reminders: ExtractedReminder[];
  clarifyingQuestions: string[];
  capabilityGaps: CapabilityGap[];
  proposedActions: ProposedAction[];
  routing: RoutingResult | null;
  extractionOk: boolean;
  error: string | null;
};

export type JournalEntry = {
  id: number;
  rawText: string;
  extractionOk: number;
  provider: string | null;
  model: string | null;
  createdAt: string;
};

export const journalIngest = (text: string, conversationId?: number) =>
  invoke<JournalIngestResult>("journal_ingest", { text, conversationId });

export const listJournalEntries = (limit?: number) =>
  invoke<JournalEntry[]>("list_journal_entries", { limit });
