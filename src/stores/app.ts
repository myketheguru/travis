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
  /// v0.20.3 — doc-only mode. When true, the chat pane is hidden and
  /// a floating overlay handles input. Persisted across launches.
  docFullscreen: boolean;
  /// v0.22.15 (Shell 4) — id of the thread card the user has focused.
  /// When set, the top-level composer routes its next turn INTO that
  /// thread instead of creating a new top-level message. Also drives
  /// the placeholder text ("Continue X…") + the "adding to X" chip.
  focusedThread: FocusedThread | null;
  /// v0.22.15 (Shell 9) — one-shot bridge for injecting text into the
  /// composer from outside the composer (e.g., SuggestionRail chip
  /// click). AskTab watches this; on change it fills its local input
  /// and clears the pending value.
  pendingComposerText: string | null;
  /// v0.25 (task 329) — which workspace surface to render. 'v2' is the
  /// canvas + HUD overlay design; 'classic' is the wrapped-Ask surface
  /// that shipped in v0.23-24. Persisted to localStorage. Users toggle
  /// via Settings; new users default to v2.
  uiSurface: "v2" | "classic";
  setActivity: (a: Activity) => void;
  setStatus: (s: AppStatus) => void;
  setProfile: (p: UserProfile | null) => void;
  setShowDiagnostics: (v: boolean) => void;
  setActiveConversationId: (id: number | null) => void;
  setViewerDocumentId: (id: number | null) => void;
  setChatPaneFraction: (f: number) => void;
  setDocFullscreen: (v: boolean) => void;
  setFocusedThread: (t: FocusedThread | null) => void;
  setPendingComposerText: (t: string | null) => void;
  setUiSurface: (s: "v2" | "classic") => void;
  pulse: () => void;
};

/// A thread the user has clicked into. `id` may be null for threads
/// the client synthesized before we have a durable id.
export type FocusedThread = { id: string | null; title: string };

let pulseTimer: ReturnType<typeof setTimeout> | null = null;

const DIAG_KEY = "travis.showDiagnostics";
const ACTIVE_CONV_KEY = "travis.activeConversationId";
const CHAT_PANE_FRACTION_KEY = "travis.chatPaneFraction";
const DOC_FULLSCREEN_KEY = "travis.docFullscreen";
const UI_SURFACE_KEY = "travis.uiSurface";

const readUiSurface = (): "v2" | "classic" => {
  try {
    if (typeof localStorage === "undefined") return "v2";
    const v = localStorage.getItem(UI_SURFACE_KEY);
    return v === "classic" ? "classic" : "v2";
  } catch {
    return "v2";
  }
};

const writeUiSurface = (s: "v2" | "classic") => {
  try {
    if (typeof localStorage !== "undefined") localStorage.setItem(UI_SURFACE_KEY, s);
  } catch {
    /* private mode */
  }
};

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

const readDocFullscreen = (): boolean => {
  try {
    return typeof localStorage !== "undefined" && localStorage.getItem(DOC_FULLSCREEN_KEY) === "true";
  } catch {
    return false;
  }
};

const writeDocFullscreen = (v: boolean) => {
  try {
    if (typeof localStorage === "undefined") return;
    localStorage.setItem(DOC_FULLSCREEN_KEY, String(v));
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
  docFullscreen: readDocFullscreen(),
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
  setDocFullscreen: (v) => {
    writeDocFullscreen(v);
    set({ docFullscreen: v });
  },
  focusedThread: null,
  setFocusedThread: (t) => set({ focusedThread: t }),
  pendingComposerText: null,
  setPendingComposerText: (t) => set({ pendingComposerText: t }),
  uiSurface: readUiSurface(),
  setUiSurface: (s) => {
    writeUiSurface(s);
    set({ uiSurface: s });
  },
  pulse: () => {
    if (get().activity === "thinking" || get().activity === "listening") return;
    set({ activity: "typing" });
    if (pulseTimer) clearTimeout(pulseTimer);
    pulseTimer = setTimeout(() => set({ activity: "idle" }), 320);
  },
}));
