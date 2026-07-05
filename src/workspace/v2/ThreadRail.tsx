/**
 * ThreadRail — v2 Shell 4.
 *
 * Vertical HUD element on the left edge showing active threads. Pinned
 * threads always show; the currently focused thread is highlighted;
 * click a chip to focus (Shell 4b will wire focal swap).
 *
 * Video-game reference: party panel / quest tracker on the side of the
 * screen. Peripheral awareness of parallel work without leaving the
 * main canvas.
 */
import { motion, AnimatePresence } from "framer-motion";
import { useAppStore } from "../../stores/app";
import { useCardLifecycle } from "../../stores/cardLifecycle";

export function ThreadRail() {
  const focusedThread = useAppStore((s) => s.focusedThread);
  const setFocusedThread = useAppStore((s) => s.setFocusedThread);
  const pinnedIds = useCardLifecycle((s) => s.pinnedIds);
  const resurrectedIds = useCardLifecycle((s) => s.resurrectedIds);

  // Sources for the rail:
  //   1. The currently focused thread (if any) — always visible
  //   2. All pinned threads (thread:*)
  //   3. Recently resurrected threads
  const threadTitles: string[] = [];
  if (focusedThread) threadTitles.push(focusedThread.title);
  for (const id of pinnedIds) {
    if (id.startsWith("thread:")) {
      const title = id.replace(/^thread:/, "");
      if (!threadTitles.includes(title)) threadTitles.push(title);
    }
  }
  for (const id of resurrectedIds) {
    if (id.startsWith("thread:")) {
      const title = id.replace(/^thread:/, "");
      if (!threadTitles.includes(title)) threadTitles.push(title);
    }
  }

  if (threadTitles.length === 0) return null;

  return (
    <div className="absolute left-3 top-1/2 -translate-y-1/2 z-20 pointer-events-auto">
      <div
        className="flex flex-col gap-1.5 rounded-2xl px-2 py-3"
        style={{
          background: "rgba(0, 0, 0, 0.25)",
          border: "1px solid rgba(255, 255, 255, 0.08)",
          backdropFilter: "blur(8px)",
          maxHeight: "70vh",
        }}
      >
        <div
          className="text-[8px] uppercase tracking-[0.24em] font-mono text-center px-1 py-0.5"
          style={{ color: "rgba(236, 236, 241, 0.35)" }}
        >
          Threads
        </div>
        <div className="flex flex-col gap-1 overflow-y-auto">
          <AnimatePresence initial={false}>
            {threadTitles.map((title, i) => {
              const isFocused =
                focusedThread !== null && focusedThread.title === title;
              return (
                <motion.button
                  key={title}
                  layout
                  initial={{ opacity: 0, x: -8 }}
                  animate={{ opacity: 1, x: 0 }}
                  exit={{ opacity: 0, x: -8 }}
                  transition={{
                    duration: 0.3,
                    ease: [0.22, 1, 0.36, 1],
                    delay: i * 0.03,
                  }}
                  whileHover={{ scale: 1.06 }}
                  whileTap={{ scale: 0.94 }}
                  onClick={() =>
                    setFocusedThread(
                      isFocused ? null : { id: null, title },
                    )
                  }
                  className="text-[10px] font-mono px-2.5 py-1.5 rounded-md whitespace-nowrap max-w-[180px] truncate"
                  style={{
                    background: isFocused
                      ? "rgba(124, 92, 255, 0.20)"
                      : "rgba(255, 255, 255, 0.03)",
                    color: isFocused
                      ? "rgb(189, 158, 255)"
                      : "rgba(236, 236, 241, 0.75)",
                    border: `1px solid ${
                      isFocused
                        ? "rgba(189, 158, 255, 0.55)"
                        : "rgba(255, 255, 255, 0.08)"
                    }`,
                    textAlign: "left",
                  }}
                  title={title}
                >
                  {isFocused && <span className="mr-1.5">●</span>}
                  {title}
                </motion.button>
              );
            })}
          </AnimatePresence>
        </div>
      </div>
    </div>
  );
}
