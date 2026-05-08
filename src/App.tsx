import { useCallback, useEffect, useState } from "react";
import { motion } from "framer-motion";
import { listen } from "@tauri-apps/api/event";
import { PresenceOrb } from "./components/PresenceOrb";
import HealthBanner from "./components/HealthBanner";
import { WorkspaceSwitcher } from "./components/WorkspaceSwitcher";
import { useAppStore } from "./stores/app";
import { getAppStatus, getUserProfile } from "./lib/ipc";
import { dbStats, type DbStats } from "./lib/domain";
import { packAlerts, type AlertResult } from "./lib/packs";
import Onboarding from "./onboarding/Onboarding";
import Settings from "./settings/Settings";
import Manage from "./manage/Manage";

type View = "splash" | "settings" | "manage";

export default function App() {
  const status = useAppStore((s) => s.status);
  const profile = useAppStore((s) => s.profile);
  const setStatus = useAppStore((s) => s.setStatus);
  const setProfile = useAppStore((s) => s.setProfile);
  const pulse = useAppStore((s) => s.pulse);
  const [view, setView] = useState<View>("splash");

  const refresh = useCallback(async () => {
    try {
      const s = await getAppStatus();
      setStatus(s);
      if (s.onboarded) {
        const p = await getUserProfile();
        setProfile(p);
      }
    } catch {
      setStatus({ version: "?", dbReady: false, onboarded: false, enabledPacks: [] });
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

  if (view === "settings") {
    return (
      <>
        <HealthBanner />
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

  if (view === "manage") {
    return (
      <>
        <HealthBanner />
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
          onOpenManage={() => setView("manage")}
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
}: {
  status: { version: string; dbReady: boolean };
  name: string | null;
  onOpenSettings: () => void;
  onOpenManage: () => void;
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
