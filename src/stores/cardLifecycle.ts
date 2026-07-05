/**
 * Card lifecycle store — Shell 5 substrate.
 *
 * Every card in the workspace has a 24-hour visibility window. Past
 * that, it archives (not deletes) — the user's canvas presents clean
 * the next day but the archived cards remain queryable for shape-
 * shifting resume ("bring back what I was doing on X").
 *
 * User can pin a card to never archive it. User can Clear all cards
 * at once to force-archive everything (explicit start-fresh gesture,
 * no confirm).
 *
 * Persisted to localStorage. Cards themselves stay in the existing
 * conversation_message DB; this store just tracks WORKSPACE lifecycle
 * over their ids.
 */
import { create } from "zustand";

const STORAGE_KEY = "travis.cardLifecycle";

/** How long a card stays visible before it archives (unless pinned or
 *  the user interacts with it). */
const VISIBILITY_WINDOW_MS = 24 * 60 * 60 * 1000;

interface Persisted {
  /** Card IDs the user has explicitly pinned. Never archive. */
  pinnedIds: string[];
  /** When the user last hit Clear. All cards created BEFORE this are
   *  considered archived even if they're within the 24h window. */
  clearedAt: string | null;
  /** Per-card timestamp of last interaction (click, expand, edit).
   *  Extends the 24h window from that point. Serialized as ISO strings.
   */
  lastInteractionAt: Record<string, string>;
  /** Shell 7 — cards the user asked Travis to bring back. These
   *  override archival (both 24h expiry AND clearedAt) so shape-shifting
   *  resume actually re-materializes the card in the canvas. Cleared
   *  when the user Clears again or explicitly dismisses. */
  resurrectedIds: string[];
}

interface CardLifecycleState extends Persisted {
  /** True if the card should render in the primary canvas right now. */
  isVisible: (cardId: string, createdAt: string | Date) => boolean;
  /** True if this card is pinned (never archives). */
  isPinned: (cardId: string) => boolean;
  /** True if the user asked Travis to bring this card back. Overrides
   *  archival. */
  isResurrected: (cardId: string) => boolean;
  /** True if visible_until has already elapsed (or the user Clear'd
   *  and the card was created before that) AND the card hasn't been
   *  resurrected. */
  isArchived: (cardId: string, createdAt: string | Date) => boolean;

  pin: (cardId: string) => void;
  unpin: (cardId: string) => void;
  /** Extend a card's visibility from now. Called on any user
   *  interaction (click, expand, reply, drag). */
  noteInteraction: (cardId: string) => void;
  /** Force-archive everything up to now. No confirm. */
  clearAll: () => void;
  /** Bring a specific card back to the canvas. Sets resurrected +
   *  notes interaction so the card also has fresh 24h from now. */
  resurrect: (cardId: string) => void;
  /** Bring multiple cards back at once (bulk shape-shift result). */
  resurrectMany: (cardIds: string[]) => void;
  /** Remove a card from the resurrected set (does not archive
   *  immediately — visibility falls back to age-based rules). */
  unresurrect: (cardId: string) => void;
}

const emptyPersisted: Persisted = {
  pinnedIds: [],
  clearedAt: null,
  lastInteractionAt: {},
  resurrectedIds: [],
};

function readPersisted(): Persisted {
  if (typeof localStorage === "undefined") return emptyPersisted;
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return emptyPersisted;
    const parsed = JSON.parse(raw) as Partial<Persisted>;
    return {
      pinnedIds: Array.isArray(parsed.pinnedIds) ? parsed.pinnedIds : [],
      clearedAt: typeof parsed.clearedAt === "string" ? parsed.clearedAt : null,
      lastInteractionAt:
        parsed.lastInteractionAt && typeof parsed.lastInteractionAt === "object"
          ? (parsed.lastInteractionAt as Record<string, string>)
          : {},
      resurrectedIds: Array.isArray(parsed.resurrectedIds) ? parsed.resurrectedIds : [],
    };
  } catch {
    return emptyPersisted;
  }
}

function writePersisted(p: Persisted): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(STORAGE_KEY, JSON.stringify(p));
  } catch {
    /* quota / private mode — ignore */
  }
}

function toMs(t: string | Date): number {
  return typeof t === "string" ? Date.parse(t) : t.getTime();
}

