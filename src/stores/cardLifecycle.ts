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
}

interface CardLifecycleState extends Persisted {
  /** True if the card should render in the primary canvas right now. */
  isVisible: (cardId: string, createdAt: string | Date) => boolean;
  /** True if this card is pinned (never archives). */
  isPinned: (cardId: string) => boolean;
  /** True if visible_until has already elapsed (or the user Clear'd
   *  and the card was created before that). */
  isArchived: (cardId: string, createdAt: string | Date) => boolean;

  pin: (cardId: string) => void;
  unpin: (cardId: string) => void;
  /** Extend a card's visibility from now. Called on any user
   *  interaction (click, expand, reply, drag). */
  noteInteraction: (cardId: string) => void;
  /** Force-archive everything up to now. No confirm. */
  clearAll: () => void;
}

const emptyPersisted: Persisted = {
  pinnedIds: [],
  clearedAt: null,
  lastInteractionAt: {},
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

    isVisible: (cardId, createdAt) => !get().isArchived(cardId, createdAt),

    isArchived: (cardId, createdAt) => {
      if (get().isPinned(cardId)) return false;
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
      });
    },

    clearAll: () => {
      const now = new Date().toISOString();
      set({ clearedAt: now });
      writePersisted({
        pinnedIds: get().pinnedIds,
        clearedAt: now,
        lastInteractionAt: get().lastInteractionAt,
      });
    },
  };
});
