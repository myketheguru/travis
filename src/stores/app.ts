import { create } from "zustand";

export type Activity = "idle" | "typing" | "thinking" | "listening" | "speaking";

export type AppStatus = {
  version: string;
  dbReady: boolean;
  onboarded: boolean;
  /// Slugs of packs compiled into this build. Used by the Manage tab
  /// list to hide pack-supplied UI (e.g. Invoices when L2E is off).
  enabledPacks: string[];
};

import type { UserProfile } from "../lib/ipc";

type AppState = {
  activity: Activity;
  status: AppStatus | null;
  profile: UserProfile | null;
  showDiagnostics: boolean;
  activeConversationId: number | null;
  /// v0.20.1 — id of the document open in the split-view previewer.
  /// Null = previewer closed; Manage falls back to single-pane layout.
  viewerDocumentId: number | null;
  /// v0.20.1 — chat-pane fraction (0..1) when viewer is open. Persists
  /// in localStorage so the resize sticks across launches.
  chatPaneFraction: number;
  setActivity: (a: Activity) => void;
  setStatus: (s: AppStatus) => void;
  setProfile: (p: UserProfile | null) => void;
  setShowDiagnostics: (v: boolean) => void;
  setActiveConversationId: (id: number | null) => void;
  setViewerDocumentId: (id: number | null) => void;
  setChatPaneFraction: (f: number) => void;
  pulse: () => void;
};

let pulseTimer: ReturnType<typeof setTimeout> | null = null;

const DIAG_KEY = "travis.showDiagnostics";
const ACTIVE_CONV_KEY = "travis.activeConversationId";
const CHAT_PANE_FRACTION_KEY = "travis.chatPaneFraction";

const readDiag = (): boolean => {
  try {
    return typeof localStorage !== "undefined" && localStorage.getItem(DIAG_KEY) === "true";
  } catch {
    return false;
  }
};

const writeDiag = (v: boolean) => {
  try {
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(DIAG_KEY, String(v));
    }
  } catch {
    /* ignore */
  }
};

const readActiveConv = (): number | null => {
  try {
    if (typeof localStorage === "undefined") return null;
    const raw = localStorage.getItem(ACTIVE_CONV_KEY);
    if (!raw) return null;
    const n = Number.parseInt(raw, 10);
    return Number.isFinite(n) && n > 0 ? n : null;
  } catch {
    return null;
  }
};

const writeActiveConv = (id: number | null) => {
  try {
    if (typeof localStorage === "undefined") return;
    if (id == null) localStorage.removeItem(ACTIVE_CONV_KEY);
    else localStorage.setItem(ACTIVE_CONV_KEY, String(id));
  } catch {
    /* ignore */
  }
};

const readChatPaneFraction = (): number => {
  try {
    if (typeof localStorage === "undefined") return 0.5;
    const raw = localStorage.getItem(CHAT_PANE_FRACTION_KEY);
    if (!raw) return 0.5;
    const n = Number.parseFloat(raw);
    return Number.isFinite(n) && n > 0.15 && n < 0.85 ? n : 0.5;
  } catch {
    return 0.5;
  }
};

const writeChatPaneFraction = (f: number) => {
  try {
    if (typeof localStorage === "undefined") return;
    localStorage.setItem(CHAT_PANE_FRACTION_KEY, String(f));
  } catch {
    /* ignore */
  }
};

export const useAppStore = create<AppState>((set, get) => ({
  activity: "idle",
  status: null,
  profile: null,
  showDiagnostics: readDiag(),
  activeConversationId: readActiveConv(),
  viewerDocumentId: null,
  chatPaneFraction: readChatPaneFraction(),
  setActivity: (activity) => set({ activity }),
  setStatus: (status) => set({ status }),
  setProfile: (profile) => set({ profile }),
  setShowDiagnostics: (v) => {
    writeDiag(v);
    set({ showDiagnostics: v });
  },
  setActiveConversationId: (id) => {
    writeActiveConv(id);
    set({ activeConversationId: id });
  },
  setViewerDocumentId: (id) => set({ viewerDocumentId: id }),
  setChatPaneFraction: (f) => {
    const clamped = Math.max(0.15, Math.min(0.85, f));
    writeChatPaneFraction(clamped);
    set({ chatPaneFraction: clamped });
  },
  pulse: () => {
    if (get().activity === "thinking" || get().activity === "listening") return;
    set({ activity: "typing" });
    if (pulseTimer) clearTimeout(pulseTimer);
    pulseTimer = setTimeout(() => set({ activity: "idle" }), 320);
  },
}));
