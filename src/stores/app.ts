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
  /// v0.25 (v2 Shell 6) — whether the Settings overlay is open on top
  /// of the current surface. Opened by ⌘, / Ctrl+, or by clicking the
  /// orb; closed by Esc. Not persisted — session-local.
  settingsOverlayOpen: boolean;
  /// v0.26 (v2 Shell 10) — history overlay (conversation switcher).
  /// Opened by ⌘K; closed by Esc or click outside. Session-local.
  historyOverlayOpen: boolean;
  /// v0.26 (v2 Shell 12b) — documents overlay. Opened via the dock or
  /// ⌘D. Session-local.
  documentsOverlayOpen: boolean;
  /// v0.26 (v2 Shell 8) — true when the immersive canvas should show the
  /// opening greeting. Computed at mount from lastActivityAt: true on
  /// cold boot OR when idle >= 24h. Fades to false on first keystroke.
  isFirstMoment: boolean;
  /// v0.26 (v2 Shell 11b) — instantaneous speech energy (0..1), used by
  /// the speech-scene spheroid to scale + intensify. Written by
  /// VoiceInputButton during STT capture (RMS of current samples) and
  /// by voice.speak during TTS playback (word-boundary envelope).
  /// Decays to 0 when nothing is writing.
  speechAmplitude: number;
  /// v0.27 (v2 Shell 13) — canvas mode selector. Drives which surface
  /// takes over the immersive canvas:
  ///   chat  — focus-shifting message stream (default when there's
  ///           any conversation)
  ///   voice — spheroid center + Listening/Speaking caption
  ///           (auto when activity is listening/speaking)
  ///   map   — full-bleed animated map (auto when the latest response
  ///           is a map part)
  ///   idle  — splash-style greeting (cold boot / 10min inactivity)
  /// The value is derived reactively in useCanvasMode(); this store
  /// field is the last computed result so components can subscribe.
  canvasMode: "idle" | "chat" | "voice" | "map";
  /// v0.27 (v2 Shell 14) — composer submit bridge. When set, the
  /// hidden AskTab picks it up, drops the text into its textarea, and
  /// fires submit(). Difference vs pendingComposerText: that just fills
  /// the input for the user to review; this triggers immediate send.
  pendingComposerSubmit: string | null;
  /// v0.28.2 — ambient listening mode. When true, the native mic
  /// pipeline transcribes ALL detected speech + saves it locally,
  /// but does NOT submit to the LLM unless the user explicitly
  /// arms (mic button, spacebar longpress) or a wake word triggers.
  /// Meant for capturing meetings, calls, or your own thinking
  /// so you can follow up with Travis later.
  ambientListening: boolean;
  /// v0.28.2 — captured ambient transcripts this session. Growing
  /// list; the user can review them from the canvas.
  ambientTranscripts: {
    id: string;
    text: string;
    occurredAt: string;
  }[];
  /// v0.28.5 — whether the map is currently expanded to fullscreen
  /// canvas. Auto-set true whenever a new map focal arrives, false
  /// when the user hits the collapse button on the map info card.
  /// When false, map parts render as inline MapCards in ChatCanvas
  /// which the user can click to re-expand.
  mapExpanded: boolean;
  /// The focal message id whose map is currently expanded. Prevents
  /// re-auto-expanding when the user has explicitly collapsed.
  mapExpandedFor: string | null;
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
  setSettingsOverlayOpen: (open: boolean) => void;
  setHistoryOverlayOpen: (open: boolean) => void;
  setDocumentsOverlayOpen: (open: boolean) => void;
  setSpeechAmplitude: (a: number) => void;
  setCanvasMode: (m: "idle" | "chat" | "voice" | "map") => void;
  setPendingComposerSubmit: (t: string | null) => void;
  setAmbientListening: (on: boolean) => void;
  appendAmbientTranscript: (text: string) => void;
  clearAmbientTranscripts: () => void;
  setMapExpanded: (expanded: boolean, forMessageId?: string) => void;
  /// Called on any real user activity — first keystroke, pill click,
  /// mic press, etc. Fades the opening greeting AND writes now to
  /// localStorage as lastActivityAt so the 24h idle rule can re-arm.
  noteUserActivity: () => void;
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
const LAST_ACTIVITY_KEY = "travis.lastActivityAt";
const IDLE_THRESHOLD_MS = 24 * 60 * 60 * 1000;

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

