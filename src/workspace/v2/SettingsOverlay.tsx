/**
 * SettingsOverlay — v2 Shell 6.
 *
 * Mounts the existing Settings component inside a floating card
 * overlay on top of the workspace. Closes with Esc, click outside,
 * or the X button. Reuses ALL existing Settings sections — no
 * duplication. Users get one-click access via ⌘, (Ctrl+, on non-Mac)
 * or by clicking the orb (Shell 4b will wire that click).
 */
import { useEffect } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useAppStore } from "../../stores/app";
import Settings from "../../settings/Settings";

export function SettingsOverlay() {
  const open = useAppStore((s) => s.settingsOverlayOpen);
  const setOpen = useAppStore((s) => s.setSettingsOverlayOpen);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [open, setOpen]);

  return (
    <AnimatePresence>
      {open && (
        <motion.div
          key="settings-overlay-backdrop"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={{ duration: 0.28, ease: [0.22, 1, 0.36, 1] }}
          className="fixed inset-0 z-40 flex items-center justify-center"
          style={{
            background: "rgba(0, 0, 0, 0.55)",
            backdropFilter: "blur(4px)",
          }}
          onClick={() => setOpen(false)}
        >
          <motion.div
            key="settings-overlay-card"
            initial={{ opacity: 0, scale: 0.98, y: 8 }}
            animate={{ opacity: 1, scale: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.98, y: 8 }}
            transition={{ duration: 0.32, ease: [0.22, 1, 0.36, 1] }}
            className="relative rounded-2xl overflow-hidden shadow-2xl"
            style={{
              width: "min(880px, 92vw)",
              height: "min(760px, 88vh)",
              background: "rgb(12, 12, 16)",
              border: "1px solid rgba(255, 255, 255, 0.10)",
            }}
            onClick={(e) => e.stopPropagation()}
          >
            {/* Close button */}
            <button
              onClick={() => setOpen(false)}
              className="absolute top-3 right-3 z-10 w-8 h-8 rounded-full flex items-center justify-center transition-colors"
              style={{
                background: "rgba(255, 255, 255, 0.04)",
                border: "1px solid rgba(255, 255, 255, 0.08)",
                color: "rgba(236, 236, 241, 0.7)",
              }}
              title="Close (esc)"
            >
              ✕
            </button>
            <div className="h-full overflow-y-auto">
              <Settings onClose={() => setOpen(false)} />
            </div>
          </motion.div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}
