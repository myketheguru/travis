/**
 * MapCanvas — v2 Shell 16.
 *
 * When the latest assistant response is a map part, the canvas becomes
 * the map — full-bleed animated route background + info overlay top-
 * left with the destination + distance + duration.
 *
 * MapLibre integration is a follow-up; for now this is an animated SVG
 * route on a subtle grid, using the map part's geometry hints.
 */
import { motion } from "framer-motion";
import { useFocalContent } from "../useFocalContent";
import { parseRichResponse, type MapPart } from "../../../lib/richResponse";

export function MapCanvas() {
  const { focal } = useFocalContent();
  const mapPart = extractMapPart(focal?.content);

  if (!mapPart) return null;

  const { route } = mapPart;
  const distanceKm = (route.distance_meters / 1000).toFixed(1);
  const durationMin = Math.round(route.duration_seconds / 60);

  return (
    <div className="absolute inset-0 overflow-hidden pointer-events-none">
      {/* Base tint */}
      <div
        className="absolute inset-0"
        style={{
          background:
            "radial-gradient(circle at 30% 40%, rgba(110, 196, 232, 0.06), transparent 65%), " +
            "linear-gradient(180deg, rgba(255,255,255,0.02), transparent)",
        }}
      />

      {/* Grid */}
      <svg
        className="absolute inset-0 w-full h-full"
        aria-hidden
      >
        <defs>
          <pattern
            id="map-grid"
            width="120"
            height="120"
            patternUnits="userSpaceOnUse"
          >
            <path
              d="M 120 0 L 0 0 0 120"
              fill="none"
              stroke="rgba(110, 196, 232, 0.06)"
              strokeWidth="1"
            />
          </pattern>
        </defs>
        <rect width="100%" height="100%" fill="url(#map-grid)" />
      </svg>

      {/* Route sweep */}
      <motion.svg
        className="absolute inset-0 w-full h-full"
        viewBox="0 0 1000 700"
        preserveAspectRatio="none"
        aria-hidden
      >
        <defs>
          <linearGradient id="routeGrad" x1="0%" y1="0%" x2="100%" y2="0%">
            <stop offset="0%" stopColor="rgba(110, 196, 232, 0)" />
            <stop offset="45%" stopColor="rgba(110, 196, 232, 0.55)" />
            <stop offset="100%" stopColor="rgba(189, 158, 255, 0)" />
          </linearGradient>
        </defs>
        <motion.path
          fill="none"
          stroke="url(#routeGrad)"
          strokeWidth="2.5"
          strokeLinecap="round"
          d="M 60 500 C 250 380, 500 620, 720 260 C 850 60, 940 200, 960 200"
          initial={{ pathLength: 0, opacity: 0 }}
          animate={{ pathLength: 1, opacity: 1 }}
          transition={{ duration: 2.2, ease: [0.22, 1, 0.36, 1] }}
        />
        {/* Origin marker */}
        <motion.circle
          cx="60"
          cy="500"
          r="9"
          fill="rgba(110, 196, 232, 0.85)"
          initial={{ scale: 0 }}
          animate={{ scale: [0, 1.2, 1] }}
          transition={{ duration: 0.6, delay: 0.4, ease: [0.34, 1.56, 0.64, 1] }}
        />
        {/* Destination marker */}
        <motion.circle
          cx="960"
          cy="200"
          r="11"
          fill="rgba(189, 158, 255, 0.9)"
          initial={{ scale: 0 }}
          animate={{ scale: [0, 1.3, 1] }}
          transition={{ duration: 0.6, delay: 2.0, ease: [0.34, 1.56, 0.64, 1] }}
        />
        {/* Slow ambient drift */}
        <motion.circle
          cx="480"
          cy="360"
          r="180"
          fill="none"
          stroke="rgba(110, 196, 232, 0.10)"
          animate={{ r: [180, 220, 180] }}
          transition={{ duration: 8, repeat: Infinity, ease: [0.42, 0, 0.58, 1] }}
        />
      </motion.svg>

      {/* Info card top-left */}
      <motion.div
        initial={{ opacity: 0, x: -12 }}
        animate={{ opacity: 1, x: 0 }}
        transition={{ duration: 0.42, delay: 0.6, ease: [0.22, 1, 0.36, 1] }}
        className="absolute top-16 left-4 rounded-2xl px-4 py-3 pointer-events-auto"
        style={{
          background: "rgba(0, 0, 0, 0.35)",
          border: "1px solid rgba(110, 196, 232, 0.25)",
          backdropFilter: "blur(10px)",
          maxWidth: 320,
        }}
      >
        <div
          className="text-[10px] uppercase tracking-[0.22em] font-mono mb-1"
          style={{ color: "rgba(110, 196, 232, 0.75)" }}
        >
          // route
        </div>
        <div
          className="text-[16px] font-medium leading-tight"
          style={{ color: "rgb(236, 236, 241)" }}
        >
          {route.destination_label ?? "your destination"}
        </div>
        <div
          className="text-[12.5px] font-mono mt-2 flex gap-4"
          style={{ color: "rgba(236, 236, 241, 0.75)" }}
        >
          <span>{durationMin} min</span>
          <span>·</span>
          <span>{distanceKm} km</span>
          {route.profile && (
            <>
              <span>·</span>
              <span>{route.profile.replace("-", " ")}</span>
            </>
          )}
        </div>
        {mapPart.narration && (
          <div
            className="text-[11.5px] mt-2 leading-relaxed"
            style={{ color: "rgba(236, 236, 241, 0.6)" }}
          >
            {mapPart.narration}
          </div>
        )}
      </motion.div>
    </div>
  );
}

function extractMapPart(content: string | undefined): MapPart | null {
  if (!content) return null;
  const rich = parseRichResponse(content);
  if (!rich) return null;
  const first = rich.parts[0];
  return first?.kind === "map" ? first : null;
}
