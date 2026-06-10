/**
 * v0.20.3 — Tier 3: floating chat overlay for doc-only mode.
 *
 * When the user clicks the "Hide chat" button in the DocumentViewer
 * header, the split-view chat pane collapses entirely and the doc
 * fills the content area. This component takes its place: a small
 * pill in the corner that, when clicked, expands to a draggable
 * floating panel hosting the full AskTab chat surface.
 *
 * Behaviour:
 *   - Default collapsed (pill bottom-right).
 *   - Expands to a 420×640 panel; draggable; constrained to viewport.
 *   - "−" collapses back to the pill.
 *   - "↗" exits doc-only mode (restores the split layout).
 *   - The pill pulses faintly when Travis is "thinking" so the user
 *     knows a reply is en route while the panel is collapsed.
 */
import { useState } from "react";
import { motion, AnimatePresence } from "framer-motion";
import AskTab from "../manage/tabs/AskTab";
import { useAppStore } from "../stores/app";

export function FloatingChatOverlay() {
  const [collapsed, setCollapsed] = useState(true);
  const activity = useAppStore((s) => s.activity);
  const setDocFullscreen = useAppStore((s) => s.setDocFullscreen);

  return (
    <AnimatePresence mode="wait">
      {collapsed ? (
        <motion.button
          key="pill"
          type="button"
          onClick={() => setCollapsed(false)}
          initial={{ opacity: 0, y: 8 }}
          animate={{ opacity: 1, y: 0 }}
          exit={{ opacity: 0, y: 8 }}
          transition={{ duration: 0.2 }}
          className={
            "fixed bottom-5 right-5 z-40 flex items-center gap-2 px-4 py-2.5 rounded-full " +
            "bg-ink-2/90 border border-white/[0.08] shadow-2xl backdrop-blur-xl " +
            "text-bone text-[12.5px] hover:bg-ink-2 transition-colors"
          }
        >
          <span
            className={
              "h-1.5 w-1.5 rounded-full " +
              (activity === "thinking" || activity === "speaking"
                ? "bg-pulse-2 shadow-[0_0_8px_rgba(110,196,232,0.7)] animate-pulse"
                : "bg-bone-3/60")
            }
          />
          <span>Chat with Travis</span>
        </motion.button>
      ) : (
        <motion.div
          key="panel"
          drag
          dragMomentum={false}
          dragConstraints={{ top: -200, left: -1200, right: 0, bottom: 0 }}
          initial={{ opacity: 0, scale: 0.95, y: 12 }}
          animate={{ opacity: 1, scale: 1, y: 0 }}
          exit={{ opacity: 0, scale: 0.95, y: 12 }}
          transition={{ duration: 0.22, ease: [0.16, 1, 0.3, 1] }}
          className={
            "fixed bottom-5 right-5 z-40 flex flex-col w-[420px] h-[640px] " +
            "rounded-2xl bg-ink-2/95 border border-white/[0.08] shadow-2xl backdrop-blur-xl " +
            "overflow-hidden"
          }
        >
          <header className="shrink-0 flex items-center justify-between gap-2 px-3 py-2 border-b border-white/[0.06] cursor-grab active:cursor-grabbing">
            <div className="flex items-center gap-2">
              <span
                className={
                  "h-1.5 w-1.5 rounded-full " +
                  (activity === "thinking" || activity === "speaking"
                    ? "bg-pulse-2 shadow-[0_0_8px_rgba(110,196,232,0.7)]"
                    : "bg-bone-3/60")
                }
              />
              <span className="text-bone text-[11.5px] tracking-wider">CHAT</span>
            </div>
            <div className="flex items-center gap-0.5">
              <OverlayBtn
                label="Restore split layout"
                onClick={() => setDocFullscreen(false)}
              >
                <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                  <rect x="3" y="3" width="18" height="18" rx="2" />
                  <line x1="12" y1="3" x2="12" y2="21" />
                </svg>
              </OverlayBtn>
              <OverlayBtn label="Collapse" onClick={() => setCollapsed(true)}>
                <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden>
                  <line x1="5" y1="12" x2="19" y2="12" />
                </svg>
              </OverlayBtn>
            </div>
          </header>

          {/*
            AskTab fills whatever container it's placed in. min-h-0 +
            overflow-hidden on the wrapper keep its internal flex layout
            from blowing out the floating panel's bounds.
          */}
          <div className="flex-1 min-h-0 overflow-hidden">
            <AskTab />
          </div>
        </motion.div>
      )}
    </AnimatePresence>
  );
}

function OverlayBtn({
  label,
  onClick,
  children,
}: {
  label: string;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      title={label}
      aria-label={label}
      className="shrink-0 text-bone-3 hover:text-bone-2 hover:bg-white/[0.06] rounded p-1 transition-colors"
    >
      {children}
    </button>
  );
}
