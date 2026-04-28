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

export const resolveConversation = (id: number) =>
  invoke<void>("resolve_conversation", { id });

export const appendUserMessage = (conversationId: number, content: string) =>
  invoke<ConversationMessage>("append_user_message", { conversationId, content });
