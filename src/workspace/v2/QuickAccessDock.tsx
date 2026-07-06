/**
 * QuickAccessDock — v0.27.2 — vertical left-mid rail with hover popovers.
 *
 * Previously bottom-right; hid behind the composer on small windows.
 * Moved to a vertical column on the left middle edge with popover
 * labels on hover so the icons alone stay compact.
 */
import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useAppStore } from "../../stores/app";

export function QuickAccessDock() {
  return (
    <div
      className="absolute left-3 top-1/2 -translate-y-1/2 z-20 pointer-events-auto flex flex-col gap-1.5"
      style={{
        // Nudge down slightly from ThreadRail so they don't overlap
        // visually when both are showing.
        transform: "translateY(calc(-50% + 128px))",
      }}
    >
      <DockRow
        label="History"
        shortcut="⌘K"
        icon={<ClockIcon />}
        useOpen={(s) => s.setHistoryOverlayOpen}
      />
      <DockRow
        label="Documents"
        shortcut="⌘D"
        icon={<DocIcon />}
        useOpen={(s) => s.setDocumentsOverlayOpen}
      />
      <DockRow
        label="Settings"
        shortcut="⌘,"
        icon={<GearIcon />}
        useOpen={(s) => s.setSettingsOverlayOpen}
      />
      <DockRow
        label="Classic view"
        icon={<SwapIcon />}
        useOpen={() => (open) => useAppStore.getState().setUiSurface(open ? "classic" : "v2")}
      />
    </div>
  );
}

function DockRow({
  label,
  shortcut,
  icon,
  useOpen,
}: {
  label: string;
  shortcut?: string;
  icon: React.ReactNode;
  useOpen: (
    s: ReturnType<typeof useAppStore.getState>,
  ) => (open: boolean) => void;
}) {
  const [hover, setHover] = useState(false);
  const open = useAppStore(useOpen);

  return (
    <div
      className="relative"
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
    >
      <motion.button
        whileHover={{ scale: 1.06 }}
        whileTap={{ scale: 0.94 }}
        onClick={() => open(true)}
        className="w-9 h-9 rounded-full flex items-center justify-center transition-colors backdrop-blur"
        style={{
          color: "rgba(236, 236, 241, 0.75)",
          background: "rgba(0, 0, 0, 0.35)",
          border: "1px solid rgba(255, 255, 255, 0.08)",
        }}
        title={shortcut ? `${label} (${shortcut})` : label}
        aria-label={label}
      >
        {icon}
      </motion.button>

      <AnimatePresence>
        {hover && (
          <motion.div
            initial={{ opacity: 0, x: -4 }}
            animate={{ opacity: 1, x: 0 }}
            exit={{ opacity: 0, x: -4 }}
            transition={{ duration: 0.16, ease: [0.22, 1, 0.36, 1] }}
            className="absolute left-11 top-1/2 -translate-y-1/2 whitespace-nowrap rounded-lg px-2.5 py-1 pointer-events-none"
            style={{
              background: "rgba(0, 0, 0, 0.65)",
              border: "1px solid rgba(255, 255, 255, 0.10)",
              backdropFilter: "blur(8px)",
              color: "rgba(236, 236, 241, 0.9)",
              fontSize: 11.5,
            }}
          >
            {label}
            {shortcut && (
              <span
                className="ml-2 font-mono"
                style={{ color: "rgba(236, 236, 241, 0.5)" }}
              >
                {shortcut}
              </span>
            )}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}

/* ─── Icons ─────────────────────────────────────────────────────── */

function ClockIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v5l3.5 2" />
    </svg>
  );
}

function DocIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" />
      <path d="M14 3v5h5" />
      <path d="M9 13h6M9 17h4" />
    </svg>
  );
}

function GearIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h.01a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v.01a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
    </svg>
  );
}

function SwapIcon() {
  return (
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
      <path d="M4 8h14M4 8l4-4M4 8l4 4" />
      <path d="M20 16H6M20 16l-4-4M20 16l-4 4" />
    </svg>
  );
}
