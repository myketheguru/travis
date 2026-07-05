import { useCallback, useEffect, useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { PresenceOrb } from "./components/PresenceOrb";
import HealthBanner from "./components/HealthBanner";
import ForceUpgradeGate from "./components/ForceUpgradeGate";
import { ResourceLoader } from "./components/ResourceLoader";
import { AmbientIndicator } from "./components/AmbientIndicator";
import { WorkspaceSwitcher } from "./components/WorkspaceSwitcher";
import { useAppStore } from "./stores/app";
import { getAppStatus, getUserProfile } from "./lib/ipc";
import { dbStats, type DbStats } from "./lib/domain";
import { packAlerts, type AlertResult } from "./lib/packs";
import Onboarding from "./onboarding/Onboarding";
import Settings from "./settings/Settings";
import Manage from "./manage/Manage";
import { WorkspaceV2 } from "./workspace/v2/WorkspaceV2";
import { SignIn } from "./components/SignIn";
import { checkForUpdate, installUpdate } from "./lib/updater";
import { MigrationPrompt } from "./components/MigrationPrompt";
import { WhileYouWereAway } from "./components/WhileYouWereAway";
import {
  cloudHasToken,
  cloudMigrationStatus,
  cloudStatus,
  type CloudUser,
} from "./lib/cloud";

type View = "splash" | "settings" | "manage" | "feed";

/// v0.22.15 (Shell 10) — cold-open persistence. When set, the user
/// has landed in the workspace at least once — subsequent launches
/// skip the splash and jump straight into Manage with the composer
/// focused. Set by App.tsx after a successful workspace entry.
const HAS_LANDED_KEY = "travis.hasLandedInWorkspace";

function readHasLanded(): boolean {
  try {
    return typeof localStorage !== "undefined" && localStorage.getItem(HAS_LANDED_KEY) === "true";
  } catch {
    return false;
  }
}

function markHasLanded(): void {
  try {
    if (typeof localStorage !== "undefined") localStorage.setItem(HAS_LANDED_KEY, "true");
  } catch {
    /* private mode / quota — non-fatal */
  }
}

// v2 Phase 1 — Tri-state cloud sign-in gate. Resolved at launch and
// after sign-in. Null while we're still checking; the empty render
// avoids a flash of the sign-in screen for already-signed-in users.
type CloudGate =
  | { kind: "checking" }
  | { kind: "signed_out" }
  | { kind: "signed_in"; user: CloudUser };

interface UpdateInfo {
  version: string;
  notes?: string | null;
  date?: string | null;
}

export default function App() {
  return (
    <ForceUpgradeGate>
      <AppInner />
      {/* Global overlay for resource bootstrap (lazy Python download +
          extract + wheel install). Stays hidden until a runtime-progress
          event fires; auto-dismisses on ready. */}
      <ResourceLoader />
      {/* Ambient wake-word listener + wake overlay. Renders nothing when
          ambient mode is off; when on, shows a small corner pill and a
          full overlay during the command capture window. Wake commands
          dispatch a window CustomEvent that AskTab listens for. */}
      <AmbientIndicator
        onCommand={(text) => {
          window.dispatchEvent(
            new CustomEvent<string>("travis-ambient-command", { detail: text }),
          );
        }}
      />
    </ForceUpgradeGate>
  );
}

function AppInner() {
  const status = useAppStore((s) => s.status);
  const profile = useAppStore((s) => s.profile);
  const uiSurface = useAppStore((s) => s.uiSurface);
  const setStatus = useAppStore((s) => s.setStatus);
  const setProfile = useAppStore((s) => s.setProfile);
  const pulse = useAppStore((s) => s.pulse);
  const [view, setView] = useState<View>("splash");
  const [pendingUpdate, setPendingUpdate] = useState<UpdateInfo | null>(null);
  const [updateDismissed, setUpdateDismissed] = useState<string | null>(null);
  const [updateInstalling, setUpdateInstalling] = useState(false);
  const [cloud, setCloud] = useState<CloudGate>({ kind: "checking" });
  // v2 Phase 2.1 — Migration prompt gate. Resolved after sign-in.
  // null = still checking; true = show prompt; false = skip prompt.
  const [needsMigration, setNeedsMigration] = useState<boolean | null>(null);

  // v2 Phase 2.1 — Check whether the migration prompt should fire.
  // Runs as soon as the user is signed in. Three statuses skip the prompt:
  //   complete / fresh / skipped. Empty status with any local data shows it.
  useEffect(() => {
    if (cloud.kind !== "signed_in") return;
    cloudMigrationStatus()
      .then((s) => {
        const decided =
          s.status === "complete" || s.status === "fresh" || s.status === "skipped";
        const totals =
          s.localCounts.profile +
          s.localCounts.memories +
          s.localCounts.conversations +
          s.localCounts.settings;
        setNeedsMigration(!decided && totals > 0);
      })
      .catch(() => {
        // If we can't read the status, don't block — let the user in.
        setNeedsMigration(false);
      });
  }, [cloud.kind]);

  // v2 Phase 1 — Check cloud sign-in status at launch. Fast-path:
  // cloudHasToken() avoids the network call when nothing is stored.
  // Slow path: cloudStatus() validates the token against the backend.
  // On 401 the backend has rejected the JWT; we treat that as signed-out.
  //
  // v0.21.8 — defensive retry on transient "state not managed" errors.
  // On older binaries that don't hide the window until setup completes
  // (everything <= v0.21.7), the WebView can race the Rust .manage()
  // call. New builds hide the window in setup() so this can't happen,
  // but we keep the retry so a corrupted install gives the user a
  // chance to recover gracefully.
  useEffect(() => {
    let cancelled = false;
    (async () => {
      const callWithRetry = async <T,>(fn: () => Promise<T>): Promise<T> => {
        let attempt = 0;
        while (true) {
          try {
            return await fn();
          } catch (e) {
            const msg = e instanceof Error ? e.message : String(e);
            if (!msg.toLowerCase().includes("state not managed") || attempt >= 30) {
              throw e;
            }
            attempt++;
            await new Promise((r) => setTimeout(r, 100));
          }
        }
      };
      try {
        const hasToken = await callWithRetry(() => cloudHasToken());
        if (!hasToken) {
          if (!cancelled) setCloud({ kind: "signed_out" });
          return;
        }
        const status = await callWithRetry(() => cloudStatus());
        if (cancelled) return;
        if (status.signedIn && status.user) {
          setCloud({ kind: "signed_in", user: status.user });
        } else {
          setCloud({ kind: "signed_out" });
        }
      } catch (e) {
        // Cloud unreachable. Treat as signed out for now so the user
        // can retry; the SignIn screen surfaces network errors clearly.
        console.error("cloud status check failed", e);
        if (!cancelled) setCloud({ kind: "signed_out" });
      }
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  const refresh = useCallback(async () => {
    // v0.20.10 — split the two calls so a getUserProfile() failure
    // doesn't fall through and falsely route the user to onboarding.
    // app_status decides onboarded; profile fetch is a separate
    // best-effort pass.
    //
    // v0.22.5 — added retry around getAppStatus and CRITICALLY changed
    // the failure mode. Previously a failed getAppStatus would set
    // {onboarded: false}, which routed the user through onboarding
    // even though they had already done it. On Windows this was
    // happening on every reboot — the WebView would race the Rust
    // AppState bringup on cold boots, getAppStatus would throw "state
    // not managed", we'd assume not-onboarded, and the user would see
    // the onboarding flow again.
    //
    // The right fix: on failure, leave status as null. The render
    // path below already shows a blank splash when status is null
    // (line `if (!status) return <main ... />`). Then we keep
    // retrying in the background. Onboarding only renders on a
    // confirmed onboarded=false from the backend.
    const callWithRetry = async <T,>(fn: () => Promise<T>): Promise<T> => {
      let attempt = 0;
      while (true) {
        try {
          return await fn();
        } catch (e) {
          const msg = e instanceof Error ? e.message : String(e);
          // Only retry on the known AppState race; surface other errors
          // immediately.
          if (!msg.toLowerCase().includes("state not managed") || attempt >= 30) {
            throw e;
          }
          attempt++;
          await new Promise((r) => setTimeout(r, 100));
        }
      }
    };

    let s;
    try {
      s = await callWithRetry(() => getAppStatus());
      setStatus(s);
    } catch (e) {
      // Real failure (not just the AppState race). Keep status null
      // so the splash stays up and we don't push the user into
      // onboarding by accident. Caller can call refresh() again.
      console.error("getAppStatus failed", e);
      return;
    }
    if (s.onboarded) {
      try {
        const p = await getUserProfile();
        setProfile(p);
      } catch (e) {
        // Don't reset onboarded — the user is onboarded per the
        // app_status probe; the profile fetch is just for prompt
        // templating. Failing it leaves the user at Splash with no
        // name, which is still better than re-onboarding.
        console.error("getUserProfile failed (non-fatal)", e);
      }

      // v0.22.15 (Shell 10) — cold-open flow. A returning onboarded
      // user who has previously landed in the workspace skips splash
      // + jumps straight into Manage with the composer focused. First
      // launch after onboarding still shows the splash so the user
      // sees the orb + gets oriented once.
      if (readHasLanded()) {
        setView((cur) => (cur === "splash" ? "manage" : cur));
      }
    }
  }, [setStatus, setProfile]);

  useEffect(() => {
    refresh();
  }, [refresh]);

  useEffect(() => {
    const onKey = () => pulse();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [pulse]);

  // v0.22.2 — listen for the travis://update event the Rust deep-link
  // handler dispatches. Triggers an update check + install via the
  // existing tauri-plugin-updater wrappers in lib/updater.
  useEffect(() => {
    async function onUpdate() {
      try {
        const info = await checkForUpdate();
        if (!info) {
          alert("You're already on the latest version of Travis.");
          return;
        }
        if (
          confirm(
            `Travis ${info.version} is available. Install it now? Travis will restart automatically when it's done.`,
          )
        ) {
          await installUpdate();
        }
      } catch (e) {
        console.error("update via deep-link failed", e);
        alert(
          "Couldn't check for updates. Try Settings → About to update manually.",
        );
      }
    }
    window.addEventListener("travis://update" as keyof WindowEventMap, onUpdate);
    return () =>
      window.removeEventListener("travis://update" as keyof WindowEventMap, onUpdate);
  }, []);

  // Listen for the background updater poll (v0.12.2+). When the
  // backend detects a newer release in the feed, it emits this event;
  // we surface a non-intrusive banner with an "Install" button.
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    listen<UpdateInfo>("update-available", (event) => {
      setPendingUpdate(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });
    return () => {
      if (unlisten) unlisten();
    };
  }, []);

  const handleInstallUpdate = useCallback(async () => {
    setUpdateInstalling(true);
    try {
      await invoke("install_update");
      // The backend restarts the app on success — code below this
      // line only runs if the install bailed before restart.
      setUpdateInstalling(false);
    } catch (e) {
      setUpdateInstalling(false);
      console.error("update install failed", e);
    }
  }, []);

  const updateBannerVisible =
    pendingUpdate !== null && updateDismissed !== pendingUpdate.version;

  // v2 Phase 1 — sign-in gate runs BEFORE the onboarding gate. A user
  // who isn't signed in can't proceed regardless of local state.
  if (cloud.kind === "checking") {
    return <main className="h-full w-full" />;
  }

  if (cloud.kind === "signed_out") {
    return (
      <motion.div
        key="signin"
        className="h-full w-full"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.45 }}
      >
        <SignIn
          onSignedIn={(user) => {
            setCloud({ kind: "signed_in", user });
            // Kick off a status refresh so the rest of the app picks
            // up any profile that may have been provisioned cloud-side.
            void refresh();
          }}
        />
      </motion.div>
    );
  }

  // v2 Phase 2.1 — Migration prompt sits between sign-in and onboarding.
  // Only triggered for signed-in users with existing local data who
  // haven't decided yet.
  if (needsMigration === null) {
    return <main className="h-full w-full" />;
  }
  if (needsMigration === true) {
    return (
      <motion.div
        key="migration"
        className="h-full w-full"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.45 }}
      >
        <MigrationPrompt onDone={() => setNeedsMigration(false)} />
      </motion.div>
    );
  }

  if (!status) {
    return <main className="h-full w-full" />;
  }

  if (!status.onboarded) {
    return (
      <motion.div
        key="onboarding"
        className="h-full w-full"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.45 }}
      >
        <Onboarding onDone={refresh} />
      </motion.div>
    );
  }

  const updateBanner = (
    <AnimatePresence>
      {updateBannerVisible && pendingUpdate && (
        <motion.div
          key={`update-${pendingUpdate.version}`}
          initial={{ opacity: 0, y: -16 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: -16 }}
          transition={{ duration: 0.28, ease: [0.16, 1, 0.3, 1] }}
          className="fixed top-3 left-1/2 -translate-x-1/2 z-50 flex items-center gap-3 px-4 py-2 rounded-full text-[12px]"
          style={{
            background: "rgba(124, 92, 255, 0.14)",
            border: "1px solid rgba(124, 92, 255, 0.32)",
            backdropFilter: "blur(20px)",
            WebkitBackdropFilter: "blur(20px)",
          }}
        >
          <span className="text-pulse">◆</span>
          <span className="text-bone">
            Travis v{pendingUpdate.version} is ready
          </span>
          <button
            onClick={handleInstallUpdate}
            disabled={updateInstalling}
            className="text-bone font-medium px-2.5 py-0.5 rounded-full bg-pulse/30 hover:bg-pulse/45 disabled:opacity-60 transition-colors"
          >
            {updateInstalling ? "installing…" : "install"}
          </button>
          <button
            onClick={() => setUpdateDismissed(pendingUpdate.version)}
            className="text-bone-3 hover:text-bone-2 px-1"
            title="Dismiss for this session"
          >
            ×
          </button>
        </motion.div>
      )}
    </AnimatePresence>
  );

  // v0.26 (v2 Shell 7) — immersive is the whole app. When uiSurface is
  // v2 (the default), skip Manage/Settings/Splash view routing entirely
  // and render WorkspaceV2 full-window. Settings + History are overlays
  // inside WorkspaceV2; no route change ever fires.
  if (uiSurface === "v2") {
    return (
      <>
        <HealthBanner />
        {updateBanner}
        <motion.div
          key="immersive"
          className="h-full w-full"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ duration: 0.4, ease: [0.22, 1, 0.36, 1] }}
        >
          <WorkspaceV2 />
        </motion.div>
      </>
    );
  }

  if (view === "settings") {
    return (
      <>
        <HealthBanner />
        {updateBanner}
        <motion.div
          key="settings"
          className="h-full w-full"
          initial={{ opacity: 0, x: 16 }}
          animate={{ opacity: 1, x: 0 }}
          transition={{ duration: 0.3, ease: [0.16, 1, 0.3, 1] }}
        >
          <Settings onClose={() => setView("splash")} />
        </motion.div>
      </>
    );
  }

  if (view === "feed") {
    return (
      <>
        <HealthBanner />
        {updateBanner}
        <motion.div
          key="feed"
          className="h-full w-full"
          initial={{ opacity: 0, x: 16 }}
          animate={{ opacity: 1, x: 0 }}
          transition={{ duration: 0.3, ease: [0.16, 1, 0.3, 1] }}
        >
          <WhileYouWereAway onClose={() => setView("splash")} />
        </motion.div>
      </>
    );
  }

  if (view === "manage") {
    return (
      <>
        <HealthBanner />
        {updateBanner}
        <motion.div
          key="manage"
          className="h-full w-full"
          initial={{ opacity: 0, x: 16 }}
          animate={{ opacity: 1, x: 0 }}
          transition={{ duration: 0.3, ease: [0.16, 1, 0.3, 1] }}
        >
          <Manage onClose={() => setView("splash")} />
        </motion.div>
      </>
    );
  }

  return (
    <>
      <HealthBanner />
      {updateBanner}
      <motion.div
        key="splash"
        className="h-full w-full"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ duration: 0.3 }}
      >
        <Splash
          status={status}
          name={profile?.name ?? null}
          onOpenSettings={() => setView("settings")}
          onOpenManage={() => {
            // v0.22.15 (Shell 10) — mark that the user has entered the
            // workspace so subsequent launches skip the splash.
            markHasLanded();
            setView("manage");
          }}
          onOpenFeed={() => setView("feed")}
        />
      </motion.div>
    </>
  );
}

