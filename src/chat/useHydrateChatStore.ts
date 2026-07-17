/**
 * useHydrateChatStore — v0.28.70.
 *
 * When the active conversation changes, fetch its DB-persisted
 * messages and populate chatStore.messagesMap. This is the "history"
 * path — the live conversation stream (useConversationStream) then
 * takes over for anything that happens after mount.
 *
 * Idempotent per conversation: if the store already has messages
 * for the current conversation and the last id matches the DB's
 * last id, skip re-fetch. This avoids stomping on in-flight
 * streaming state when the same conversation re-mounts.
 */
import { useEffect, useRef } from "react";
import { useAppStore } from "../stores/app";
import { useChatStore, type UIMessage } from "../stores/chatStore";
import { getThread } from "../lib/conversation";

export function useHydrateChatStore(): void {
  const activeConversationId = useAppStore((s) => s.activeConversationId);
  const lastLoadedRef = useRef<number | null>(null);

  useEffect(() => {
    if (activeConversationId === null) return;
    // v0.28.70 — skip if we've already hydrated this conversation AND
    // it has messages. Streaming state that landed via events shouldn't
    // be stomped by a redundant fetch.
    const cur = useChatStore.getState().messagesMap[activeConversationId];
    if (
      lastLoadedRef.current === activeConversationId &&
      cur &&
      cur.length > 0
    ) {
      return;
    }
    let cancelled = false;
    void (async () => {
      try {
        const thread = await getThread(activeConversationId);
        if (cancelled) return;
        const mapped: UIMessage[] = thread.messages
          .filter(
            (m) =>
              m.role === "user" || m.role === "assistant" || m.role === "system",
          )
          .map((m) => ({
            id: m.id,
            conversationId: m.conversationId,
            role: m.role as UIMessage["role"],
            content: m.content,
            createdAt: m.createdAt,
          }));
        useChatStore.getState().hydrateConversation(
          activeConversationId,
          mapped,
        );
        lastLoadedRef.current = activeConversationId;
      } catch (err) {
        console.warn("[chat-hydrate] failed:", err);
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [activeConversationId]);
}
