import { create } from "zustand";

export type Activity = "idle" | "typing" | "thinking" | "listening" | "speaking";

export type AppStatus = {
  version: string;
  dbReady: boolean;
  onboarded: boolean;
};

import type { UserProfile } from "../lib/ipc";

type AppState = {
  activity: Activity;
  status: AppStatus | null;
  profile: UserProfile | null;
  showDiagnostics: boolean;
  setActivity: (a: Activity) => void;
  setStatus: (s: AppStatus) => void;
  setProfile: (p: UserProfile | null) => void;
  setShowDiagnostics: (v: boolean) => void;
  pulse: () => void;
};

let pulseTimer: ReturnType<typeof setTimeout> | null = null;

const DIAG_KEY = "travis.showDiagnostics";

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

export const useAppStore = create<AppState>((set, get) => ({
  activity: "idle",
  status: null,
  profile: null,
  showDiagnostics: readDiag(),
  setActivity: (activity) => set({ activity }),
  setStatus: (status) => set({ status }),
  setProfile: (profile) => set({ profile }),
  setShowDiagnostics: (v) => {
    writeDiag(v);
    set({ showDiagnostics: v });
  },
  pulse: () => {
    if (get().activity === "thinking" || get().activity === "listening") return;
    set({ activity: "typing" });
    if (pulseTimer) clearTimeout(pulseTimer);
    pulseTimer = setTimeout(() => set({ activity: "idle" }), 320);
  },
}));
