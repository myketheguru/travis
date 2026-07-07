/**
 * MapCanvas — v0.28.3.
 *
 * Real interactive map. MapLibre GL renders OpenStreetMap raster tiles
 * (no API key, no rate limit for reasonable use). Center + zoom come
 * from the LLM's `place` or `route` payload. Overlay pill on top with
 * the place name + descriptor.
 *
 * Falls back to a text-only card when the LLM didn't include coords
 * yet — that shape still works until v0.28.3's prompt update lands.
 */
import { useEffect, useRef } from "react";
import { motion } from "framer-motion";
import maplibregl, { type Map as MapLibreMap } from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";

import { useFocalContent } from "../useFocalContent";
import { parseRichResponse, type MapPart } from "../../../lib/richResponse";
import { ChatCanvas } from "./ChatCanvas";

const OSM_STYLE = {
  version: 8,
  sources: {
    osm: {
      type: "raster",
      tiles: [
        "https://a.tile.openstreetmap.org/{z}/{x}/{y}.png",
        "https://b.tile.openstreetmap.org/{z}/{x}/{y}.png",
        "https://c.tile.openstreetmap.org/{z}/{x}/{y}.png",
      ],
      tileSize: 256,
      attribution: "© OpenStreetMap contributors",
    },
  },
  layers: [
    {
      id: "osm",
      type: "raster",
      source: "osm",
      minzoom: 0,
      maxzoom: 19,
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

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    console.log(
      "[map] init — container size:",
      el.clientWidth,
      "x",
      el.clientHeight,
      "coords:",
      coords,
    );
    let map: MapLibreMap | null = null;
    try {
      map = new maplibregl.Map({
        container: el,
        style: OSM_STYLE,
        center: [coords.lng, coords.lat],
        zoom: coords.zoom,
        attributionControl: { compact: true },
      });
      mapRef.current = map;
      map.on("load", () => {
        console.log("[map] load event fired");
        map?.resize();
      });
      map.on("error", (e) => {
        console.warn("[map] error event:", e);
      });
      // Force a resize on the next tick — WebGL sometimes measures
      // 0x0 during initial mount inside stacked absolute containers.
      requestAnimationFrame(() => {
        map?.resize();
      });
      const marker = new maplibregl.Marker({
        color: "rgb(189, 158, 255)",
      })
        .setLngLat([coords.lng, coords.lat])
        .addTo(map);
      markerRef.current = marker;
    } catch (err) {
      // Silent failure — degrades to the text card below on next render.
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
          background: "rgba(0, 0, 0, 0.60)",
          border: "1px solid rgba(110, 196, 232, 0.35)",
          backdropFilter: "blur(14px)",
        }}
      >
        <div
          className="text-[10px] uppercase tracking-[0.22em] font-mono mb-1"
          style={{ color: "rgba(110, 196, 232, 0.85)" }}
        >
          // {mapPart.route ? "route" : "place"}
        </div>
        <div
          className="text-[17px] font-medium leading-tight"
          style={{ color: "rgb(236, 236, 241)" }}
        >
          {label}
        </div>
        {bits && (
          <div
            className="text-[12px] font-mono mt-1"
            style={{ color: "rgba(236, 236, 241, 0.7)" }}
          >
            {bits}
          </div>
        )}
        {mapPart.narration && (
          <div
            className="text-[12.5px] mt-2 leading-relaxed"
            style={{ color: "rgba(236, 236, 241, 0.78)" }}
          >
            {mapPart.narration}
          </div>
        )}
      </motion.div>
    </div>
  );
}

/**
 * Text-only rendering when the LLM gave us a map part with narration
 * but no coordinates. Used while the prompt-update side lands.
 */
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
        <div
          className="text-[10px] mt-5 font-mono tracking-wide"
          style={{ color: "rgba(236, 236, 241, 0.32)" }}
        >
          Interactive map appears when Travis includes coordinates.
        </div>
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
