import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { motion } from "framer-motion";
import { checkForceUpgrade, type ForceUpgradeStatus } from "../lib/ipc";

/**
 * Hard gate shown when the running build is older than the
 * remotely-published minimum supported version. Non-dismissible —
 * the only escape hatches are "install update" or "quit".
 *
 * Network-fail is treated as "not required" so a transient outage
 * never blocks the user. The sentinel file
 * `min-supported.json` on the releases repo controls the threshold.
 */
export default function ForceUpgradeGate({ children }: { children: React.ReactNode }) {
  const [status, setStatus] = useState<ForceUpgradeStatus | null>(null);
  const [installing, setInstalling] = useState(false);
  const [installError, setInstallError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    checkForceUpgrade()
      .then((s) => {
        if (!cancelled) setStatus(s);
      })
      .catch(() => {
        if (!cancelled) {
          setStatus({
            required: false,
            currentVersion: "?",
            minVersion: null,
            latestVersion: null,
            reason: null,
          });
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const onInstall = async () => {
    setInstalling(true);
    setInstallError(null);
    try {
      await invoke("install_update");
    } catch (e) {
      setInstallError(e instanceof Error ? e.message : String(e));
      setInstalling(false);
    }
  };

  if (!status?.required) {
    return <>{children}</>;
  }

  return (
    <div className="fixed inset-0 z-[100] flex items-center justify-center bg-ink/95 backdrop-blur-xl">
      <motion.div
        initial={{ opacity: 0, y: 16 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.4, ease: [0.16, 1, 0.3, 1] }}
        className="max-w-md mx-6 rounded-2xl border border-warn/30 bg-ink-2/90 p-8 flex flex-col gap-5"
      >
        <div>
          <div className="text-warn text-[10px] tracking-[0.22em] uppercase mb-2">
            Update required
          </div>
          <h1 className="text-bone text-2xl font-light tracking-tight">
            Travis needs to update before you can continue.
          </h1>
        </div>

        <p className="text-bone-2 text-sm leading-relaxed">
          {status.reason ??
            "This version of Travis is no longer supported. Install the latest release to keep using it."}
        </p>

        <div className="text-bone-3 text-[11px] font-mono">
          <div>Your version: v{status.currentVersion}</div>
          {status.minVersion && <div>Required: v{status.minVersion} or newer</div>}
          {status.latestVersion && <div>Latest: v{status.latestVersion}</div>}
        </div>

        {installError && (
          <p className="text-warn text-[11px] leading-relaxed">
            Install failed: {installError}. You can also download the latest
            release manually from the Travis releases page.
          </p>
        )}

        <div className="flex items-center gap-3 pt-1">
          <button
            onClick={onInstall}
            disabled={installing}
            className="px-5 py-2.5 rounded-full bg-bone/95 text-ink text-sm font-medium hover:bg-bone disabled:opacity-40 transition-colors"
          >
            {installing ? "Installing…" : "Install update"}
          </button>
          <button
            onClick={() => invoke("quit_app").catch(() => window.close())}
            className="text-bone-3 hover:text-bone-2 text-xs"
          >
            Quit Travis
          </button>
        </div>
      </motion.div>
    </div>
  );
}
