/**
 * useAssistantStream — v0.28.66.
 *
 * Subscribes to the journal://assistant-* Tauri events and mutates
 * the store's `streamingAssistant` slot as chunks arrive. Modeled on
 * lobehub/lobe-chat's `StreamingHandler` (packages/fetch-sse) — one
 * accumulator per in-flight turn, cleared on `assistant-done`.
 *
 * ChatCanvas reads `streamingAssistant` from the store to render a
 * live-updating bubble WITHOUT a per-render subscription to every
 * intermediate value — Zustand does the diff so we only re-render
 * when content actually changes.
 *
 * Mount ONCE at the WorkspaceV2 root. Multiple mounts would produce
 * duplicate deltas.
 */
import { useEffect } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useAppStore } from "../stores/app";

interface ChunkEvent {
  conversationId: number;
  iter: number;
  delta: string;
}
interface ToolStartEvent {
  conversationId: number;
  iter: number;
  toolCallId: string;
  toolName: string;
}
interface DoneEvent {
  conversationId: number;
  assistantMessageId: number | null;
  content: string;
}

export function useAssistantStream(): void {
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];
    let mounted = true;

    (async () => {
      const store = useAppStore.getState;

      unlisteners.push(
        await listen<ChunkEvent>("journal://assistant-chunk", (evt) => {
          if (!mounted) return;
          store().appendStreamingDelta(evt.payload.conversationId, evt.payload.delta);
        }),
      );

      unlisteners.push(
        await listen<ChunkEvent>("journal://reasoning-chunk", (evt) => {
          if (!mounted) return;
          store().appendStreamingReasoning(evt.payload.conversationId, evt.payload.delta);
        }),
      );

      unlisteners.push(
        await listen<ToolStartEvent>("journal://assistant-tool-start", (evt) => {
          if (!mounted) return;
          store().appendStreamingToolCall(
            evt.payload.conversationId,
            evt.payload.toolCallId,
            evt.payload.toolName,
          );
        }),
      );

      unlisteners.push(
        await listen<DoneEvent>("journal://assistant-done", (evt) => {
          if (!mounted) return;
          // Clear the streaming slot — the real DB message will
          // appear in the polled thread and take over rendering.
          // v0.28.66 also tolerates the case where the persisted
          // row hasn't shown up yet: ChatCanvas checks whether the
          // last assistant message content matches the streamed
          // content before dropping the live bubble.
          const cur = useAppStore.getState().streamingAssistant;
          if (cur && cur.conversationId === evt.payload.conversationId) {
            store().setStreamingAssistant(null);
          }
        }),
      );
    })().catch((err) => {
      console.warn("[assistant-stream] listener setup failed:", err);
    });

    return () => {
      mounted = false;
      unlisteners.forEach((u) => u());
    };
  }, []);
}
