/**
 * CanvasBackdrop — v2 Shell 2.
 *
 * Full-bleed backdrop with ambient depth that reflects Travis's
 * current activity. Video-game world-map feel: quiet when idle,
 * subtly alive when something's running.
 *
 * Three layers stacked (bottom → top):
 *   1. Radial gradient wash — always on, gives depth
 *   2. Faint mesh grid — HUD scanline reference, static
 *   3. Flowing aurora lines — slow SVG path morphs; intensity keys
 *      off the app store's activity state (idle / typing / thinking /
 *      listening / speaking)
 *
 * Everything respects prefers-reduced-motion — the flowing layer
 * collapses to a static state when the user has motion turned down.
 */
import { useEffect, useState } from "react";
import { motion } from "framer-motion";
import { useAppStore } from "../../stores/app";

export function CanvasBackdrop() {
  const activity = useAppStore((s) => s.activity);
  const reducedMotion = usePrefersReducedMotion();

  const active = activity !== "idle";
  const activityHue = hueFor(activity);

  return (
    <div className="absolute inset-0 z-0 overflow-hidden pointer-events-none">
      {/* Layer 1 — radial gradient wash */}
      <div
        className="absolute inset-0"
        style={{
          background:
            "radial-gradient(circle at 20% 30%, rgba(124, 92, 255, 0.08), transparent 60%), " +
            "radial-gradient(circle at 80% 70%, rgba(110, 196, 232, 0.06), transparent 55%), " +
            "linear-gradient(180deg, rgba(255,255,255,0.01), transparent)",
        }}
      />

      {/* Layer 2 — mesh grid, static */}
      <svg
        className="absolute inset-0 w-full h-full"
        xmlns="http://www.w3.org/2000/svg"
        aria-hidden
      >
        <defs>
          <pattern
            id="mesh"
            x="0"
            y="0"
            width="80"
            height="80"
            patternUnits="userSpaceOnUse"
          >
            <path
              d="M 80 0 L 0 0 0 80"
              fill="none"
              stroke="rgba(255,255,255,0.02)"
              strokeWidth="1"
            />
          </pattern>
        </defs>
        <rect width="100%" height="100%" fill="url(#mesh)" />
      </svg>

      {/* Layer 3 — flowing aurora lines. Animate opacity + path when
          Travis is active; go quiet when idle. */}
      <motion.svg
        className="absolute inset-0 w-full h-full"
        viewBox="0 0 1000 700"
        preserveAspectRatio="none"
        xmlns="http://www.w3.org/2000/svg"
        animate={{ opacity: active ? 0.55 : 0.18 }}
        transition={{ duration: 1.2, ease: [0.22, 1, 0.36, 1] }}
        aria-hidden
      >
        <defs>
          <linearGradient id="auroraA" x1="0%" y1="0%" x2="100%" y2="0%">
            <stop offset="0%" stopColor={`hsla(${activityHue}, 80%, 70%, 0)`} />
            <stop offset="50%" stopColor={`hsla(${activityHue}, 80%, 70%, 0.35)`} />
            <stop offset="100%" stopColor={`hsla(${activityHue}, 80%, 70%, 0)`} />
          </linearGradient>
          <linearGradient id="auroraB" x1="0%" y1="0%" x2="100%" y2="0%">
            <stop
              offset="0%"
              stopColor={`hsla(${(activityHue + 40) % 360}, 70%, 65%, 0)`}
            />
            <stop
              offset="50%"
              stopColor={`hsla(${(activityHue + 40) % 360}, 70%, 65%, 0.28)`}
            />
            <stop
              offset="100%"
              stopColor={`hsla(${(activityHue + 40) % 360}, 70%, 65%, 0)`}
            />
          </linearGradient>
        </defs>

        {/* Two flowing aurora curves at different depths */}
        <AuroraCurve
          d0="M0,220 C250,180 500,320 750,200 C900,140 1000,240 1000,240 L1000,700 L0,700 Z"
          d1="M0,240 C250,300 500,180 750,280 C900,320 1000,220 1000,220 L1000,700 L0,700 Z"
          stroke="url(#auroraA)"
          strokeWidth={1.6}
          animate={active && !reducedMotion}
        />
        <AuroraCurve
          d0="M0,420 C250,380 500,500 750,420 C900,380 1000,440 1000,440"
          d1="M0,440 C250,500 500,380 750,500 C900,440 1000,400 1000,400"
          stroke="url(#auroraB)"
          strokeWidth={1.2}
          animate={active && !reducedMotion}
          delay={0.6}
        />
      </motion.svg>
    </div>
  );
}

/* ─── Sub components ────────────────────────────────────────────── */

function AuroraCurve({
  d0,
  d1,
  stroke,
  strokeWidth,
  animate,
  delay = 0,
}: {
  d0: string;
  d1: string;
  stroke: string;
  strokeWidth: number;
  animate: boolean;
  delay?: number;
}) {
  return (
    <motion.path
      fill="none"
      stroke={stroke}
      strokeWidth={strokeWidth}
      strokeLinecap="round"
      d={d0}
      animate={
        animate
          ? { d: [d0, d1, d0] }
          : { d: d0 }
      }
      transition={
        animate
          ? {
              duration: 14,
              ease: [0.4, 0.0, 0.6, 1],
              repeat: Infinity,
              delay,
            }
          : { duration: 1.6, ease: [0.22, 1, 0.36, 1] }
      }
    />
  );
}

/* ─── Helpers ───────────────────────────────────────────────────── */

function hueFor(activity: string): number {
  switch (activity) {
    case "thinking":
      return 260; // purple
    case "listening":
      return 200; // teal
    case "speaking":
      return 130; // green
    case "typing":
      return 30; // amber
    default:
      return 240; // subtle blue when idle
  }
}

function usePrefersReducedMotion(): boolean {
  const [reduced, setReduced] = useState(false);
  useEffect(() => {
    if (typeof window === "undefined" || !window.matchMedia) return;
    const mq = window.matchMedia("(prefers-reduced-motion: reduce)");
    setReduced(mq.matches);
    const handler = (e: MediaQueryListEvent) => setReduced(e.matches);
    mq.addEventListener("change", handler);
    return () => mq.removeEventListener("change", handler);
  }, []);
  return reduced;
}
