/**
 * useFocalContent — v2 Shell 3.
 *
 * Reads the active conversation and splits it into:
 *   - focal:    the most recent assistant response (or null on empty)
 *   - orbits:   the 4 preceding assistant responses
 *   - shelf:    everything before the orbits (visible on demand)
 *
 * Polls the same get_thread endpoint AskTab uses so the two surfaces
 * stay coherent (updates surface here whenever the classic view
 * refreshes too). Poll rather than sub-because the DB doesn't emit
 * change events today; 3-second interval keeps latency reasonable
 * without hammering.
 */
import { useEffect, useState } from "react";
import { getThread, type ConversationMessage } from "../../lib/conversation";
import { useAppStore } from "../../stores/app";

interface FocalContent {
  focal: ConversationMessage | null;
  orbits: ConversationMessage[];
  shelf: ConversationMessage[];
  totalMessages: number;
  /** v0.27.2 — full chronological message stream (user + assistant),
   *  for ChatCanvas which renders BOTH sides of the exchange. The
   *  focal/orbits/shelf split above is preserved for legacy consumers
   *  that only care about assistant responses. */
  allMessages: ConversationMessage[];
}

const POLL_MS = 1200;
const ORBIT_COUNT = 4;

export function useFocalContent(): FocalContent {
  const conversationId = useAppStore((s) => s.activeConversationId);
  const [messages, setMessages] = useState<ConversationMessage[]>([]);

  useEffect(() => {
    if (!conversationId) {
      setMessages([]);
      return;
    }
    let cancelled = false;
    async function tick() {
      try {
        const t = await getThread(conversationId!);
        if (!cancelled) setMessages(t.messages);
      } catch {
        // Silently skip a bad poll; next tick can recover.
      }
    }
    void tick();
    const handle = setInterval(tick, POLL_MS);
    return () => {
      cancelled = true;
      clearInterval(handle);
    };
  }, [conversationId]);

  const assistantMessages = messages.filter((m) => m.role === "assistant");
  const focal = assistantMessages[assistantMessages.length - 1] ?? null;
  const orbits = assistantMessages
    .slice(Math.max(0, assistantMessages.length - 1 - ORBIT_COUNT), Math.max(0, assistantMessages.length - 1))
    .reverse();
  const shelf = assistantMessages.slice(
    0,
    Math.max(0, assistantMessages.length - 1 - ORBIT_COUNT),
  );

  return {
    focal,
    orbits,
    shelf,
    totalMessages: assistantMessages.length,
    allMessages: messages,
  };
}
