/**
 * smoothMessage — v0.28.71.
 *
 * Direct port of lobehub/lobe-chat's `createSmoothMessage`
 * (packages/fetch-sse/src/fetchSSE.ts:127-217). Adaptive
 * character-per-frame drain that makes streamed tokens read like
 * fluid typing regardless of upstream bursty chunk cadence.
 *
 * Usage:
 *   const smoother = createSmoothMessage(conversationId, messageId);
 *   smoother.push(delta);           // called per chunk event
 *   smoother.done();                 // called on assistant-done
 *
 * The smoother owns a queue of characters + a RAF loop that
 * drains at `currentSpeed` chars/second, where:
 *   currentSpeed += (max(startSpeed, queueLen) - currentSpeed)
 *                 * speedChangeRate
 *   speedChangeRate = |Δqueue| * 0.0008 + 0.005
 *
 * When the queue is deep (Claude just dumped 500 chars in one burst),
 * currentSpeed ramps up toward the queue depth. When the queue
 * drains, it decays back toward startSpeed for smooth even flow.
 */
import { useChatStore, type MessageId } from "../stores/chatStore";

const START_SPEED_CHARS_PER_SEC = 12;

interface Smoother {
  /** Push a chunk of characters into the smooth-drain queue. */
  push: (delta: string) => void;
  /**
   * Signal that no more chunks will arrive. The smoother finishes
   * draining and stops the RAF loop. Idempotent.
   */
  done: () => void;
}

const active = new Map<string, Smoother>();

function keyFor(conversationId: number, messageId: MessageId): string {
  return `${conversationId}:${String(messageId)}`;
}

export function getSmoother(
  conversationId: number,
  messageId: MessageId,
): Smoother {
  const k = keyFor(conversationId, messageId);
  const existing = active.get(k);
  if (existing) return existing;

  let queue: string[] = [];
  let currentSpeed = START_SPEED_CHARS_PER_SEC;
  let lastTs = 0;
  let rafHandle: number | null = null;
  let finished = false;

  const tick = (ts: number) => {
    rafHandle = null;
    if (lastTs === 0) lastTs = ts;
    const elapsedMs = ts - lastTs;
    lastTs = ts;

    const qLen = queue.length;
    if (qLen > 0) {
      // Adapt speed toward the queue depth.
      const target = Math.max(START_SPEED_CHARS_PER_SEC, qLen);
      const rate = Math.abs(target - currentSpeed) * 0.0008 + 0.005;
      currentSpeed += (target - currentSpeed) * rate;

      const drainCount = Math.max(
        1,
        Math.floor((elapsedMs * currentSpeed) / 1000),
      );
      const take = Math.min(drainCount, queue.length);
      if (take > 0) {
        const chunk = queue.splice(0, take).join("");
        useChatStore
          .getState()
          .appendContent(conversationId, messageId, chunk);
      }
    }

    if (queue.length > 0) {
      rafHandle = requestAnimationFrame(tick);
    } else if (finished) {
      // Nothing left AND upstream signalled done — stop.
      active.delete(k);
      return;
    } else {
      // Nothing left but upstream may send more — keep the loop
      // alive at 60fps so we're responsive.
      rafHandle = requestAnimationFrame(tick);
    }
  };

  const smoother: Smoother = {
    push: (delta: string) => {
      if (!delta) return;
      // Explode to individual characters so we drain a smooth
      // character-per-frame cadence, not chunk-boundary jumps.
      for (const ch of delta) queue.push(ch);
      if (rafHandle === null) {
        lastTs = 0;
        rafHandle = requestAnimationFrame(tick);
      }
    },
    done: () => {
      finished = true;
      // If the queue is empty at this moment, the loop cleanup path
      // will remove us from `active` on the next frame. If it's
      // still draining, the tick keeps going until it finishes.
      if (rafHandle === null && queue.length === 0) {
        active.delete(k);
      }
    },
  };
  active.set(k, smoother);
  return smoother;
}

/** Force-drain everything for a conversation. Used on Clear / sign-out. */
export function dropAllSmoothers(): void {
  active.clear();
}
