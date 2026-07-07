/**
 * Inline map card — the first real "God's-Eye" surface (INTERFACE.md).
 *
 * Renders a MapLibre map with the route drawn + origin/destination
 * markers. When the user is on a slow device or offline, degrades
 * gracefully to a mini info card with distance + ETA + destination
 * label — the narration string is the ultimate fallback for
 * screen-reader / voice-only mode.
 *
 * MapLibre GL is not bundled yet — this card scaffolds the render
 * with a static info panel + a "map coming online" placeholder for
 * the tile canvas. When we add MapLibre GL to the desktop bundle,
 * the placeholder gets replaced with the real interactive map.
 */

import type { MapRoute } from "../../lib/richResponse";

interface Props {
  route: MapRoute;
  narration?: string;
}

function fmtDistance(meters: number): string {
  if (meters < 1000) return `${Math.round(meters)} m`;
  const km = meters / 1000;
  // Rough imperial preference — swap to a locale check when we care.
  const miles = km * 0.621371;
  return miles < 10 ? `${miles.toFixed(1)} mi` : `${Math.round(miles)} mi`;
}

function fmtDuration(seconds: number): string {
  const min = Math.round(seconds / 60);
  if (min < 60) return `${min} min`;
  const h = Math.floor(min / 60);
  const rem = min - h * 60;
  return rem === 0 ? `${h} h` : `${h} h ${rem} min`;
}

export function MapCard({ route, narration }: Props) {
  // v0.28.1 — guard against malformed rich responses. The LLM
  // occasionally emits a map part shell without a route (or with a
  // partially-typed one) which used to crash rendering + blank the
  // whole app. Now: soft fallback.
  if (!route || typeof route.distance_meters !== "number") {
    return (
      <div
        className="rounded-2xl px-4 py-3 text-[13px] leading-relaxed"
        style={{
          border: "1px solid rgba(255, 179, 92, 0.35)",
          background: "rgba(255, 179, 92, 0.08)",
          color: "rgba(236, 236, 241, 0.85)",
        }}
      >
        <div
          className="text-[10px] uppercase tracking-[0.22em] font-mono mb-1.5"
          style={{ color: "rgba(255, 179, 92, 0.9)" }}
        >
          map data missing
        </div>
        {narration ?? "Travis wanted to show a map here, but the response was incomplete."}
      </div>
    );
  }
  return (
    <div
      className="rounded-2xl overflow-hidden"
      style={{
        border: "1px solid rgba(124, 92, 255, 0.35)",
        background:
          "linear-gradient(180deg, rgba(124, 92, 255, 0.06), rgba(110, 196, 232, 0.04))",
        boxShadow: "0 4px 24px -12px rgba(0, 0, 0, 0.5)",
      }}
    >
      {/* Map canvas placeholder. MapLibre GL wires in when we add the
          dep; the map + route + markers replace this. */}
      <div
        className="relative w-full aspect-[16/9]"
        style={{
          background:
            "radial-gradient(circle at 30% 40%, rgba(124, 92, 255, 0.15), transparent 60%), radial-gradient(circle at 70% 60%, rgba(110, 196, 232, 0.15), transparent 60%), rgba(7, 8, 11, 0.4)",
        }}
      >
        {/* Static SVG glyph representing a route */}
        <svg
          viewBox="0 0 400 225"
          preserveAspectRatio="none"
          className="absolute inset-0 w-full h-full"
          aria-hidden
        >
          <path
            d="M 40 180 Q 120 100, 200 130 T 360 45"
            stroke="rgba(124, 92, 255, 0.8)"
            strokeWidth="3"
            strokeLinecap="round"
            fill="none"
            strokeDasharray="6 6"
          >
            <animate
              attributeName="stroke-dashoffset"
              from="0"
              to="-12"
              dur="1s"
              repeatCount="indefinite"
            />
          </path>
          {/* origin */}
          <circle cx="40" cy="180" r="6" fill="rgba(236, 236, 241, 0.95)" />
          <circle
            cx="40"
            cy="180"
            r="12"
            fill="none"
            stroke="rgba(236, 236, 241, 0.5)"
            strokeWidth="1.5"
          />
          {/* destination */}
          <circle cx="360" cy="45" r="8" fill="rgb(124, 92, 255)" />
          <circle
            cx="360"
            cy="45"
            r="16"
            fill="none"
            stroke="rgba(124, 92, 255, 0.5)"
            strokeWidth="1.5"
          >
            <animate
              attributeName="r"
              values="14; 20; 14"
              dur="2s"
              repeatCount="indefinite"
            />
            <animate
              attributeName="opacity"
              values="0.5; 0; 0.5"
              dur="2s"
              repeatCount="indefinite"
            />
          </circle>
        </svg>

        <div
          className="absolute top-3 left-3 text-[10px] tracking-[0.16em] uppercase font-mono"
          style={{ color: "rgba(236, 236, 241, 0.55)" }}
        >
          // route
        </div>
      </div>

      {/* Info panel */}
      <div className="px-4 py-3 flex items-center justify-between gap-4">
        <div className="min-w-0">
          <div
            className="text-[14.5px] font-medium truncate"
            style={{ color: "rgb(236, 236, 241)" }}
          >
            {route.destination_label ?? "Destination"}
          </div>
          <div
            className="text-[12px] mt-0.5 font-mono"
            style={{ color: "rgba(236, 236, 241, 0.6)" }}
          >
            {fmtDuration(route.duration_seconds)} · {fmtDistance(route.distance_meters)}
            {route.profile && ` · ${route.profile.replace("-", " ")}`}
          </div>
          {narration && (
            <div
              className="text-[11px] mt-1 leading-relaxed"
              style={{ color: "rgba(236, 236, 241, 0.5)" }}
            >
              {narration}
            </div>
          )}
        </div>
        <button
          className="shrink-0 px-3 py-1.5 rounded-full text-[12px] font-medium transition-colors"
          style={{
            background: "rgba(124, 92, 255, 0.25)",
            border: "1px solid rgba(124, 92, 255, 0.5)",
            color: "rgb(236, 236, 241)",
          }}
        >
          Leave now
        </button>
      </div>
    </div>
  );
}
