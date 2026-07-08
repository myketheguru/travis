/**
 * MapCanvas — v0.28.6.
 *
 * Fullscreen interactive map. MapLibre GL renders CartoDB Dark Matter
 * tiles + a subtle purple brand overlay so the map feels distinctly
 * Travis-branded rather than a stock OSM viewer. Info card overlay is
 * lighter now (was making the already-dark map appear even darker)
 * with a translucent purple accent. Custom marker uses the brand
 * violet with a soft glow + pulse.
 *
 * NOTE: when light-mode lands, swap the CartoDB style for Voyager
 * (dark) or Positron (light) and lighten the brand overlay.
 */
import { useEffect, useRef } from "react";
import { motion } from "framer-motion";
import maplibregl, { type Map as MapLibreMap } from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";

import { useFocalContent } from "../useFocalContent";
import { parseRichResponse, type MapPart } from "../../../lib/richResponse";
import { useAppStore } from "../../../stores/app";
import { ChatCanvas } from "./ChatCanvas";

const DARK_STYLE = {
  version: 8,
  sources: {
    carto: {
      type: "raster",
      tiles: [
        "https://a.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}@2x.png",
        "https://b.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}@2x.png",
        "https://c.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}@2x.png",
        "https://d.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}@2x.png",
      ],
      tileSize: 256,
      attribution: "© OpenStreetMap contributors © CARTO",
    },
  },
  layers: [
    {
      id: "carto",
      type: "raster",
      source: "carto",
      minzoom: 0,
      maxzoom: 20,
    },
  ],
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
} as any;

export function MapCanvas() {
  const { focal } = useFocalContent();
  const mapPart = extractMapPart(focal?.content);
  if (!mapPart) return <ChatCanvas />;

  const coords = coordsFromMapPart(mapPart);
  if (!coords) return <TextOnlyMap mapPart={mapPart} />;

  return <InteractiveMap mapPart={mapPart} coords={coords} />;
}

function InteractiveMap({
  mapPart,
  coords,
}: {
  mapPart: MapPart;
  coords: { lat: number; lng: number; zoom: number };
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const mapRef = useRef<MapLibreMap | null>(null);
  const markerRef = useRef<maplibregl.Marker | null>(null);
  const setMapExpanded = useAppStore((s) => s.setMapExpanded);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    let map: MapLibreMap | null = null;
    try {
      map = new maplibregl.Map({
        container: el,
        style: DARK_STYLE,
        center: [coords.lng, coords.lat],
        zoom: coords.zoom,
        attributionControl: { compact: true },
      });
      mapRef.current = map;
      map.on("load", () => map?.resize());
      requestAnimationFrame(() => map?.resize());

      // Custom branded marker — violet with pulse.
      const markerEl = document.createElement("div");
      markerEl.className = "travis-map-marker";
      markerEl.innerHTML = `
        <div class="travis-marker-pulse"></div>
        <div class="travis-marker-dot"></div>
      `;
      const marker = new maplibregl.Marker({ element: markerEl })
        .setLngLat([coords.lng, coords.lat])
        .addTo(map);
      markerRef.current = marker;
    } catch (err) {
      console.warn("[map] MapLibre init failed:", err);
    }
    return () => {
      // v0.28.11 — wrap each cleanup so a marker-remove throw can't
      // block map.remove() (which was causing the "blank on rapid
      // switch" bug: half-cleaned MapLibre instance blocked next init).
      try { markerRef.current?.remove(); } catch { /* ignore */ }
      try { mapRef.current?.remove(); } catch { /* ignore */ }
      markerRef.current = null;
      mapRef.current = null;
    };
  }, [coords.lat, coords.lng, coords.zoom]);

  useEffect(() => {
    if (!mapRef.current) return;
    mapRef.current.flyTo({
      center: [coords.lng, coords.lat],
      zoom: coords.zoom,
      duration: 1200,
      essential: true,
    });
  }, [coords.lat, coords.lng, coords.zoom]);

  const label =
    mapPart.route?.destination_label ?? mapPart.place?.label ?? "map";
  const bits = [
    mapPart.place?.descriptor,
    mapPart.place?.region,
    mapPart.place?.country,
  ]
    .filter(Boolean)
    .join(" · ");

  return (
    <div className="absolute inset-0 overflow-hidden">
      <BrandMarkerStyles />

      <div
        ref={containerRef}
        className="absolute inset-0"
        style={{
          background: "rgb(6, 6, 10)",
          minWidth: "100%",
          minHeight: "100%",
        }}
      />

      {/* v0.28.6 brand tint — subtle violet gradient at top + bottom */}
      <div
        className="absolute inset-0 pointer-events-none"
        style={{
          background:
            "radial-gradient(ellipse at 50% -20%, rgba(189, 158, 255, 0.14), transparent 55%), radial-gradient(ellipse at 50% 120%, rgba(124, 92, 255, 0.10), transparent 55%)",
          mixBlendMode: "screen",
        }}
      />

      <motion.div
        initial={{ opacity: 0, y: -8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.36, ease: [0.22, 1, 0.36, 1] }}
        className="absolute top-16 left-4 max-w-md rounded-2xl px-4 py-3 pointer-events-auto"
        style={{
          // v0.28.6 — lighter card. Was rgba(0,0,0,0.72) which
          // stacked with the already-dark map and read as opaque
          // black. Now a translucent slate-violet that lets tile
          // detail show through.
          background:
            "linear-gradient(180deg, rgba(28, 24, 40, 0.62), rgba(20, 18, 30, 0.58))",
          border: "1px solid rgba(189, 158, 255, 0.32)",
          backdropFilter: "blur(18px) saturate(1.2)",
          boxShadow:
            "0 12px 40px -14px rgba(0, 0, 0, 0.6), 0 0 30px -8px rgba(189, 158, 255, 0.20)",
        }}
      >
        <div className="flex items-start gap-3">
          <div className="min-w-0 flex-1">
            <div
              className="text-[10px] uppercase tracking-[0.22em] font-mono mb-1"
              style={{ color: "rgba(189, 158, 255, 0.90)" }}
            >
              // {mapPart.route ? "route" : "place"}
            </div>
            <div
              className="text-[17px] font-medium leading-tight truncate"
              style={{ color: "rgb(240, 240, 246)" }}
            >
              {label}
            </div>
            {bits && (
              <div
                className="text-[12px] font-mono mt-1"
                style={{ color: "rgba(236, 236, 241, 0.78)" }}
              >
                {bits}
              </div>
            )}
            {mapPart.narration && (
              <div
                className="text-[12.5px] mt-2 leading-relaxed"
                style={{ color: "rgba(236, 236, 241, 0.88)" }}
              >
                {mapPart.narration}
              </div>
            )}
          </div>
          <button
            onClick={() => setMapExpanded(false)}
            className="shrink-0 h-7 w-7 rounded-full flex items-center justify-center transition-colors"
            style={{
              background: "rgba(255, 255, 255, 0.10)",
              border: "1px solid rgba(255, 255, 255, 0.18)",
              color: "rgba(236, 236, 241, 0.90)",
            }}
            title="Collapse map · return to chat"
            aria-label="Collapse map"
          >
            <svg
              width="12"
              height="12"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2.2"
              strokeLinecap="round"
              strokeLinejoin="round"
              aria-hidden
            >
              <path d="M6 6l12 12M18 6L6 18" />
            </svg>
          </button>
        </div>
      </motion.div>
    </div>
  );
}

