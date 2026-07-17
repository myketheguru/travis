/**
 * chatStore — v0.28.70 rearchitecture.
 *
 * Message-map state modeled directly on lobehub/lobe-chat's
 * `src/store/chat/slices/message`. One `messagesMap` keyed by
 * conversation id; reducer-style actions mutate individual messages
 * by (tmpId or real DB id). Streaming, tool calls, reasoning,
 * errors, aborts all live as fields on the UIMessage — no parallel
 * "streamingAssistant" slot, no polling, no optimistic bubble
 * managed elsewhere.
 *
 * The old flow (v0.28.66) had TWO stores fighting: `streamingAssistant`
 * for the in-flight bubble + `useFocalContent` polling the DB for
 * persisted rows, meeting at a "swap" moment. That's why the audio
 * card, streaming, and message rendering felt patchy — different
 * pieces of the same turn lived in different places.
 *
 * This store is the single source of truth for chat rendering.
 */
import { create } from "zustand";
import { subscribeWithSelector } from "zustand/middleware";

export type MessageId = number | string;

/**
 * v0.28.70 — UIMessage. Shape mirrors lobe-chat's UIChatMessage
 * (packages/types/src/message/ui/chat.ts:195-345), stripped to what
 * Travis actually needs today. All streaming state lives here.
 */
export interface UIMessage {
  /** Real DB id (numeric) once known; "tmp_XXX" while optimistic. */
  id: MessageId;
  /** Set while id is a tmp; correlator for swapTmpId. */
  tmpId?: string;
  conversationId: number;
  role: "user" | "assistant" | "system";
  /** Free-text markdown content. Accumulates during stream. */
  content: string;
  /** ISO timestamp. */
  createdAt: string;
  /**
   * True while the assistant turn is actively streaming. Renderer
   * shows a cursor at the tail and hides the hover-action strip.
   */
  streaming?: boolean;
  /** Extended thinking / reasoning body (Anthropic thinking blocks). */
  reasoning?: string;
  /** Tool calls emitted during the turn. */
  toolCalls?: Array<{ id: string; name: string }>;
  /** Voice audio metadata for user turns. */
  audio?: { audioPath: string; durationMs: number; transcript: string };
  /**
   * Set if the turn errored or was aborted. Renderer shows an
   * error footer under whatever partial content landed. Modeled
   * on lobe-chat's `error` field + InterruptedHint.
   */
  error?: string;
  aborted?: boolean;
}

interface ChatState {
  /** Per-conversation message list. Ordered by createdAt. */
  messagesMap: Record<number, UIMessage[]>;

  /** Create a message. Overwrites if id already exists (idempotent). */
  createMessage: (m: UIMessage) => void;

  /** Patch a message by id. */
  updateMessage: (
    conversationId: number,
    id: MessageId,
    patch: Partial<UIMessage>,
  ) => void;

  /** Append text to a message's content (streaming). */
  appendContent: (conversationId: number, id: MessageId, delta: string) => void;

  /** Append to reasoning. */
  appendReasoning: (
    conversationId: number,
    id: MessageId,
    delta: string,
  ) => void;

  /** Add a tool call to a message (dedup by id). */
  addToolCall: (
    conversationId: number,
    id: MessageId,
    tc: { id: string; name: string },
  ) => void;

  /**
   * Swap a temporary id for its real DB id. Preserves content and
   * all other fields. Used when the persisted DB row's id comes
   * back after journal_ingest returns.
   */
  swapTmpId: (
    conversationId: number,
    tmpId: string,
    realId: number,
    extra?: Partial<UIMessage>,
  ) => void;

  /** Remove a message (used for AbortReason.cancelled on empty). */
  removeMessage: (conversationId: number, id: MessageId) => void;

  /** Bulk-hydrate messages for a conversation (from DB fetch). */
  hydrateConversation: (conversationId: number, messages: UIMessage[]) => void;

  /**
   * Get the messages array for a conversation, or empty. Selector
   * form; components subscribe via `useChatStore((s) => s.messagesFor(id))`.
   */
  messagesFor: (conversationId: number) => UIMessage[];

  /** Reset entire store — used on sign-out. */
  clear: () => void;
}

