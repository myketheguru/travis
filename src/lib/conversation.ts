import { invoke } from "@tauri-apps/api/core";

export type Conversation = {
  id: number;
  kind: string;
  title: string | null;
  status: "open" | "awaiting_user" | "resolved" | string;
  linkKind: string | null;
  linkId: number | null;
  createdAt: string;
  updatedAt: string;
};

export type ConversationMessage = {
  id: number;
  conversationId: number;
  role: "user" | "assistant" | "system" | "tool" | string;
  content: string;
  payloadJson: string | null;
  createdAt: string;
  /// v0.17.0 — classifier stamped by the agent loop. Drives ChatTurn's
  /// distinct reasoning-card render. NULL for messages written before
  /// v0.17.0 (UI falls back to the standard bubble).
  responseKind?: "extraction" | "text_response" | "reasoning_only" | null;
};

export type Thread = {
  conversation: Conversation;
  messages: ConversationMessage[];
};

export type ConversationFilter = {
  status?: string;
  kind?: string;
};

export const listConversations = (filter?: ConversationFilter, limit?: number) =>
  invoke<Conversation[]>("list_conversations", { filter, limit });

export const getThread = (conversationId: number) =>
  invoke<Thread>("get_thread", { conversationId });

export const activeConversation = () => invoke<Thread | null>("active_conversation");

/// v0.18.2 — load older messages on scroll-up. Returns at most `limit`
/// (default 50, max 200) messages with ids < `beforeId`, in ASC order.
export const loadMoreMessages = (
  conversationId: number,
  beforeId: number,
  limit?: number,
) =>
  invoke<ConversationMessage[]>("load_more_messages", {
    conversationId,
    beforeId,
    limit,
  });

/// v0.18.3 — conversation switcher row shape.
export type ConversationListItem = {
  id: number;
  title: string | null;
  preview: string | null;
  status: string;
  kind: string;
  messageCount: number;
  updatedAt: string;
  createdAt: string;
};

/// v0.18.3 — switcher backing API. Optional `query` does case-
/// insensitive substring match on title OR any message body content.
export const listConversationsForSwitcher = (query?: string, limit?: number) =>
  invoke<ConversationListItem[]>("list_conversations_for_switcher", {
    query: query || null,
    limit,
  });

export const resolveConversation = (id: number) =>
  invoke<void>("resolve_conversation", { id });

export const appendUserMessage = (conversationId: number, content: string) =>
  invoke<ConversationMessage>("append_user_message", { conversationId, content });

export const deleteMessageAndAfter = (conversationId: number, messageId: number) =>
  invoke<number>("delete_message_and_after", { conversationId, messageId });
