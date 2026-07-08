/**
 * Inline map card — v0.28.5.
 *
 * Renders in the conversation feed as a small clickable preview. Click
 * the card and Travis re-expands the map to full canvas. Works for
 * both route-based (A -> B) and place-based (single location) map parts.
 *
 * When there's no structured data at all (just narration), degrades
 * to a text-only card.
 */

import { useAppStore } from "../../stores/app";
import type { MapPlace, MapRoute } from "../../lib/richResponse";

interface Props {
  route?: MapRoute;
  place?: MapPlace;
  narration?: string;
  /// v0.28.5 — the assistant message id this map belongs to. Passed to
  /// setMapExpanded so useCanvasMode's auto-expand memoization stays
  /// per-focal.
  messageId?: string;
}

function fmtDistance(meters: number): string {
  if (meters < 1000) return `${Math.round(meters)} m`;
  const km = meters / 1000;
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

export function MapCard({ route, place, narration, messageId }: Props) {
  const setMapExpanded = useAppStore((s) => s.setMapExpanded);
  const hasRoute = !!route && typeof route.distance_meters === "number";
  const hasPlace = !!place?.label;

  const canExpand = hasRoute || hasPlace;

  const handleExpand = () => {
    if (!canExpand) return;
    setMapExpanded(true, messageId);
  };

  // No structured data — degrade to a text-only card (no expand).
  if (!hasRoute && !hasPlace) {
    if (!narration) return null;
    return (
      <div
        className="rounded-2xl px-4 py-3 text-[13.5px] leading-relaxed"
        style={{
          border: "1px solid rgba(189, 158, 255, 0.25)",
          background: "rgba(189, 158, 255, 0.05)",
          color: "rgba(236, 236, 241, 0.9)",
        }}
      >
        <div
          className="text-[10px] uppercase tracking-[0.22em] font-mono mb-1.5"
          style={{ color: "rgba(189, 158, 255, 0.8)" }}
        >
          // location
        </div>
        {narration}
      </div>
    );
  }

  const label =
    route?.destination_label ?? place?.label ?? "map";
  const bits =
    !hasRoute && hasPlace
      ? [place!.descriptor, place!.region, place!.country]
          .filter(Boolean)
          .join(" · ")
      : hasRoute
        ? [
            fmtDuration(route!.duration_seconds),
            fmtDistance(route!.distance_meters),
            route!.profile?.replace("-", " "),
          ]
            .filter(Boolean)
            .join(" · ")
        : "";

  return (
    <button
      onClick={handleExpand}
      disabled={!canExpand}
      className="w-full text-left rounded-2xl transition-transform disabled:cursor-not-allowed"
      style={{
        border: "1px solid rgba(189, 158, 255, 0.32)",
        background:
          "linear-gradient(180deg, rgba(189, 158, 255, 0.06), rgba(124, 92, 255, 0.02))",
        boxShadow: "0 4px 24px -12px rgba(0, 0, 0, 0.5)",
      }}
      onMouseEnter={(e) => {
        if (canExpand) e.currentTarget.style.transform = "translateY(-1px)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.transform = "translateY(0)";
      }}
    >
      <div className="px-4 py-3.5 flex items-center gap-4">
        <MapGlyph />
        <div className="min-w-0 flex-1">
          <div
            className="text-[10px] uppercase tracking-[0.22em] font-mono mb-0.5"
            style={{ color: "rgba(189, 158, 255, 0.85)" }}
          >
            // {hasRoute ? "route" : "place"}
          </div>
          <div
            className="text-[15.5px] font-medium truncate"
            style={{ color: "rgb(236, 236, 241)" }}
          >
            {label}
          </div>
          {bits && (
            <div
              className="text-[12px] font-mono mt-0.5"
              style={{ color: "rgba(236, 236, 241, 0.7)" }}
            >
              {bits}
            </div>
          )}
          {narration && (
            <div
              className="text-[12.5px] mt-1.5 leading-relaxed"
              style={{ color: "rgba(236, 236, 241, 0.72)" }}
            >
              {narration}
            </div>
          )}
        </div>
        {canExpand && (
          <div
            className="shrink-0 text-[10px] uppercase tracking-[0.22em] font-mono"
            style={{ color: "rgba(189, 158, 255, 0.75)" }}
          >
            expand ↗
          </div>
        )}
      </div>
    </button>
  );
}

function MapGlyph() {
  return (
    <div
      className="shrink-0 h-12 w-12 rounded-xl flex items-center justify-center"
      style={{
        background: "rgba(189, 158, 255, 0.12)",
        border: "1px solid rgba(189, 158, 255, 0.30)",
      }}
    >
      <svg
        width="20"
        height="20"
        viewBox="0 0 24 24"
        fill="none"
        stroke="rgb(189, 158, 255)"
        strokeWidth="1.6"
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden
      >
        <path d="M12 21s-7-6.5-7-12a7 7 0 0 1 14 0c0 5.5-7 12-7 12z" />
        <circle cx="12" cy="9" r="2.5" />
      </svg>
    </div>
  );
}
