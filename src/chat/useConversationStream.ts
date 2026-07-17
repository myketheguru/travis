/**
 * useConversationStream — v0.28.70.
 *
 * Single event → chatStore bridge for the ENTIRE chat lifecycle.
 * Replaces useAssistantStream (v0.28.66) — this hook does its job
 * plus the user-message-inserted + assistant-message-created +
 * assistant-done side of the picture.
 *
 * Event → action mapping:
 *   journal://user-inserted           → createMessage (user role)
 *   voice://audio-ready               → updateMessage(audio) — attaches
 *                                        to the most recent optimistic user
 *                                        message OR queues for the next one
 *   journal://assistant-message-created → createMessage (assistant, streaming)
 *   journal://assistant-chunk          → appendContent
 *   journal://reasoning-chunk          → appendReasoning
 *   journal://assistant-tool-start     → addToolCall
 *   journal://assistant-done           → swapTmpId + updateMessage(streaming=false)
 *
 * Mount ONCE at the WorkspaceV2 root. Multiple mounts = double-write.
 */
import { useEffect } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useAppStore } from "../stores/app";
import { useChatStore, tmpMessageId } from "../stores/chatStore";
import { getSmoother } from "./smoothMessage";

interface UserInsertedEvent {
  conversationId: number;
  userMessageId: number;
  content: string;
}
interface AssistantCreatedEvent {
  conversationId: number;
  tmpId: string;
}
interface ChunkEvent {
  conversationId: number;
  tmpId: string;
  iter: number;
  delta: string;
}
interface ToolStartEvent {
  conversationId: number;
  tmpId: string;
  iter: number;
  toolCallId: string;
  toolName: string;
}
interface AssistantDoneEvent {
  conversationId: number;
  tmpId: string;
  assistantMessageId: number | null;
  content: string;
}
interface AudioReadyEvent {
  audioPath: string;
  durationMs: number;
}

