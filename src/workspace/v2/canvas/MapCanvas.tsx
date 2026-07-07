/**
 * MapCanvas — v0.28.2.
 *
 * When the assistant returns a map part, the canvas becomes a route
 * summary. Center-stage destination + distance + duration + narration
 * over a subtle animated route sketch. Loud enough that a user in a
 * dark room can read it at a glance — the SVG art alone was invisible
 * against the black canvas in v0.28.1.
 *
 * Real MapLibre tile rendering is scoped for a future release; when it
 * lands, the SVG placeholder gets replaced with the interactive map.
 */
import { motion } from "framer-motion";
import { useFocalContent } from "../useFocalContent";
import { parseRichResponse, type MapPart } from "../../../lib/richResponse";

import { ChatCanvas } from "./ChatCanvas";

export function MapCanvas() {
  const { focal } = useFocalContent();
  const mapPart = extractMapPart(focal?.content);

  if (
    !mapPart ||
    !mapPart.route ||
    typeof mapPart.route.distance_meters !== "number"
  ) {
    return <ChatCanvas />;
  }

  const { route } = mapPart;
  const distanceKm = (route.distance_meters / 1000).toFixed(1);
  const durationMin = Math.round(route.duration_seconds / 60);
  const destination = route.destination_label ?? "your destination";

  return (
    <div className="absolute inset-0 overflow-hidden pointer-events-none">
      {/* Subtle animated route sketch behind everything */}
      <RouteSketch />

      {/* Foreground info card — center-stage, loud, readable */}
      <motion.div
        initial={{ opacity: 0, y: 12 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.42, ease: [0.22, 1, 0.36, 1] }}
        className="absolute inset-0 flex items-center justify-center px-6 pointer-events-none"
        style={{ paddingBottom: 180 }}
      >
        <div
          className="max-w-2xl w-full rounded-3xl px-8 py-8 pointer-events-auto"
          style={{
            background:
              "linear-gradient(180deg, rgba(20, 20, 26, 0.85), rgba(12, 12, 16, 0.92))",
            border: "1px solid rgba(110, 196, 232, 0.32)",
            backdropFilter: "blur(20px)",
            boxShadow:
              "0 24px 80px -20px rgba(0, 0, 0, 0.7), 0 0 60px -12px rgba(110, 196, 232, 0.20)",
          }}
        >
          <div
            className="text-[10px] uppercase tracking-[0.28em] font-mono mb-3"
            style={{ color: "rgba(110, 196, 232, 0.85)" }}
          >
            // route
          </div>
          <div
            className="text-[32px] font-light leading-tight tracking-tight"
            style={{ color: "rgb(236, 236, 241)" }}
          >
            {destination}
          </div>
          <div
            className="flex items-baseline gap-6 mt-5 font-mono"
            style={{ color: "rgba(236, 236, 241, 0.88)" }}
          >
            <Metric label="duration" value={`${durationMin} min`} />
            <Metric label="distance" value={`${distanceKm} km`} />
            {route.profile && (
              <Metric
                label="mode"
                value={route.profile.replace("-", " ")}
              />
            )}
          </div>
          {mapPart.narration && (
            <div
              className="text-[14px] mt-6 leading-relaxed"
              style={{ color: "rgba(236, 236, 241, 0.72)" }}
            >
              {mapPart.narration}
            </div>
          )}
          <div
            className="text-[10px] mt-5 font-mono tracking-wide"
            style={{ color: "rgba(236, 236, 241, 0.32)" }}
          >
            interactive map view · coming soon
          </div>
        </div>
      </motion.div>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <div
        className="text-[9px] uppercase tracking-[0.22em]"
        style={{ color: "rgba(236, 236, 241, 0.42)" }}
      >
        {label}
      </div>
      <div className="text-[17px] mt-0.5">{value}</div>
    </div>
  );
}

function RouteSketch() {
  return (
    <>
      <div
        className="absolute inset-0"
        style={{
          background:
            "radial-gradient(circle at 30% 40%, rgba(110, 196, 232, 0.06), transparent 65%)",
        }}
      />
      <svg className="absolute inset-0 w-full h-full" aria-hidden>
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
      <motion.svg
        className="absolute inset-0 w-full h-full"
        viewBox="0 0 1000 700"
        preserveAspectRatio="none"
        aria-hidden
      >
        <defs>
          <linearGradient id="routeGrad" x1="0%" y1="0%" x2="100%" y2="0%">
            <stop offset="0%" stopColor="rgba(110, 196, 232, 0)" />
            <stop offset="45%" stopColor="rgba(110, 196, 232, 0.45)" />
            <stop offset="100%" stopColor="rgba(189, 158, 255, 0)" />
          </linearGradient>
        </defs>
        <motion.path
          fill="none"
          stroke="url(#routeGrad)"
          strokeWidth="2"
          strokeLinecap="round"
          d="M 60 500 C 250 380, 500 620, 720 260 C 850 60, 940 200, 960 200"
          initial={{ pathLength: 0, opacity: 0 }}
          animate={{ pathLength: 1, opacity: 0.6 }}
          transition={{ duration: 2.2, ease: [0.22, 1, 0.36, 1] }}
        />
      </motion.svg>
    </>
  );
}

function extractMapPart(content: string | undefined): MapPart | null {
  if (!content) return null;
  const rich = parseRichResponse(content);
  if (!rich) return null;
  return (rich.parts.find((p) => p.kind === "map") as MapPart | undefined) ?? null;
}
