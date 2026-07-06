/**
 * QuickAccessDock — bottom-right HUD dock with visible overlay entries.
 *
 * The immersive canvas has no sidebar, so users need a discoverable way
 * to reach Settings, History, Documents, and the Interface toggle. This
 * is that lever — video-game action-bar style, corner-anchored, visible
 * at rest and hoverable for labels.
 *
 * Icons open their respective overlays or trigger inline actions. All
 * are also reachable via keyboard shortcuts, shown in tooltips.
 */
import { motion } from "framer-motion";
import { useAppStore } from "../../stores/app";

export function QuickAccessDock() {
  const setSettingsOverlayOpen = useAppStore((s) => s.setSettingsOverlayOpen);
  const setHistoryOverlayOpen = useAppStore((s) => s.setHistoryOverlayOpen);
  const setDocumentsOverlayOpen = useAppStore(
    (s) => s.setDocumentsOverlayOpen,
  );
  const setUiSurface = useAppStore((s) => s.setUiSurface);

  return (
    <div className="absolute bottom-4 right-4 z-20 pointer-events-auto">
      <div
        className="flex items-center gap-1 rounded-full px-2 py-1.5"
        style={{
          background: "rgba(0, 0, 0, 0.35)",
          border: "1px solid rgba(255, 255, 255, 0.08)",
          backdropFilter: "blur(8px)",
        }}
      >
        <DockButton
          label="History"
          shortcut="⌘K"
          onClick={() => setHistoryOverlayOpen(true)}
        >
          <ClockIcon />
        </DockButton>
        <DockButton
          label="Documents"
          onClick={() => setDocumentsOverlayOpen(true)}
        >
          <DocIcon />
        </DockButton>
        <DockButton
          label="Settings"
          shortcut="⌘,"
          onClick={() => setSettingsOverlayOpen(true)}
        >
          <GearIcon />
        </DockButton>
        <div
          className="mx-0.5 h-6 w-px"
          style={{ background: "rgba(255,255,255,0.08)" }}
        />
        <DockButton
          label="Classic view"
          onClick={() => setUiSurface("classic")}
        >
          <SwapIcon />
        </DockButton>
      </div>
    </div>
  );
}

function DockButton({
  label,
  shortcut,
  onClick,
  children,
}: {
  label: string;
  shortcut?: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <motion.button
      whileHover={{ scale: 1.08, backgroundColor: "rgba(255,255,255,0.06)" }}
      whileTap={{ scale: 0.94 }}
      onClick={onClick}
      className="relative w-9 h-9 rounded-full flex items-center justify-center transition-colors"
      style={{ color: "rgba(236, 236, 241, 0.75)" }}
      title={shortcut ? `${label} (${shortcut})` : label}
      aria-label={label}
    >
      {children}
    </motion.button>
  );
}

/* ─── Icons (inline SVG so no external deps) ────────────────────── */

function ClockIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <circle cx="12" cy="12" r="9" />
      <path d="M12 7v5l3.5 2" />
    </svg>
  );
}

function DocIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z" />
      <path d="M14 3v5h5" />
      <path d="M9 13h6M9 17h4" />
    </svg>
  );
}

function GearIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <circle cx="12" cy="12" r="3" />
      <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09a1.65 1.65 0 0 0-1-1.51 1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09a1.65 1.65 0 0 0 1.51-1 1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33h.01a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82v.01a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
    </svg>
  );
}

function SwapIcon() {
  return (
    <svg
      width="16"
      height="16"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden
    >
      <path d="M4 8h14M4 8l4-4M4 8l4 4" />
      <path d="M20 16H6M20 16l-4-4M20 16l-4 4" />
    </svg>
  );
}