/// v0.26 (v2 Shell 8) — the immersive canvas shows the opening greeting
/// when this is the FIRST render of the current session AND either the
/// user has never opened the app OR the last recorded activity was
/// >= 24h ago. Called once at store-init.
const computeInitialFirstMoment = (): boolean => {
  try {
    if (typeof localStorage === "undefined") return true;
    const raw = localStorage.getItem(LAST_ACTIVITY_KEY);
    if (!raw) return true;
    const last = Number(raw);
    if (!Number.isFinite(last)) return true;
    return Date.now() - last >= IDLE_THRESHOLD_MS;
  } catch {
    return true;
  }
};

const stampActivityNow = () => {
  try {
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(LAST_ACTIVITY_KEY, String(Date.now()));
    }
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

// v0.28.1 — always start with a fresh chat on launch. Previously we
// restored activeConversationId from localStorage, which meant every
// restart continued the old conversation. Users wanted the app to
// feel clean-slate on open. Prior conversations are still reachable
// via the History overlay (⌘K).
try {
  if (typeof localStorage !== "undefined") {
    localStorage.removeItem(ACTIVE_CONV_KEY);
  }
} catch {
  /* private mode */
}

export const useAppStore = create<AppState>((set, get) => ({
  activity: "idle",
  status: null,
  profile: null,
  showDiagnostics: readDiag(),
  activeConversationId: null,
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
  settingsOverlayOpen: false,
  setSettingsOverlayOpen: (open) => set({ settingsOverlayOpen: open }),
  historyOverlayOpen: false,
  setHistoryOverlayOpen: (open) => set({ historyOverlayOpen: open }),
  documentsOverlayOpen: false,
  setDocumentsOverlayOpen: (open) => set({ documentsOverlayOpen: open }),
  speechAmplitude: 0,
  setSpeechAmplitude: (a) => set({ speechAmplitude: Math.max(0, Math.min(1, a)) }),
  canvasMode: "idle",
  setCanvasMode: (m) => set({ canvasMode: m }),
  pendingComposerSubmit: null,
  setPendingComposerSubmit: (t) => set({ pendingComposerSubmit: t }),
  ambientListening: (() => {
    try {
      if (typeof localStorage === "undefined") return false;
      return localStorage.getItem("travis.ambientListening") === "1";
    } catch {
      return false;
    }
  })(),
  ambientTranscripts: [],
  setAmbientListening: (on) => {
    try {
      if (typeof localStorage !== "undefined") {
        localStorage.setItem("travis.ambientListening", on ? "1" : "0");
      }
    } catch {
      /* private mode */
    }
    set({ ambientListening: on });
  },
  appendAmbientTranscript: (text) => {
    const trimmed = text.trim();
    if (!trimmed) return;
    const entry = {
      id: `amb_${Date.now()}_${Math.random().toString(36).slice(2, 6)}`,
      text: trimmed,
      occurredAt: new Date().toISOString(),
    };
    set((s) => ({
      ambientTranscripts: [...s.ambientTranscripts, entry].slice(-500),
    }));
  },
  clearAmbientTranscripts: () => set({ ambientTranscripts: [] }),
  mapExpanded: true,
  mapExpandedFor: null,
  setMapExpanded: (expanded, forMessageId) =>
    set({
      mapExpanded: expanded,
      mapExpandedFor: expanded ? forMessageId ?? null : null,
    }),
  isFirstMoment: computeInitialFirstMoment(),
  noteUserActivity: () => {
    stampActivityNow();
    // Only clear isFirstMoment if it was true — avoid pointless re-renders.
    if (get().isFirstMoment) set({ isFirstMoment: false });
  },
  pulse: () => {
    if (get().activity === "thinking" || get().activity === "listening") return;
    set({ activity: "typing" });
    if (pulseTimer) clearTimeout(pulseTimer);
    pulseTimer = setTimeout(() => set({ activity: "idle" }), 320);
  },
}));
