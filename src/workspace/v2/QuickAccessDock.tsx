/**
 * QuickAccessDock — v0.27.2 vertical left-mid rail with hover popovers.
 * v0.27.3 hotfix: no closure-capture selectors — those triggered an
 * infinite re-render loop on mount and blanked the app after onboarding.
 */
import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import { useAppStore } from "../../stores/app";

export function QuickAccessDock() {
  // All setters are pulled once via stable store selectors. No closures
  // constructed during render — that was the v0.27.2 blank-screen bug.
  const setHistoryOverlayOpen = useAppStore((s) => s.setHistoryOverlayOpen);
  const setDocumentsOverlayOpen = useAppStore((s) => s.setDocumentsOverlayOpen);
  const setSettingsOverlayOpen = useAppStore((s) => s.setSettingsOverlayOpen);
  const setUiSurface = useAppStore((s) => s.setUiSurface);

  return (
    <div
      className="absolute left-3 top-1/2 z-20 pointer-events-auto flex flex-col gap-1.5"
      style={{
        // Nudge down slightly from the thread rail so they don't
        // overlap visually when both are showing.
        transform: "translateY(calc(-50% + 128px))",
      }}
    >
      <DockRow
        label="History"
        shortcut="⌘K"
        icon={<ClockIcon />}
        onClick={() => setHistoryOverlayOpen(true)}
      />
      <DockRow
        label="Documents"
        shortcut="⌘D"
        icon={<DocIcon />}
        onClick={() => setDocumentsOverlayOpen(true)}
      />
      <DockRow
        label="Settings"
        shortcut="⌘,"
        icon={<GearIcon />}
        onClick={() => setSettingsOverlayOpen(true)}
      />
      <DockRow
        label="Classic view"
        icon={<SwapIcon />}
        onClick={() => setUiSurface("classic")}
      />
    </div>
  );
}

function DockRow({
  label,
  shortcut,
  icon,
  onClick,
}: {
  label: string;
  shortcut?: string;
  icon: React.ReactNode;
  onClick: () => void;
}) {
  const [hover, setHover] = useState(false);

  return (
    <div
      className="relative"
      onMouseEnter={() => setHover(true)}
      onMouseLeave={() => setHover(false)}
    >
      <motion.button
        whileHover={{ scale: 1.06 }}
        whileTap={{ scale: 0.94 }}
        onClick={onClick}
        className="w-9 h-9 rounded-full flex items-center justify-center transition-colors backdrop-blur"
        style={{
          // v0.28.5 — denser bg so dock reads on light canvases (map).
          color: "rgba(236, 236, 241, 0.85)",
          background: "rgba(0, 0, 0, 0.68)",
          border: "1px solid rgba(255, 255, 255, 0.18)",
          boxShadow: "0 4px 16px -8px rgba(0, 0, 0, 0.6)",
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
