/**
 * CanvasBackdrop — v2 Shell 2 + v0.28.44 cloth grid.
 *
 * Full-bleed backdrop with ambient depth that reflects Travis's
 * current activity. Video-game world-map feel: quiet when idle,
 * subtly alive when something's running.
 *
 * Stacked layers (bottom → top):
 *   1. Radial gradient wash — always on, gives depth
 *   2. Cloth grid — canvas-drawn mesh with minor (80px) and major
 *      (320px) cells. Vertices deform toward the pointer with a
 *      Gaussian falloff so the grid reads as a piece of stretched
 *      cloth that magnetizes around the cursor.
 *   3. Flowing aurora lines — slow SVG path morphs; intensity keys
 *      off the app store's activity state.
 *
 * Reduced-motion: no cloth deformation (grid renders static), no
 * aurora flow.
 */
import { useEffect, useRef, useState } from "react";
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
            "radial-gradient(circle at 20% 30%, rgba(124, 92, 255, 0.10), transparent 60%), " +
            "radial-gradient(circle at 80% 70%, rgba(110, 196, 232, 0.07), transparent 55%), " +
            "linear-gradient(180deg, rgba(255,255,255,0.015), transparent)",
        }}
      />

      {/* Layer 2 — cloth grid on canvas. Deforms under the cursor
          like a stretched fabric being tugged. */}
      <ClothGrid reducedMotion={reducedMotion} />

      {/* Layer 3 — flowing aurora lines. */}
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

/* ─── Cloth grid ────────────────────────────────────────────────── */

const MINOR_CELL = 80;
const SAMPLE_STEP = 20;        // how often to sample a line (px on the base grid)
const DEFORM_RADIUS = 180;     // px of influence around the cursor
const DEFORM_STRENGTH = 12;    // max px pull toward the cursor — tuned down from
                               // 34; original read as folding, not fabric-flex.
const CURSOR_LERP = 0.16;      // per-frame lerp toward true cursor
const GLOW_RADIUS = 260;       // brand-purple halo around the cursor