export const useCardLifecycle = create<CardLifecycleState>((set, get) => {
  const initial = readPersisted();
  return {
    ...initial,

    isPinned: (cardId) => get().pinnedIds.includes(cardId),

    isResurrected: (cardId) => get().resurrectedIds.includes(cardId),

    isVisible: (cardId, createdAt) => !get().isArchived(cardId, createdAt),

    isArchived: (cardId, createdAt) => {
      if (get().isPinned(cardId)) return false;
      // Shape-shifting resume: resurrected cards are always visible
      // regardless of clear/age rules.
      if (get().isResurrected(cardId)) return false;
      const created = toMs(createdAt);
      const cleared = get().clearedAt ? Date.parse(get().clearedAt!) : 0;
      if (created < cleared) return true;
      const lastInteract = get().lastInteractionAt[cardId];
      const anchor = lastInteract ? Math.max(created, Date.parse(lastInteract)) : created;
      return Date.now() - anchor > VISIBILITY_WINDOW_MS;
    },

    pin: (cardId) => {
      const cur = get().pinnedIds;
      if (cur.includes(cardId)) return;
      const next = [...cur, cardId];
      set({ pinnedIds: next });
      writePersisted({
        pinnedIds: next,
        clearedAt: get().clearedAt,
        lastInteractionAt: get().lastInteractionAt,
        resurrectedIds: get().resurrectedIds,
      });
    },

    unpin: (cardId) => {
      const cur = get().pinnedIds;
      if (!cur.includes(cardId)) return;
      const next = cur.filter((id) => id !== cardId);
      set({ pinnedIds: next });
      writePersisted({
        pinnedIds: next,
        clearedAt: get().clearedAt,
        lastInteractionAt: get().lastInteractionAt,
        resurrectedIds: get().resurrectedIds,
      });
    },

    noteInteraction: (cardId) => {
      const now = new Date().toISOString();
      const next = { ...get().lastInteractionAt, [cardId]: now };
      set({ lastInteractionAt: next });
      writePersisted({
        pinnedIds: get().pinnedIds,
        clearedAt: get().clearedAt,
        lastInteractionAt: next,
        resurrectedIds: get().resurrectedIds,
      });
    },

    clearAll: () => {
      const now = new Date().toISOString();
      // Clearing also drops the current resurrected set — user is
      // starting fresh, not preserving old shape-shifts.
      set({ clearedAt: now, resurrectedIds: [] });
      writePersisted({
        pinnedIds: get().pinnedIds,
        clearedAt: now,
        lastInteractionAt: get().lastInteractionAt,
        resurrectedIds: [],
      });
    },

    resurrect: (cardId) => {
      const cur = get().resurrectedIds;
      if (cur.includes(cardId)) return;
      const nextIds = [...cur, cardId];
      const now = new Date().toISOString();
      const nextInteract = { ...get().lastInteractionAt, [cardId]: now };
      set({ resurrectedIds: nextIds, lastInteractionAt: nextInteract });
      writePersisted({
        pinnedIds: get().pinnedIds,
        clearedAt: get().clearedAt,
        lastInteractionAt: nextInteract,
        resurrectedIds: nextIds,
      });
    },

    resurrectMany: (cardIds) => {
      if (cardIds.length === 0) return;
      const curIds = new Set(get().resurrectedIds);
      const now = new Date().toISOString();
      const nextInteract = { ...get().lastInteractionAt };
      let changed = false;
      for (const id of cardIds) {
        if (!curIds.has(id)) {
          curIds.add(id);
          changed = true;
        }
        nextInteract[id] = now;
      }
      if (!changed && cardIds.every((id) => nextInteract[id] === now)) return;
      const nextIds = Array.from(curIds);
      set({ resurrectedIds: nextIds, lastInteractionAt: nextInteract });
      writePersisted({
        pinnedIds: get().pinnedIds,
        clearedAt: get().clearedAt,
        lastInteractionAt: nextInteract,
        resurrectedIds: nextIds,
      });
    },

    unresurrect: (cardId) => {
      const cur = get().resurrectedIds;
      if (!cur.includes(cardId)) return;
      const next = cur.filter((id) => id !== cardId);
      set({ resurrectedIds: next });
      writePersisted({
        pinnedIds: get().pinnedIds,
        clearedAt: get().clearedAt,
        lastInteractionAt: get().lastInteractionAt,
        resurrectedIds: next,
      });
    },
  };
});