export function useConversationStream(): void {
  useEffect(() => {
    const unlisteners: UnlistenFn[] = [];
    let mounted = true;
    const chat = useChatStore.getState;
    const app = useAppStore.getState;

    (async () => {
      // ---- user message ----
      unlisteners.push(
        await listen<UserInsertedEvent>("journal://user-inserted", (evt) => {
          if (!mounted) return;
          const { conversationId, userMessageId, content } = evt.payload;
          // Flip the active conversation if the journal opened a new one.
          if (app().activeConversationId !== conversationId) {
            app().setActiveConversationId(conversationId);
          }
          // Create-or-update: if an optimistic user message with this
          // content already exists (posted from Composer), swap its
          // tmpId to the real DB id. Otherwise create fresh.
          const list = chat().messagesFor(conversationId);
          const optimistic = list.find(
            (m) =>
              m.role === "user" &&
              typeof m.id === "string" &&
              m.content.trim() === content.trim(),
          );
          const audio = app().pendingVoiceAudio ?? undefined;
          if (optimistic) {
            chat().swapTmpId(
              conversationId,
              optimistic.id as string,
              userMessageId,
              audio ? { audio } : undefined,
            );
          } else {
            chat().createMessage({
              id: userMessageId,
              conversationId,
              role: "user",
              content,
              createdAt: new Date().toISOString(),
              audio,
            });
          }
        }),
      );

      // ---- voice audio ready (before whisper completes) ----
      unlisteners.push(
        await listen<AudioReadyEvent>("voice://audio-ready", (evt) => {
          if (!mounted) return;
          // Update pendingVoiceAudio so the composer optimistic path
          // picks it up. When the user message is created (via the
          // journal event above), the audio flows onto the message.
          const cur = app().pendingVoiceAudio;
          app().setPendingVoiceAudio({
            audioPath: evt.payload.audioPath,
            durationMs: evt.payload.durationMs,
            transcript: cur?.transcript ?? "",
          });
        }),
      );

      // ---- assistant message created (LLM turn start) ----
      unlisteners.push(
        await listen<AssistantCreatedEvent>(
          "journal://assistant-message-created",
          (evt) => {
            if (!mounted) return;
            const { conversationId, tmpId } = evt.payload;
            chat().createMessage({
              id: tmpId,
              tmpId,
              conversationId,
              role: "assistant",
              content: "",
              reasoning: "",
              toolCalls: [],
              streaming: true,
              createdAt: new Date().toISOString(),
            });
          },
        ),
      );

      // ---- assistant text delta (smooth-drained) ----
      // v0.28.71 — direct calls to appendContent make React re-render
      // per token which reads as "chunky" on longer responses. Route
      // through the smoothMessage RAF drain — adaptive char-per-frame
      // cadence keeps the queue responsive to bursty upstream chunks
      // (Claude sometimes dumps 100+ chars in one event) while
      // rendering as fluid typing.
      unlisteners.push(
        await listen<ChunkEvent>("journal://assistant-chunk", (evt) => {
          if (!mounted) return;
          const { conversationId, tmpId, delta } = evt.payload;
          getSmoother(conversationId, tmpId).push(delta);
        }),
      );

      // ---- reasoning delta ----
      unlisteners.push(
        await listen<ChunkEvent>("journal://reasoning-chunk", (evt) => {
          if (!mounted) return;
          const { conversationId, tmpId, delta } = evt.payload;
          chat().appendReasoning(conversationId, tmpId, delta);
        }),
      );

      // ---- tool call start ----
      unlisteners.push(
        await listen<ToolStartEvent>(
          "journal://assistant-tool-start",
          (evt) => {
            if (!mounted) return;
            const { conversationId, tmpId, toolCallId, toolName } = evt.payload;
            chat().addToolCall(conversationId, tmpId, {
              id: toolCallId,
              name: toolName,
            });
          },
        ),
      );

      // ---- assistant done ----
      unlisteners.push(
        await listen<AssistantDoneEvent>("journal://assistant-done", (evt) => {
          if (!mounted) return;
          const { conversationId, tmpId, assistantMessageId, content } =
            evt.payload;
          // Stop the smoother's RAF loop for this message.
          getSmoother(conversationId, tmpId).done();
          // v0.28.74 — preserve the LONGER of streamed vs done content.
          // Rust's `assistant_visible` comes from the extraction tool's
          // response field; if the LLM streamed prose + a big code
          // block but extraction returned a shorter synthesis, the
          // naive REPLACE would drop the code (users reported code
          // vanishing on second Bezier request). Only take the done
          // content if it's longer OR similar length — otherwise keep
          // what we accumulated.
          const list = chat().messagesFor(conversationId);
          const cur = list.find(
            (m) => m.tmpId === tmpId || m.id === tmpId,
          );
          const streamedLen = cur?.content.length ?? 0;
          const finalContent =
            content.length >= streamedLen - 40 ? content : cur?.content ?? content;
          if (assistantMessageId !== null && assistantMessageId !== undefined) {
            chat().swapTmpId(conversationId, tmpId, assistantMessageId, {
              content: finalContent,
              streaming: false,
            });
          } else {
            chat().updateMessage(conversationId, tmpId, {
              streaming: false,
              content: finalContent,
            });
          }
        }),
      );
    })().catch((err) => {
      console.warn("[conversation-stream] listener setup failed:", err);
    });

    return () => {
      mounted = false;
      unlisteners.forEach((u) => u());
    };
  }, []);
}

/**
 * Create an optimistic user message in the store and return its
 * tmpId. Callers (Composer) use this on submit and pass the text
 * along to journal_ingest — when the real DB row lands via
 * journal://user-inserted, the listener swaps tmpId → real id.
 */
export function insertOptimisticUserMessage(
  conversationId: number,
  content: string,
  audio?: { audioPath: string; durationMs: number; transcript: string },
): string {
  const tmpId = tmpMessageId();
  useChatStore.getState().createMessage({
    id: tmpId,
    tmpId,
    conversationId,
    role: "user",
    content,
    audio,
    createdAt: new Date().toISOString(),
  });
  return tmpId;
}