/// Canvas-drawn grid whose vertices are pulled toward the pointer
/// with a Gaussian falloff. Lines are drawn as polylines through
/// deformed sample points every SAMPLE_STEP so straight grid lines
/// visibly bend near the cursor.
function ClothGrid({ reducedMotion }: { reducedMotion: boolean }) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  // True cursor position (updated by pointermove) + smoothed value
  // used for drawing. `active` flag flips false when the pointer
  // leaves the window; we still animate the smoothed position back
  // to a rest state so the grid relaxes rather than snapping.
  const cursorRef = useRef<{ x: number; y: number; active: boolean }>({
    x: -9999,
    y: -9999,
    active: false,
  });
  const smoothRef = useRef<{ x: number; y: number; influence: number }>({
    x: -9999,
    y: -9999,
    influence: 0,
  });

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    let width = 0;
    let height = 0;

    const resize = () => {
      const dpr = Math.min(window.devicePixelRatio, 2);
      width = canvas.clientWidth;
      height = canvas.clientHeight;
      canvas.width = Math.floor(width * dpr);
      canvas.height = Math.floor(height * dpr);
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    };
    resize();
    const ro = new ResizeObserver(resize);
    ro.observe(canvas);

    const onMove = (e: PointerEvent) => {
      cursorRef.current.x = e.clientX;
      cursorRef.current.y = e.clientY;
      cursorRef.current.active = true;
    };
    const onLeave = () => {
      cursorRef.current.active = false;
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerleave", onLeave);

    // ─── Deformation function ────────────────────────────────────
    // Applies the cursor pull to (x, y). Returns [dx, dy] delta.
    // A vertex within DEFORM_RADIUS gets pulled toward the cursor
    // with a smooth 1 - (r/R)^2 falloff (parabolic; softer than
    // linear, no expensive exp).
    const deform = (
      x: number,
      y: number,
      cx: number,
      cy: number,
      influence: number,
    ): [number, number] => {
      if (influence <= 0.001) return [0, 0];
      const dx = cx - x;
      const dy = cy - y;
      const distSq = dx * dx + dy * dy;
      if (distSq > DEFORM_RADIUS * DEFORM_RADIUS) return [0, 0];
      const dist = Math.sqrt(distSq);
      const t = 1 - dist / DEFORM_RADIUS;
      // Falloff shaped like a soft bell — strong near the center,
      // decays smoothly to zero at the radius edge.
      const falloff = t * t * (3 - 2 * t); // smoothstep
      const magnitude = falloff * DEFORM_STRENGTH * influence;
      // Pull toward cursor (positive dx, dy already points inward).
      // Normalize by dist; guard against divide-by-zero at 0px.
      const norm = dist > 0.001 ? magnitude / dist : 0;
      return [dx * norm, dy * norm];
    };

    // ─── Line rasterizer ─────────────────────────────────────────
    // Draws a horizontal line at y=yPos across [xStart, xEnd] as a
    // polyline through deformed sample points. Same for vertical.
    const drawHorizontal = (
      yPos: number,
      xStart: number,
      xEnd: number,
      cx: number,
      cy: number,
      influence: number,
    ) => {
      ctx.beginPath();
      let firstPoint = true;
      for (let x = xStart; x <= xEnd + 0.01; x += SAMPLE_STEP) {
        const clampedX = Math.min(x, xEnd);
        const [dxOff, dyOff] = deform(clampedX, yPos, cx, cy, influence);
        const px = clampedX + dxOff;
        const py = yPos + dyOff;
        if (firstPoint) {
          ctx.moveTo(px, py);
          firstPoint = false;
        } else {
          ctx.lineTo(px, py);
        }
      }
      ctx.stroke();
    };
    const drawVertical = (
      xPos: number,
      yStart: number,
      yEnd: number,
      cx: number,
      cy: number,
      influence: number,
    ) => {
      ctx.beginPath();
      let firstPoint = true;
      for (let y = yStart; y <= yEnd + 0.01; y += SAMPLE_STEP) {
        const clampedY = Math.min(y, yEnd);
        const [dxOff, dyOff] = deform(xPos, clampedY, cx, cy, influence);
        const px = xPos + dxOff;
        const py = clampedY + dyOff;
        if (firstPoint) {
          ctx.moveTo(px, py);
          firstPoint = false;
        } else {
          ctx.lineTo(px, py);
        }
      }
      ctx.stroke();
    };

    // ─── Frame loop ──────────────────────────────────────────────
    let raf = 0;
    const frame = () => {
      // Advance the smoothed cursor toward the real one, or toward
      // a rest state when the pointer's left the window.
      const target = cursorRef.current;
      const smooth = smoothRef.current;
      if (target.active) {
        if (smooth.influence < 1) {
          smooth.x = target.x;
          smooth.y = target.y;
          smooth.influence = 1;
        } else {
          smooth.x += (target.x - smooth.x) * CURSOR_LERP;
          smooth.y += (target.y - smooth.y) * CURSOR_LERP;
        }
      } else {
        // Fade influence out over a few frames so the grid relaxes
        // gently instead of snapping back when the pointer leaves.
        smooth.influence *= 0.9;
        if (smooth.influence < 0.01) smooth.influence = 0;
      }

      const inf = reducedMotion ? 0 : smooth.influence;
      const cx = smooth.x;
      const cy = smooth.y;

      ctx.clearRect(0, 0, width, height);

      // v0.28.44 — uniform grid at MINOR_CELL spacing. Major grid
      // and intersection dots were pulled per the user's ask so the
      // scaffolding reads as one texture, not two densities.
      ctx.strokeStyle = "rgba(189, 158, 255, 0.055)";
      ctx.lineWidth = 1;
      for (let y = 0; y <= height; y += MINOR_CELL) {
        drawHorizontal(y, 0, width, cx, cy, inf);
      }
      for (let x = 0; x <= width; x += MINOR_CELL) {
        drawVertical(x, 0, height, cx, cy, inf);
      }

      // v0.28.44 — cursor glow. Wider + fainter than the first pass:
      // 160px reach, low-alpha center that decays across three stops
      // so it fades to nothing over a big area. `screen` composite
      // brightens whatever's underneath without occluding it.
      if (inf > 0.05 && cx > -1000) {
        ctx.save();
        ctx.globalCompositeOperation = "screen";
        const grad = ctx.createRadialGradient(cx, cy, 0, cx, cy, GLOW_RADIUS);
        grad.addColorStop(0, `rgba(210, 185, 255, ${0.11 * inf})`);
        grad.addColorStop(0.3, `rgba(189, 158, 255, ${0.055 * inf})`);
        grad.addColorStop(0.65, `rgba(189, 158, 255, ${0.018 * inf})`);
        grad.addColorStop(1, "rgba(189, 158, 255, 0)");
        ctx.fillStyle = grad;
        ctx.beginPath();
        ctx.arc(cx, cy, GLOW_RADIUS, 0, Math.PI * 2);
        ctx.fill();
        ctx.restore();
      }

      raf = requestAnimationFrame(frame);
    };
    frame();

    return () => {
      cancelAnimationFrame(raf);
      ro.disconnect();
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerleave", onLeave);
    };
  }, [reducedMotion]);

  return (
    <canvas
      ref={canvasRef}
      className="absolute inset-0 w-full h-full pointer-events-none"
      aria-hidden
    />
  );
}

/* ─── Aurora sub-component ─────────────────────────────────────── */

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
