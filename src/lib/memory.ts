import { invoke } from "@tauri-apps/api/core";

export type MemoryHit = {
  kind: string;
  sourceId: number;
  text: string;
  score: number;
  similarity: number;
  recency: number;
  entityMatch: number;
  createdAt: string;
};

import type { ConversationMessage } from "./conversation";

export type AskResponse = {
  conversationId: number;
  answer: string;
  messages: ConversationMessage[];
  sources: MemoryHit[];
  model: string;
};

/**
 * Bulk (re)index every journal entry's raw_text into the embedding store.
 * Idempotent — replaces existing rows. Returns the number of entries indexed.
 *
 * NOTE: First call may take several seconds while fastembed downloads the
 * BGE-small model (~130MB) into the OS cache dir. Subsequent calls are fast.
 */
export const indexAllJournalEntries = () =>
  invoke<number>("index_all_journal_entries");

/**
 * Ask Travis a question. Travis pulls top-5 semantic hits + up to 10 open tasks
 * as grounding context, then calls the user's configured LLM provider.
 */
export const askTravis = (question: string, conversationId?: number) =>
  invoke<AskResponse>("ask_travis", { question, conversationId });