function Splash({
  status,
  name,
  onOpenSettings,
  onOpenManage,
  onOpenFeed,
}: {
  status: { version: string; dbReady: boolean };
  name: string | null;
  onOpenSettings: () => void;
  onOpenManage: () => void;
  onOpenFeed: () => void;
}) {
  const first = name ? name.split(" ")[0] : null;
  const [stats, setStats] = useState<DbStats | null>(null);
  const [alerts, setAlerts] = useState<AlertResult[]>([]);

  const refreshStats = useCallback(() => {
    dbStats().then(setStats).catch(() => setStats(null));
    packAlerts().then(setAlerts).catch(() => setAlerts([]));
  }, []);

  useEffect(() => {
    refreshStats();
    let unlistenFn: (() => void) | null = null;
    listen<string>("domain-changed", () => {
      refreshStats();
    }).then((fn) => {
      unlistenFn = fn;
    });
    const onFocus = () => refreshStats();
    window.addEventListener("focus", onFocus);
    return () => {
      window.removeEventListener("focus", onFocus);
      if (unlistenFn) unlistenFn();
    };
  }, [refreshStats]);

  return (
    <main className="relative h-full w-full flex flex-col items-center overflow-hidden">
      <div className="absolute top-4 right-4 flex items-center gap-2">
        <WorkspaceSwitcher />
        <button
          onClick={onOpenFeed}
          title="While you were away"
          aria-label="While you were away"
          className="h-9 w-9 flex items-center justify-center rounded-full text-bone-3 hover:text-bone-2 hover:bg-white/[0.04] transition-colors"
        >
          <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
            <circle cx="12" cy="12" r="9" />
            <path d="M12 7v5l3 2" />
          </svg>
        </button>
        <button
          onClick={onOpenManage}
          title="Manage"
          aria-label="Manage"
          className="h-9 w-9 flex items-center justify-center rounded-full text-bone-3 hover:text-bone-2 hover:bg-white/[0.04] transition-colors"
        >
          <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
            <rect x="3" y="3" width="7" height="7" rx="1.5" />
            <rect x="14" y="3" width="7" height="7" rx="1.5" />
            <rect x="3" y="14" width="7" height="7" rx="1.5" />
            <rect x="14" y="14" width="7" height="7" rx="1.5" />
          </svg>
        </button>
        <button
          onClick={onOpenSettings}
          title="Settings"
          aria-label="Settings"
          className="h-9 w-9 flex items-center justify-center rounded-full text-bone-3 hover:text-bone-2 hover:bg-white/[0.04] transition-colors"
        >
          <svg viewBox="0 0 24 24" width="18" height="18" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
            <circle cx="12" cy="12" r="3" />
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1 0 2.83 2 2 0 0 1-2.83 0l-.06-.06A1.65 1.65 0 0 0 15 19.4a1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83 0 2 2 0 0 1 0-2.83l.06-.06A1.65 1.65 0 0 0 4.6 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 0-2.83 2 2 0 0 1 2.83 0l.06.06A1.65 1.65 0 0 0 9 4.6a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09A1.65 1.65 0 0 0 15 4.6a1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 0 2 2 0 0 1 0 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9c.14.34.22.7.22 1.07A1.65 1.65 0 0 0 21 11h.09A2 2 0 0 1 21 15h-.09a1.65 1.65 0 0 0-1.51 1z" />
          </svg>
        </button>
      </div>

      <motion.div
        className="mt-24"
        initial={{ opacity: 0, scale: 0.94 }}
        animate={{ opacity: 1, scale: 1 }}
        transition={{ duration: 0.9, ease: [0.16, 1, 0.3, 1] }}
      >
        <PresenceOrb size={220} />
      </motion.div>

      <motion.div
        className="mt-10 flex flex-col items-center gap-2"
        initial={{ opacity: 0, y: 12 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.7, ease: "easeOut", delay: 0.2 }}
      >
        <h1 className="text-5xl font-light tracking-[-0.04em] text-bone">
          {first ? `Hello, ${first}.` : "Travis"}
        </h1>
        <p className="text-bone-2 text-sm tracking-wide">
          {first ? "I'm here." : "Your operations layer."}
        </p>
      </motion.div>

      <motion.div
        className="absolute bottom-8 flex flex-col items-center gap-2 text-bone-3 text-xs"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        transition={{ delay: 0.6, duration: 0.6 }}
      >
        {alerts.length > 0 && (
          <div className="flex flex-col items-center gap-1 mb-2">
            {alerts.map((a) => (
              <span
                key={`${a.packSlug}.${a.alertSlug}`}
                className={
                  "text-[11px] tracking-wide " +
                  (a.severity === "money"
                    ? "text-warn"
                    : a.severity === "action"
                    ? "text-pulse-2"
                    : "text-bone-3")
                }
              >
                <span className="font-mono">{a.count}</span>{" "}
                {a.label.toLowerCase()}
              </span>
            ))}
          </div>
        )}
        {stats && (
          <div className="flex items-center gap-3 text-[11px] tracking-wide font-mono opacity-70 mb-1">
            <span><span className="text-bone-2">{stats.tasksOpen}</span> tasks</span>
            <span className="opacity-40">·</span>
            <span><span className="text-bone-2">{stats.invoices}</span> invoices</span>
            <span className="opacity-40">·</span>
            <span><span className="text-bone-2">{stats.coaches}</span> coaches</span>
            <span className="opacity-40">·</span>
            <span><span className="text-bone-2">{stats.schools}</span> schools</span>
          </div>
        )}
        <div className="flex items-center gap-1.5">
          <span>Press</span>
          <kbd className="px-1.5 py-0.5 rounded border border-ink-3 bg-ink-2/60 text-bone-2 font-mono text-[10px]">
            Ctrl
          </kbd>
          <span>+</span>
          <kbd className="px-1.5 py-0.5 rounded border border-ink-3 bg-ink-2/60 text-bone-2 font-mono text-[10px]">
            J
          </kbd>
          <span>anywhere</span>
        </div>
        <div className="flex items-center gap-2 mt-1">
          <span
            className={
              "h-1.5 w-1.5 rounded-full " +
              (status.dbReady ? "bg-pulse-2" : "bg-warn")
            }
          />
          <span className="font-mono">v{status.version}</span>
          <span className="opacity-60">·</span>
          <span>ready</span>
        </div>
      </motion.div>
    </main>
  );
}