/**
 * Immer-lite mutator. We could bring immer in as a dep but for this
 * shape the manual patches are lean enough. Each action returns a
 * new state object so Zustand's diff triggers subscribers.
 */
export const useChatStore = create<ChatState>()(
  subscribeWithSelector((set, get) => ({
    messagesMap: {},

    createMessage: (m) =>
      set((s) => {
        const list = s.messagesMap[m.conversationId] ?? [];
        // If a message with this id already exists, replace (idempotent).
        const idx = list.findIndex((x) => x.id === m.id);
        const next = idx === -1 ? [...list, m] : list.map((x, i) => (i === idx ? m : x));
        return {
          messagesMap: { ...s.messagesMap, [m.conversationId]: next },
        };
      }),

    updateMessage: (conversationId, id, patch) =>
      set((s) => {
        const list = s.messagesMap[conversationId];
        if (!list) return {};
        const idx = list.findIndex((x) => x.id === id);
        if (idx === -1) return {};
        const next = list.map((x, i) => (i === idx ? { ...x, ...patch } : x));
        return {
          messagesMap: { ...s.messagesMap, [conversationId]: next },
        };
      }),

    appendContent: (conversationId, id, delta) =>
      set((s) => {
        const list = s.messagesMap[conversationId];
        if (!list) return {};
        const idx = list.findIndex((x) => x.id === id);
        if (idx === -1) return {};
        const next = list.map((x, i) =>
          i === idx ? { ...x, content: x.content + delta } : x,
        );
        return {
          messagesMap: { ...s.messagesMap, [conversationId]: next },
        };
      }),

    appendReasoning: (conversationId, id, delta) =>
      set((s) => {
        const list = s.messagesMap[conversationId];
        if (!list) return {};
        const idx = list.findIndex((x) => x.id === id);
        if (idx === -1) return {};
        const next = list.map((x, i) =>
          i === idx ? { ...x, reasoning: (x.reasoning ?? "") + delta } : x,
        );
        return {
          messagesMap: { ...s.messagesMap, [conversationId]: next },
        };
      }),

    addToolCall: (conversationId, id, tc) =>
      set((s) => {
        const list = s.messagesMap[conversationId];
        if (!list) return {};
        const idx = list.findIndex((x) => x.id === id);
        if (idx === -1) return {};
        const cur = list[idx].toolCalls ?? [];
        // Dedup by tool call id.
        if (cur.some((c) => c.id === tc.id)) return {};
        const next = list.map((x, i) =>
          i === idx ? { ...x, toolCalls: [...cur, tc] } : x,
        );
        return {
          messagesMap: { ...s.messagesMap, [conversationId]: next },
        };
      }),

    swapTmpId: (conversationId, tmpId, realId, extra) =>
      set((s) => {
        const list = s.messagesMap[conversationId];
        if (!list) return {};
        const idx = list.findIndex((x) => x.tmpId === tmpId || x.id === tmpId);
        if (idx === -1) return {};
        const next = list.map((x, i) =>
          i === idx
            ? { ...x, id: realId, tmpId: undefined, ...(extra ?? {}) }
            : x,
        );
        return {
          messagesMap: { ...s.messagesMap, [conversationId]: next },
        };
      }),

    removeMessage: (conversationId, id) =>
      set((s) => {
        const list = s.messagesMap[conversationId];
        if (!list) return {};
        const next = list.filter((x) => x.id !== id);
        return {
          messagesMap: { ...s.messagesMap, [conversationId]: next },
        };
      }),

    hydrateConversation: (conversationId, messages) =>
      set((s) => ({
        messagesMap: { ...s.messagesMap, [conversationId]: messages },
      })),

    messagesFor: (conversationId) => get().messagesMap[conversationId] ?? [],

    clear: () => set({ messagesMap: {} }),
  })),
);

/**
 * Generate a temp id for optimistic messages. Kept short so it's
 * distinguishable from numeric DB ids at a glance in devtools.
 */
export function tmpMessageId(): string {
  return `tmp_${Math.random().toString(36).slice(2, 10)}`;
}

/**
 * Type guard for "this message has a real DB id" (numeric). Used by
 * the renderer to decide which action-strip actions are available
 * (Regenerate only makes sense on a persisted row, etc.).
 */
export function isPersistedMessage(m: UIMessage): boolean {
  return typeof m.id === "number";
}
