/**
 * SpeechScene — v2 Shell 11.
 *
 * Full-bleed overlay that fades over the canvas when the user or
 * Travis is actively speaking. A large gradient spheroid drifts, folds,
 * and ripples at the center. Silvery-cool palette when the user is
 * speaking; warm bronze palette when Travis is. Both fade back to the
 * canvas the moment speech stops.
 *
 * Activity source: reads useAppStore.activity —
 *   'listening' -> user is speaking (silvery)
 *   'speaking'  -> travis is speaking (bronze)
 *
 * Amplitude reactivity in this slice is time-driven (slow sine + noise
 * envelope). Real amplitude wiring lands when we route audio energy
 * from VoiceInputButton + Piper TTS callbacks into the store.
 */
import { motion, AnimatePresence } from "framer-motion";
import { useAppStore } from "../../stores/app";

export function SpeechScene() {
  const activity = useAppStore((s) => s.activity);
  const amplitude = useAppStore((s) => s.speechAmplitude);
  const mode: SpeechMode | null =
    activity === "listening"
      ? "user"
      : activity === "speaking"
        ? "travis"
        : null;

  return (
    <AnimatePresence>
      {mode && <SpeechSceneImpl key={mode} mode={mode} amplitude={amplitude} />}
    </AnimatePresence>
  );
}

type SpeechMode = "user" | "travis";

function SpeechSceneImpl({
  mode,
  amplitude,
}: {
  mode: SpeechMode;
  amplitude: number;
}) {
  const palette = mode === "user" ? SILVER : BRONZE;

  return (
    <motion.div
      className="absolute inset-0 z-30 pointer-events-none flex items-center justify-center"
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={{ duration: 0.52, ease: [0.22, 1, 0.36, 1] }}
      style={{
        background:
          `radial-gradient(circle at 50% 55%, ${palette.wash}, transparent 65%)`,
      }}
    >
      <Spheroid palette={palette} amplitude={amplitude} />
    </motion.div>
  );
}

/* ─── Spheroid drawing ─────────────────────────────────────────── */

interface Palette {
  wash: string;
  coreA: string;
  coreB: string;
  rim: string;
  glow: string;
  ripple: string;
  fold: string;
}

const SILVER: Palette = {
  wash: "rgba(210, 220, 240, 0.10)",
  coreA: "rgba(245, 248, 255, 0.95)",
  coreB: "rgba(180, 200, 230, 0.65)",
  rim: "rgba(140, 165, 200, 0.55)",
  glow: "rgba(200, 220, 245, 0.5)",
  ripple: "rgba(220, 232, 250, 0.35)",
  fold: "rgba(180, 195, 220, 0.28)",
};

const BRONZE: Palette = {
  wash: "rgba(255, 195, 130, 0.10)",
  coreA: "rgba(255, 224, 178, 0.95)",
  coreB: "rgba(200, 130, 70, 0.72)",
  rim: "rgba(160, 95, 40, 0.55)",
  glow: "rgba(230, 170, 100, 0.5)",
  ripple: "rgba(240, 190, 140, 0.35)",
  fold: "rgba(200, 140, 80, 0.28)",
};

function Spheroid({
  palette,
  amplitude,
}: {
  palette: Palette;
  amplitude: number;
}) {
  // Map amplitude 0..1 -> scale bump 0..0.18 and glow multiplier 1..2.2.
  // Base scale sits at 1 so silence -> resting spheroid.
  const scaleBump = 1 + amplitude * 0.18;
  const glowMultiplier = 1 + amplitude * 1.2;
  return (
    <motion.svg
      viewBox="-200 -200 400 400"
      width="min(56vmin, 520px)"
      height="min(56vmin, 520px)"
      style={{
        filter: `drop-shadow(0 0 ${80 * glowMultiplier}px ${palette.glow}) drop-shadow(0 0 ${
          32 * glowMultiplier
        }px ${palette.glow})`,
      }}
      initial={{ scale: 0.92, opacity: 0 }}
      animate={{ scale: scaleBump, opacity: 1 }}
      exit={{ scale: 0.92, opacity: 0 }}
      transition={{
        scale: { duration: 0.12, ease: [0.22, 1, 0.36, 1] },
        opacity: { duration: 0.5, ease: [0.22, 1, 0.36, 1] },
      }}
      aria-hidden
    >
      <defs>
        <radialGradient id="sph-core" cx="42%" cy="35%" r="72%">
          <stop offset="0%" stopColor={palette.coreA} />
          <stop offset="45%" stopColor={palette.coreB} />
          <stop offset="100%" stopColor={palette.rim} stopOpacity="0" />
        </radialGradient>
        <radialGradient id="sph-fold" cx="55%" cy="65%" r="60%">
          <stop offset="0%" stopColor={palette.fold} stopOpacity="0" />
          <stop offset="60%" stopColor={palette.fold} />
          <stop offset="100%" stopColor={palette.fold} stopOpacity="0" />
        </radialGradient>
      </defs>

      {/* Base spheroid — deforms slowly on a rotation loop */}
      <motion.g
        animate={{ rotate: [0, 6, -4, 0] }}
        transition={{
          duration: 8,
          repeat: Infinity,
          ease: [0.42, 0, 0.58, 1],
        }}
      >
        {/* Rippling outer boundary — path animates between two nearly
             identical shapes, deforming subtly like a jellyfish. */}
        <motion.path
          fill="url(#sph-core)"
          animate={{
            d: [
              "M 140,0 C 140,80 78,148 0,148 C -85,148 -148,82 -148,-2 C -148,-84 -82,-148 4,-148 C 82,-148 140,-80 140,0 Z",
              "M 148,4 C 145,86 82,144 -2,146 C -88,148 -146,80 -148,-6 C -150,-88 -86,-146 0,-148 C 88,-150 148,-84 148,4 Z",
              "M 142,-4 C 144,82 78,150 -4,148 C -86,146 -150,80 -146,-6 C -142,-90 -80,-150 8,-146 C 86,-142 140,-80 142,-4 Z",
              "M 140,0 C 140,80 78,148 0,148 C -85,148 -148,82 -148,-2 C -148,-84 -82,-148 4,-148 C 82,-148 140,-80 140,0 Z",
            ],
          }}
          transition={{
            duration: 5.6,
            repeat: Infinity,
            ease: [0.42, 0, 0.58, 1],
          }}
        />

        {/* Fold overlay — a warped ellipse that suggests folded material */}
        <motion.ellipse
          cx="0"
          cy="0"
          rx="120"
          ry="55"
          fill="url(#sph-fold)"
          animate={{
            rx: [120, 132, 118, 120],
            ry: [55, 48, 62, 55],
            rotate: [15, 22, 8, 15],
          }}
          transition={{
            duration: 7,
            repeat: Infinity,
            ease: [0.42, 0, 0.58, 1],
          }}
        />
      </motion.g>

      {/* Two concentric ripple rings — expand outward and fade */}
      <RippleRing color={palette.ripple} delay={0} />
      <RippleRing color={palette.ripple} delay={1.4} />
    </motion.svg>
  );
}

function RippleRing({ color, delay }: { color: string; delay: number }) {
  return (
    <motion.circle
      cx="0"
      cy="0"
      r="150"
      fill="none"
      stroke={color}
      strokeWidth="1.4"
      animate={{
        r: [150, 185, 210],
        opacity: [0.55, 0.3, 0],
      }}
      transition={{
        duration: 2.8,
        repeat: Infinity,
        ease: [0.22, 1, 0.36, 1],
        delay,
      }}
    />
  );
}
