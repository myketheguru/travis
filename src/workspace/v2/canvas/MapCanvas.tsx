/**
 * MapCanvas — v0.28.5.
 *
 * Fullscreen interactive map. MapLibre GL renders CartoDB Dark Matter
 * tiles so the map matches Travis's dark aesthetic. Info card overlay
 * top-left with label + descriptor + narration, and a collapse button
 * that returns the user to chat mode (the map focal renders as an
 * inline MapCard the user can click to re-expand).
 *
 * Falls back to a text-only card when the LLM didn't include coords.
 */
import { useEffect, useRef } from "react";
import { motion } from "framer-motion";
import maplibregl, { type Map as MapLibreMap } from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";

import { useFocalContent } from "../useFocalContent";
import { parseRichResponse, type MapPart } from "../../../lib/richResponse";
import { useAppStore } from "../../../stores/app";
import { ChatCanvas } from "./ChatCanvas";

// CartoDB Dark Matter — free-tier friendly, matches Travis's dark canvas.
// Attribution required (rendered by MapLibre automatically).
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
      attribution:
        "© OpenStreetMap contributors © CARTO",
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
  if (!coords) {
    return <TextOnlyMap mapPart={mapPart} />;
  }

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
      const marker = new maplibregl.Marker({
        color: "rgb(189, 158, 255)",
      })
        .setLngLat([coords.lng, coords.lat])
        .addTo(map);
      markerRef.current = marker;
    } catch (err) {
      console.warn("[map] MapLibre init failed:", err);
    }
    return () => {
      markerRef.current?.remove();
      mapRef.current?.remove();
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
      <div
        ref={containerRef}
        className="absolute inset-0"
        style={{
          background: "rgb(6, 6, 10)",
          minWidth: "100%",
          minHeight: "100%",
        }}
      />

      <motion.div
        initial={{ opacity: 0, y: -8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.36, ease: [0.22, 1, 0.36, 1] }}
        className="absolute top-16 left-4 max-w-md rounded-2xl px-4 py-3 pointer-events-auto"
        style={{
          background: "rgba(0, 0, 0, 0.72)",
          border: "1px solid rgba(189, 158, 255, 0.40)",
          backdropFilter: "blur(14px)",
          boxShadow: "0 12px 40px -12px rgba(0,0,0,0.6)",
        }}
      >
        <div className="flex items-start gap-3">
          <div className="min-w-0 flex-1">
            <div
              className="text-[10px] uppercase tracking-[0.22em] font-mono mb-1"
              style={{ color: "rgba(189, 158, 255, 0.85)" }}
            >
              // {mapPart.route ? "route" : "place"}
            </div>
            <div
              className="text-[17px] font-medium leading-tight truncate"
              style={{ color: "rgb(236, 236, 241)" }}
            >
              {label}
            </div>
            {bits && (
              <div
                className="text-[12px] font-mono mt-1"
                style={{ color: "rgba(236, 236, 241, 0.72)" }}
              >
                {bits}
              </div>
            )}
            {mapPart.narration && (
              <div
                className="text-[12.5px] mt-2 leading-relaxed"
                style={{ color: "rgba(236, 236, 241, 0.82)" }}
              >
                {mapPart.narration}
              </div>
            )}
          </div>
          <button
            onClick={() => setMapExpanded(false)}
            className="shrink-0 h-7 w-7 rounded-full flex items-center justify-center transition-colors"
            style={{
              background: "rgba(255, 255, 255, 0.08)",
              border: "1px solid rgba(255, 255, 255, 0.14)",
              color: "rgba(236, 236, 241, 0.85)",
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
            "linear-gradient(180deg, rgba(20, 20, 26, 0.85), rgba(12, 12, 16, 0.92))",
          border: "1px solid rgba(189, 158, 255, 0.32)",
          backdropFilter: "blur(20px)",
        }}
      >
        <div
          className="text-[10px] uppercase tracking-[0.28em] font-mono mb-3"
          style={{ color: "rgba(189, 158, 255, 0.85)" }}
        >
          // {mapPart.route ? "route" : "place"}
        </div>
        <div
          className="text-[32px] font-light leading-tight tracking-tight"
          style={{ color: "rgb(236, 236, 241)" }}
        >
          {label}
        </div>
        {mapPart.narration && (
          <div
            className="text-[14px] mt-5 leading-relaxed"
            style={{ color: "rgba(236, 236, 241, 0.78)" }}
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
    return {
      lat: mapPart.route.to.lat,
      lng: mapPart.route.to.lng,
      zoom: 12,
    };
  }
  return null;
}

function extractMapPart(content: string | undefined): MapPart | null {
  if (!content) return null;
  const rich = parseRichResponse(content);
  if (!rich) return null;
  return (rich.parts.find((p) => p.kind === "map") as MapPart | undefined) ?? null;
}