function BrandMarkerStyles() {
  return (
    <style>
      {`
        .travis-map-marker {
          position: relative;
          width: 26px;
          height: 26px;
          display: flex;
          align-items: center;
          justify-content: center;
        }
        .travis-marker-dot {
          width: 14px;
          height: 14px;
          border-radius: 50%;
          background: radial-gradient(circle at 30% 30%, rgb(220, 200, 255), rgb(160, 120, 240));
          border: 2px solid rgba(255, 255, 255, 0.85);
          box-shadow:
            0 0 0 1px rgba(189, 158, 255, 0.35),
            0 0 20px 4px rgba(189, 158, 255, 0.55);
          position: relative;
          z-index: 2;
        }
        .travis-marker-pulse {
          position: absolute;
          inset: 0;
          border-radius: 50%;
          background: rgba(189, 158, 255, 0.32);
          animation: travis-marker-pulse 2.2s cubic-bezier(0.22, 1, 0.36, 1) infinite;
          z-index: 1;
        }
        @keyframes travis-marker-pulse {
          0%   { transform: scale(0.6); opacity: 0.65; }
          70%  { transform: scale(2.2); opacity: 0;    }
          100% { transform: scale(2.2); opacity: 0;    }
        }
        @media (prefers-reduced-motion: reduce) {
          .travis-marker-pulse { animation: none; opacity: 0; }
        }
      `}
    </style>
  );
}

function TextOnlyMap({ mapPart }: { mapPart: MapPart }) {
  const label =
    mapPart.route?.destination_label ?? mapPart.place?.label ?? "map";
  return (
    <div className="absolute inset-0 flex items-center justify-center px-6 pointer-events-none">
      <motion.div
        initial={{ opacity: 0, y: 12 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.42, ease: [0.22, 1, 0.36, 1] }}
        className="max-w-2xl w-full rounded-3xl px-8 py-8 pointer-events-auto"
        style={{
          background:
            "linear-gradient(180deg, rgba(28, 24, 40, 0.62), rgba(20, 18, 30, 0.58))",
          border: "1px solid rgba(189, 158, 255, 0.32)",
          backdropFilter: "blur(20px)",
        }}
      >
        <div
          className="text-[10px] uppercase tracking-[0.28em] font-mono mb-3"
          style={{ color: "rgba(189, 158, 255, 0.90)" }}
        >
          // {mapPart.route ? "route" : "place"}
        </div>
        <div
          className="text-[32px] font-light leading-tight tracking-tight"
          style={{ color: "rgb(240, 240, 246)" }}
        >
          {label}
        </div>
        {mapPart.narration && (
          <div
            className="text-[14px] mt-5 leading-relaxed"
            style={{ color: "rgba(236, 236, 241, 0.88)" }}
          >
            {mapPart.narration}
          </div>
        )}
      </motion.div>
    </div>
  );
}

function coordsFromMapPart(
  mapPart: MapPart,
): { lat: number; lng: number; zoom: number } | null {
  if (
    mapPart.place &&
    typeof mapPart.place.lat === "number" &&
    typeof mapPart.place.lng === "number"
  ) {
    return { lat: mapPart.place.lat, lng: mapPart.place.lng, zoom: 11 };
  }
  if (mapPart.route?.to?.lat != null && mapPart.route?.to?.lng != null) {
    return { lat: mapPart.route.to.lat, lng: mapPart.route.to.lng, zoom: 12 };
  }
  return null;
}

function extractMapPart(content: string | undefined): MapPart | null {
  if (!content) return null;
  const rich = parseRichResponse(content);
  if (!rich) return null;
  return (rich.parts.find((p) => p.kind === "map") as MapPart | undefined) ?? null;
}
