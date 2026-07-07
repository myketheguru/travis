/**
 * MapCanvas — v0.28.2.
 *
 * When the assistant returns a map part, the canvas surfaces it as a
 * loud center-stage card. Two shapes are supported:
 *
 *   route  — origin/destination with distance + duration
 *   place  — single location (city, neighborhood, landmark)
 *
 * Either shape renders a legible summary. When only a narration is
 * provided (LLM returned a bare map part), we still surface the
 * narration in the same visual frame so the user gets a coherent
 * response instead of a "coming soon" placeholder.
 *
 * Real MapLibre tile rendering is scoped for a follow-up release.
 */
import { motion } from "framer-motion";
import { useFocalContent } from "../useFocalContent";
import { parseRichResponse, type MapPart } from "../../../lib/richResponse";

import { ChatCanvas } from "./ChatCanvas";

export function MapCanvas() {
  const { focal } = useFocalContent();
  const mapPart = extractMapPart(focal?.content);

  if (!mapPart) return <ChatCanvas />;

  const hasRoute =
    !!mapPart.route && typeof mapPart.route.distance_meters === "number";
  const hasPlace = !!mapPart.place?.label;
  const hasNarration = !!mapPart.narration?.trim();

  // If we truly have nothing to show, degrade to chat rather than
  // rendering an empty frame.
  if (!hasRoute && !hasPlace && !hasNarration) {
    return <ChatCanvas />;
  }

  return (
    <div className="absolute inset-0 overflow-hidden pointer-events-none">
      <RouteSketch />

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
            // {hasRoute ? "route" : "place"}
          </div>
          <RenderHeadline mapPart={mapPart} />
          {hasRoute && <RouteMetrics mapPart={mapPart} />}
          {hasPlace && !hasRoute && <PlaceMetrics mapPart={mapPart} />}
          {hasNarration && (
            <div
              className="text-[14px] mt-6 leading-relaxed"
              style={{ color: "rgba(236, 236, 241, 0.78)" }}
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

function RenderHeadline({ mapPart }: { mapPart: MapPart }) {
  const title =
    mapPart.route?.destination_label ??
    mapPart.place?.label ??
    "map";
  return (
    <div
      className="text-[32px] font-light leading-tight tracking-tight"
      style={{ color: "rgb(236, 236, 241)" }}
    >
      {title}
    </div>
  );
}

function RouteMetrics({ mapPart }: { mapPart: MapPart }) {
  const route = mapPart.route!;
  const distanceKm = (route.distance_meters / 1000).toFixed(1);
  const durationMin = Math.round(route.duration_seconds / 60);
  return (
    <div
      className="flex items-baseline gap-6 mt-5 font-mono"
      style={{ color: "rgba(236, 236, 241, 0.88)" }}
    >
      <Metric label="duration" value={`${durationMin} min`} />
      <Metric label="distance" value={`${distanceKm} km`} />
      {route.profile && (
        <Metric label="mode" value={route.profile.replace("-", " ")} />
      )}
    </div>
  );
}

function PlaceMetrics({ mapPart }: { mapPart: MapPart }) {
  const place = mapPart.place!;
  const bits = [place.descriptor, place.region, place.country]
    .filter(Boolean)
    .join(" · ");
  if (!bits) return null;
  return (
    <div
      className="mt-5 font-mono text-[13px]"
      style={{ color: "rgba(236, 236, 241, 0.78)" }}
    >
      {bits}
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
