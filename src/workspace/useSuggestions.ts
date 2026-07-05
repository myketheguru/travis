/**
 * useSuggestions — feeds the ambient SuggestionRail.
 *
 * Suggestions are ANTICIPATORY (vs the attention strip which is
 * REACTIVE to already-happened signals). Travis proposes 3-5 next
 * moves the user might want to take, based on:
 *  - Time of day (morning check-in, midday planning, end-of-day wrap)
 *  - Calendar (next event, prep time)
 *  - Recent patterns (things the user does every Monday)
 *
 * MVP ships the time-of-day layer; the calendar + pattern layers
 * plug in as their data becomes available (existing Google Calendar
 * integration is a starter source).
 *
 * Suggestions include a `prompt` field — clicking submits this text
 * into the command pill so the user gets exactly what they asked for
 * with one click.
 */
import { useEffect, useState } from "react";

export type SuggestionKind =
  | "check_in"
  | "plan"
  | "wrap"
  | "calendar"
  | "reminder"
  | "recent";

export interface Suggestion {
  id: string;
  kind: SuggestionKind;
  /** Chip label — short, imperative, first-person to Travis. */
  label: string;
  /** The text that gets submitted if the user clicks the chip. */
  prompt: string;
  /** Optional detail line for tooltip. */
  detail?: string;
}

const REFRESH_MS = 5 * 60_000; // 5 minutes

export function useSuggestions() {
  const [items, setItems] = useState<Suggestion[]>([]);

  useEffect(() => {
    function refresh() {
      setItems(computeSuggestions());
    }
    refresh();
    const handle = setInterval(refresh, REFRESH_MS);
    return () => clearInterval(handle);
  }, []);

  return items;
}

// ─── Suggestion sources ──────────────────────────────────────────

function computeSuggestions(): Suggestion[] {
  const now = new Date();
  const h = now.getHours();
  const dayOfWeek = now.getDay(); // 0 = Sunday

  const items: Suggestion[] = [];

  // Time-of-day layer.
  if (h >= 6 && h < 11) {
    items.push({
      id: "morning-inbox",
      kind: "check_in",
      label: "Check inbox",
      prompt: "What's important in my inbox this morning?",
      detail: "Travis will scan and highlight what matters.",
    });
    items.push({
      id: "morning-agenda",
      kind: "calendar",
      label: "Today's agenda",
      prompt: "Walk me through today's calendar.",
      detail: "Meetings, prep time, gaps.",
    });
  } else if (h >= 11 && h < 15) {
    items.push({
      id: "midday-focus",
      kind: "plan",
      label: "Refocus",
      prompt: "What's the most important thing to work on right now?",
      detail: "Travis surfaces the top-priority open item.",
    });
    items.push({
      id: "midday-next-meeting",
      kind: "calendar",
      label: "Next meeting",
      prompt: "What's my next meeting and what do I need to prep?",
    });
  } else if (h >= 15 && h < 19) {
    items.push({
      id: "afternoon-wrap",
      kind: "wrap",
      label: "Wrap for the day",
      prompt: "Help me wrap up — what did I not get to?",
      detail: "Roll unfinished items into tomorrow.",
    });
    items.push({
      id: "afternoon-plan-tomorrow",
      kind: "plan",
      label: "Plan tomorrow",
      prompt: "What should tomorrow morning look like?",
    });
  } else {
    // Evening / off-hours — quieter suggestion set.
    items.push({
      id: "evening-brief",
      kind: "wrap",
      label: "Brief for tomorrow",
      prompt: "Give me a short brief for tomorrow morning.",
      detail: "Meetings + top three priorities.",
    });
  }

  // Weekday-only: recurring pattern suggestion.
  if (dayOfWeek === 1 && h >= 6 && h < 12) {
    items.push({
      id: "monday-weekly",
      kind: "recent",
      label: "Weekly rundown",
      prompt: "What's on the docket for this week?",
      detail: "Themes, deadlines, standing meetings.",
    });
  }

  // Cap at 5.
  return items.slice(0, 5);
}
