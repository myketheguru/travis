/**
 * MapCanvas — v0.28.14 with OpenFreeMap vector tiles.
 *
 * Vector-tile custom styling delivers the deferred v0.28.6 promise:
 * we now use OpenFreeMap's dark preset (open-source, free, no API
 * key) instead of raster CartoDB tiles. Because these are vector
 * tiles, MapLibre GL renders them at any zoom + we can tint/blend
 * with Travis brand accents.
 *
 * Info card overlay: translucent slate-violet (lighter than v0.28.5)
 * with brand purple accents. Custom marker with pulse.
 */
import { useEffect, useMemo, useRef, useState } from "react";
import { motion } from "framer-motion";
import maplibregl, { type Map as MapLibreMap } from "maplibre-gl";
import "maplibre-gl/dist/maplibre-gl.css";

import { useFocalContent } from "../useFocalContent";
import { parseRichResponse, type MapPart, type MapOverlay } from "../../../lib/richResponse";
import { useAppStore } from "../../../stores/app";
import { ChatCanvas } from "./ChatCanvas";

// OpenFreeMap dark style — free vector tiles, no API key.
// If OpenFreeMap ever becomes unavailable, MapLibre falls through to
// the raster fallback source we include so the map never goes blank.
const STYLE_URL = "https://tiles.openfreemap.org/styles/dark";

export function MapCanvas() {
  const { focal } = useFocalContent();
  // v0.28.34 — memoize on focal.content (a stable string) so mapPart
  // holds a stable object reference across renders. Without this
  // every render created a new mapPart, cascading fresh references
  // through InteractiveMap's memos + effects.
  const mapPart = useMemo(
    () => extractMapPart(focal?.content),
    [focal?.content],
  );
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
  const markersRef = useRef<maplibregl.Marker[]>([]);
  // v0.28.43 — ref (not state) so it's visible synchronously in the
  // sibling geometrySig effect the same tick the fetch starts. If we
  // rely on `isFetchingPath` state, the sibling effect reads the
  // previous render's `false` and prematurely commits "straight".
  const isFetchingRef = useRef(false);
  const setMapExpanded = useAppStore((s) => s.setMapExpanded);
  // v0.28.36 — real-path fetch. When we have endpoints but no
  // geometry from the LLM, hit the cloud maps proxy for the actual
  // road-following LineString. Straight line renders instantly as a
  // fallback while this resolves; the fetched geometry then upgrades
  // the layer in place (like Google Maps loading the path).
  const [fetchedGeometry, setFetchedGeometry] = useState<unknown | null>(null);
  // v0.28.42 — surface which geometry source is actively drawn so
  // users (and I) can see at a glance whether the road-follow made
  // it. Green = real ORS, yellow = LLM-supplied, red = straight
  // fallback, gray = fetching. v0.28.43 — separate a "fetch is in
  // flight" flag from the source itself so the straight-line pass
  // during load doesn't overwrite `loading` in the badge.
  const [pathSource, setPathSource] = useState<"cloud" | "llm" | "straight" | "loading" | "none">("none");
  const [pathErrorReason, setPathErrorReason] = useState<string | null>(null);

  // v0.28.31 — decide markers to place based on part shape. For a
  // pure place, one marker in the middle. For a route, two markers
  // (from + to) each with a persistent label above the dot so the
  // user doesn't have to guess which end is which.
  //
  // v0.28.34 — MUST be memoized. Previously a fresh array literal on
  // every render, which made it a new reference each time, which
  // caused the init useEffect (whose deps include markerPlan) to
  // re-run on every render — destroying and recreating the entire
  // MapLibre instance in a tight loop. That was the "openmap layer
  // kept rerendering so fast" the user hit.
  const markerPlan = useMemo(() => {
    const out: { lat: number; lng: number; label?: string; kind: "place" | "from" | "to" }[] = [];
    if (mapPart.route?.from?.lat != null && mapPart.route?.from?.lng != null) {
      out.push({ lat: mapPart.route.from.lat, lng: mapPart.route.from.lng, label: mapPart.route.from.label, kind: "from" });
    }
    if (mapPart.route?.to?.lat != null && mapPart.route?.to?.lng != null) {
      out.push({ lat: mapPart.route.to.lat, lng: mapPart.route.to.lng, label: mapPart.route.destination_label ?? mapPart.route.to.label, kind: "to" });
    }
    if (out.length === 0 && mapPart.place?.lat != null && mapPart.place?.lng != null) {
      out.push({ lat: mapPart.place.lat, lng: mapPart.place.lng, label: mapPart.place.label, kind: "place" });
    }
    return out;
    // Primitive deps only so this recomputes ONLY when the actual
    // marker positions/labels change, not when the enclosing mapPart
    // reference churns.
  }, [
    mapPart.route?.from?.lat,
    mapPart.route?.from?.lng,
    mapPart.route?.from?.label,
    mapPart.route?.to?.lat,
    mapPart.route?.to?.lng,
    mapPart.route?.to?.label,
    mapPart.route?.destination_label,
    mapPart.place?.lat,
    mapPart.place?.lng,
    mapPart.place?.label,
  ]);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    let map: MapLibreMap | null = null;
    try {
      map = new maplibregl.Map({
        container: el,
        style: STYLE_URL,
        center: [coords.lng, coords.lat],
        zoom: coords.zoom,
        // v0.28.32 Stage 1 — sci-fi depth. Default pitch tilts the
        // camera 55° so buildings extrude visibly. A subtle bearing
        // gives an isometric feel. Users can still pan/zoom/rotate
        // freely (MapLibre default gestures include drag-rotate).
        pitch: 55,
        bearing: -18,
        maxPitch: 75,
        attributionControl: { compact: true },
      });
      mapRef.current = map;
      map.on("load", () => {
        // Try tinting default background + water with Travis brand
        // purple. Works best with OpenFreeMap dark style layer ids;
        // failures are silent so the base map still renders.
        try {
          map?.setPaintProperty("background", "background-color", "rgb(10, 8, 18)");
          map?.setPaintProperty("water", "fill-color", "rgba(28, 22, 52, 0.9)");
        } catch {
          /* style layers may differ — ignore */
        }
        // v0.28.32 — 3D building extrusions. Uses the openmaptiles
        // building source-layer that OpenFreeMap ships. Height is
        // interpolated from OSM data (`render_height` or `height`).
        // Fill color subtly gradients from bronze-lavender at ground
        // to lighter brand-purple at top so tall towers feel lit.
        if (map) addBuildingExtrusion(map);
        // v0.28.37 Stage 3 — actual 3D terrain via AWS Open Data
        // Terrarium DEM tiles. MapLibre uses the encoded elevation
        // to displace tile vertices when pitch > 0. Combined with
        // the existing pitch: 55, mountains actually rise.
        if (map) addTerrain(map);
        // v0.28.27 — draw the route geometry as a glowing brand line
        // when the map part carries one. Called after style load so
        // the source + layer add cleanly.
        if (map) addRouteLayer(map, mapPart);
        // v0.28.38 Stage 2 — data overlays (heatmap / polygons /
        // circles). Composable with routes; layers drop in above the
        // basemap but below symbol labels.
        if (map) applyMapOverlays(map, mapPart.overlays);
        // v0.28.39 Stage 5 — hover callouts. When the cursor hovers a
        // 3D building, the layer id changes to interactive and we can
        // fetch OSM features under the pointer for a small tooltip.
        if (map) wireBuildingHoverCallouts(map);
        map?.resize();
      });
      // v0.28.33 — one-shot fallback. The previous handler swapped the
      // style on ANY error event, and each setStyle triggered fresh
      // load errors during initialization, giving an infinite reload
      // loop that showed nothing but the DOM markers. Now we only
      // fall back once, only when the primary style URL itself
      // failed to load, and never in response to tile/expression/
      // source-transient errors that resolve on their own.
      let hasFellBack = false;
      map.on("error", (e) => {
        // Expression evaluation, tile 404s, and layer add errors all
        // bubble through here. They're noise — the map recovers on
        // its own. Log for triage and DO NOT swap the style.
        const errMsg = (e as { error?: { message?: string } })?.error?.message ?? "";
        const isPrimaryStyleFailure =
          !hasFellBack &&
          (errMsg.includes(STYLE_URL) ||
            /style.*(load|fetch|parse)/i.test(errMsg) ||
            /Failed to fetch/i.test(errMsg));
        if (!isPrimaryStyleFailure) {
          if (errMsg) console.debug("[map] transient error (ignored):", errMsg);
          return;
        }
        hasFellBack = true;
        console.warn("[map] primary style unavailable, swapping to raster fallback:", errMsg);
        try {
          map?.setStyle({
            version: 8,
            sources: {
              carto: {
                type: "raster",
                tiles: [
                  "https://a.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}@2x.png",
                  "https://b.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}@2x.png",
                  "https://c.basemaps.cartocdn.com/dark_all/{z}/{x}/{y}@2x.png",
                ],
                tileSize: 256,
                attribution: "© OpenStreetMap contributors © CARTO",
              },
            },
            layers: [{ id: "carto", type: "raster", source: "carto" }],
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
          } as any);
        } catch {
          /* nothing else to try */
        }
      });
      requestAnimationFrame(() => map?.resize());

      // v0.28.34 — marker planting moved to a dedicated sync effect
      // below, keyed on `markerPlan`. Keeps map init decoupled from
      // marker changes.
    } catch (err) {
      console.warn("[map] MapLibre init failed:", err);
    }
    return () => {
      for (const marker of markersRef.current) {
        try { marker.remove(); } catch { /* ignore */ }
      }
      markersRef.current = [];
      try { mapRef.current?.remove(); } catch { /* ignore */ }
      mapRef.current = null;
    };
    // v0.28.34 — deps intentionally exclude markerPlan. The plan is
    // consumed at init to plant markers; if the plan changes
    // afterward, the separate marker-sync effect below rebuilds
    // markers in place without destroying the map. Including
    // markerPlan (memoized or not) risks re-init on every content
    // change, which is what tanked the map render.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [coords.lat, coords.lng, coords.zoom]);

  // v0.28.34 — sync markers when markerPlan changes WITHOUT tearing
  // down the map. Preserves camera state + tile progress across
  // follow-up map updates.
  useEffect(() => {
    const map = mapRef.current;
    if (!map) return;
    for (const marker of markersRef.current) {
      try { marker.remove(); } catch { /* ignore */ }
    }
    markersRef.current = [];
    for (const m of markerPlan) {
      const markerEl = document.createElement("div");
      markerEl.className = `travis-map-marker travis-marker-${m.kind}`;
      // v0.28.43 — the drop-in animation MUST live on an inner
      // element, not on markerEl itself. MapLibre positions markers
      // by writing `element.style.transform = translate(x, y)` on
      // every frame; a CSS `animation` with `fill-mode: both` on the
      // same element overrides that inline transform (animations beat
      // inline styles per the CSS cascade), pinning every marker at
      // translate(0,0) of the map container after 0.65s — the "markers
      // displaced" behavior. Keeping the outer element clean lets
      // MapLibre's positioning stick.
      markerEl.innerHTML = `
        <div class="travis-marker-inner">
          <div class="travis-marker-pulse"></div>
          <div class="travis-marker-dot"></div>
          ${m.label ? `<div class="travis-marker-label">${escapeHtml(m.label)}</div>` : ""}
        </div>
      `;
      // v0.28.41 — explicit "bottom" anchor so the dot sits on the
      // actual coordinate; the label floats above via the CSS
      // `bottom: calc(100% + 10px)` rule.
      const marker = new maplibregl.Marker({ element: markerEl, anchor: "bottom" })
        .setLngLat([m.lng, m.lat])
        .addTo(map);
      markersRef.current.push(marker);
    }
  }, [markerPlan]);

  useEffect(() => {
    if (!mapRef.current) return;
    mapRef.current.flyTo({
      center: [coords.lng, coords.lat],
      zoom: coords.zoom,
      duration: 1200,
      essential: true,
    });
  }, [coords.lat, coords.lng, coords.zoom]);

  // v0.28.27 — reapply the route layer whenever the underlying
  // geometry actually changes (a follow-up turn produced a new
  // map). fitBounds inside addRouteLayer overrides flyTo above when
  // a route is present, giving the "pan to encompass both endpoints"
  // behavior the user asked for.
  //
  // v0.28.34 — the dep was `mapPart` which is a fresh object every
  // render, so this effect used to re-fire continuously and stack
  // route-layer removes/adds against a not-yet-ready map. Use a
  // stable geometry signature instead.
  // v0.28.35 — sig covers both the ORS geometry AND the endpoint
  // coords so the straight-line fallback triggers a re-render when a
  // new route arrives without geometry. v0.28.36 also folds in
  // fetchedGeometry so the upgrade path re-runs addRouteLayer.
  // v0.28.38 — also covers overlays so heatmap/polygon/circle
  // changes re-run the layer sync.
  const geometrySig = JSON.stringify({
    geo: mapPart.route?.geometry_geojson ?? null,
    from: mapPart.route?.from ? [mapPart.route.from.lat, mapPart.route.from.lng] : null,
    to: mapPart.route?.to ? [mapPart.route.to.lat, mapPart.route.to.lng] : null,
    fetched: fetchedGeometry ?? null,
    overlays: mapPart.overlays ?? null,
    intent: mapPart.intent ?? null,
  });

  // v0.28.36 — fetch real road-following path in the background when
  // the LLM didn't include one. On endpoint change we clear any
  // previous fetched geometry so the straight-line renders first,
  // then the real path swaps in when the invoke resolves.
  const routeFromLat = mapPart.route?.from?.lat;
  const routeFromLng = mapPart.route?.from?.lng;
  const routeToLat = mapPart.route?.to?.lat;
  const routeToLng = mapPart.route?.to?.lng;
  const routeProfile = mapPart.route?.profile;
  // v0.28.41 — always fetch the real ORS geometry when we have
  // endpoints, regardless of whether the LLM emitted a
  // geometry_geojson. On live runs the LLM often invents a 2-point
  // LineString between the endpoints (giving a straight line across
  // rivers, buildings, etc.). We now treat any LLM-emitted geometry
  // as a placeholder — the client fetches the road-following path
  // and addRouteLayer picks whichever geometry actually has more
  // than 2 vertices.
  useEffect(() => {
    if (routeFromLat == null || routeFromLng == null || routeToLat == null || routeToLng == null) {
      setPathSource("none");
      setPathErrorReason(null);
      isFetchingRef.current = false;
      return;
    }
    let cancelled = false;
    setFetchedGeometry(null); // clear stale on new endpoints
    setPathSource("loading");
    setPathErrorReason(null);
    isFetchingRef.current = true;
    void (async () => {
      try {
        const { invoke } = await import("@tauri-apps/api/core");
        const result = await invoke<{ geometry: unknown }>("fetch_route_geometry", {
          fromLat: routeFromLat,
          fromLng: routeFromLng,
          toLat: routeToLat,
          toLng: routeToLng,
          profile: routeProfile ?? "driving-car",
        });
        if (cancelled) return;
        if (result?.geometry) {
          setFetchedGeometry(result.geometry);
          // success — the geometrySig effect will commit "cloud"
          // on its next run.
        } else {
          console.warn("[map] fetch_route_geometry returned no geometry");
          setPathErrorReason("no geometry in response");
          setPathSource("straight");
        }
      } catch (e) {
        // v0.28.42 — warn instead of debug so failures land in
        // production console + surface on the badge.
        // v0.28.43 — also expose the reason on the badge so users
        // can see WHY we fell back to the straight line without
        // needing to open DevTools.
        console.warn("[map] fetch_route_geometry failed:", e);
        if (!cancelled) {
          setPathErrorReason(String(e).replace(/^Error:\s*/, "").slice(0, 80));
          setPathSource("straight");
        }
      } finally {
        if (!cancelled) {
          isFetchingRef.current = false;
        }
      }
    })();
    return () => { cancelled = true; };
  }, [routeFromLat, routeFromLng, routeToLat, routeToLng, routeProfile]);
  useEffect(() => {
    if (!mapRef.current) return;
    const m = mapRef.current;
    // v0.28.43 — while a fetch is in flight, DON'T let a "straight"
    // fallback overwrite "loading" in the badge. The straight line
    // still gets drawn as a placeholder, but the badge continues to
    // read "fetching…" until the invoke resolves. Read the ref (not
    // state) because on the initial render the state hasn't
    // propagated yet to this sibling effect's closure.
    const commitSource = (src: "cloud" | "llm" | "straight" | "none") => {
      if (src === "none") return;
      if (src === "straight" && isFetchingRef.current) return;
      setPathSource(src);
    };
    if (m.isStyleLoaded()) {
      const src = addRouteLayer(m, mapPart, fetchedGeometry);
      commitSource(src);
      applyMapOverlays(m, mapPart.overlays);
      maybeFlyAlong(m, mapPart, fetchedGeometry);
    } else {
      m.once("load", () => {
        const src = addRouteLayer(m, mapPart, fetchedGeometry);
        commitSource(src);
        applyMapOverlays(m, mapPart.overlays);
        maybeFlyAlong(m, mapPart, fetchedGeometry);
      });
    }
    // Depend on the stable signature only — mapPart itself is a fresh
    // object every render. mapPart is closed over via the ref-based
    // mapRef; the current geometry is what actually decides re-work.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [geometrySig]);

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

      <div
        className="absolute inset-0 pointer-events-none"
        style={{
          background:
            "radial-gradient(ellipse at 50% -20%, rgba(189, 158, 255, 0.14), transparent 55%), radial-gradient(ellipse at 50% 120%, rgba(124, 92, 255, 0.10), transparent 55%)",
          mixBlendMode: "screen",
        }}
      />
      {/* v0.28.39 Stage 5 — scanline post-processing overlay. A
          repeating horizontal gradient at low opacity gives the map
          a subtle "screen" feel without obscuring detail. Blend
          mode: overlay so light areas stay light and dark ones dark. */}
      <div
        className="absolute inset-0 pointer-events-none travis-map-scanlines"
        aria-hidden
      />

      <motion.div
        initial={{ opacity: 0, y: -8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.36, ease: [0.22, 1, 0.36, 1] }}
        className="absolute top-16 left-4 max-w-md rounded-2xl px-4 py-3 pointer-events-auto"
        style={{
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
              className="text-[10px] uppercase tracking-[0.22em] font-mono mb-1 flex items-center gap-2"
              style={{ color: "rgba(189, 158, 255, 0.90)" }}
            >
              <span>// {mapPart.route ? "route" : "place"}</span>
              {mapPart.route && <PathSourceBadge source={pathSource} />}
            </div>
            {mapPart.route && pathSource === "straight" && pathErrorReason && (
              <div
                className="text-[10px] font-mono mb-1"
                style={{ color: "rgba(255, 155, 155, 0.75)" }}
                title="Why the road-follow fetch fell back to a straight line"
              >
                path fetch: {pathErrorReason}
              </div>
            )}
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

/// v0.28.27 — add the route as a source + line layer if the current
/// map part carries a GeoJSON geometry. Idempotent: removes the
/// previous source/layer first so a follow-up map update morphs the
/// path in place instead of stacking. Also fits the camera to the
/// route bounds so both endpoints land in view.
function addRouteLayer(
  map: MapLibreMap,
  mapPart: MapPart,
  fetchedGeometry?: unknown | null,
): "cloud" | "llm" | "straight" | "none" {
  // v0.28.41 — pick whichever geometry has the MOST vertices. LLM
  // frequently invents a 2-point straight line for geometry_geojson;
  // when the client fetched a real ORS path (usually 30-200 pts),
  // prefer that. Otherwise fall through to the straight-line
  // fallback so the user still sees a connection.
  const llmGeo = mapPart.route?.geometry_geojson as
    | { type?: string; coordinates?: number[][] }
    | undefined;
  const cloudGeo = fetchedGeometry as
    | { type?: string; coordinates?: number[][] }
    | undefined;
  const llmCoords = llmGeo?.coordinates?.length ?? 0;
  const cloudCoords = cloudGeo?.coordinates?.length ?? 0;
  let geo: unknown = null;
  let source: "cloud" | "llm" | "straight" | "none" = "none";
  if (cloudCoords >= 3 && cloudCoords >= llmCoords) {
    geo = cloudGeo;
    source = "cloud";
  } else if (llmCoords >= 3) {
    geo = llmGeo;
    source = "llm";
  }
  console.debug("[map] addRouteLayer:", {
    hasRoute: !!mapPart.route,
    hasFrom: !!mapPart.route?.from,
    hasTo: !!mapPart.route?.to,
    llmCoords,
    cloudCoords,
    source,
  });
  if (!geo && mapPart.route?.from && mapPart.route?.to) {
    const f = mapPart.route.from;
    const t = mapPart.route.to;
    if (typeof f.lng === "number" && typeof f.lat === "number" && typeof t.lng === "number" && typeof t.lat === "number") {
      geo = {
        type: "LineString",
        coordinates: [
          [f.lng, f.lat],
          [t.lng, t.lat],
        ],
      };
      source = "straight";
      console.warn("[map] using straight-line fallback (no geometry available yet)");
    }
  }
  const SRC = "travis-route";
  const LYR = "travis-route-line";
  const GLOW = "travis-route-glow";
  try {
    if (map.getLayer(LYR)) map.removeLayer(LYR);
    if (map.getLayer(GLOW)) map.removeLayer(GLOW);
    if (map.getSource(SRC)) map.removeSource(SRC);
  } catch {
    /* ignore */
  }
  if (!geo || typeof geo !== "object") return source;
  try {
    map.addSource(SRC, {
      type: "geojson",
      data: { type: "Feature", geometry: geo as GeoJSON.Geometry, properties: {} },
    });
    // Wide soft glow underneath.
    map.addLayer({
      id: GLOW,
      type: "line",
      source: SRC,
      layout: { "line-cap": "round", "line-join": "round" },
      paint: {
        "line-color": "rgba(189, 158, 255, 0.35)",
        "line-width": 10,
        "line-blur": 6,
      },
    });
    // Crisp inner line. v0.28.37 Stage 4a — progressive draw. Uses a
    // long dash + gap that starts fully offset (line invisible), then
    // shifts the dash pattern over 1.4s so the line "draws itself"
    // from origin to destination. Ends with a solid line.
    map.addLayer({
      id: LYR,
      type: "line",
      source: SRC,
      layout: { "line-cap": "round", "line-join": "round" },
      paint: {
        "line-color": "rgb(220, 200, 255)",
        "line-width": 3.5,
        // Big dash + big gap. Total pattern length = 30 units in
        // line-widths. dashArray moves left→right via the animation
        // loop below.
        "line-dasharray": [0, 4, 3],
      },
    });
    animateRouteDraw(map, LYR);
    // Fit bounds around the LineString coords.
    const coords = (geo as { coordinates?: number[][] }).coordinates ?? [];
    if (coords.length >= 2) {
      let minLng = coords[0][0];
      let maxLng = coords[0][0];
      let minLat = coords[0][1];
      let maxLat = coords[0][1];
      for (const [lng, lat] of coords) {
        if (lng < minLng) minLng = lng;
        if (lng > maxLng) maxLng = lng;
        if (lat < minLat) minLat = lat;
        if (lat > maxLat) maxLat = lat;
      }
      // v0.28.32 — preserve the sci-fi camera pose (pitch + bearing)
      // when fitting to route bounds. Without these, fitBounds resets
      // to a top-down view and the 3D buildings visually collapse.
      map.fitBounds(
        [
          [minLng, minLat],
          [maxLng, maxLat],
        ],
        {
          padding: 80,
          duration: 1400,
          essential: true,
          pitch: map.getPitch(),
          bearing: map.getBearing(),
        },
      );
    }
  } catch (e) {
    console.warn("[map] route layer add failed:", e);
  }
  return source;
}

/// v0.28.39 Stage 5 — hover callouts on 3D buildings.
///
/// When the cursor enters a building extrusion, we cache the id and
/// pull queryable properties (height, OSM name, kind) via
/// map.queryRenderedFeatures. A single popup follows the cursor
/// showing the tallest matching feature's info. Popup + listeners
/// clean up when the map is torn down.
function wireBuildingHoverCallouts(map: MapLibreMap) {
  const LAYER = "travis-3d-buildings";
  if (!map.getLayer(LAYER)) return;

  const popup = new maplibregl.Popup({
    closeButton: false,
    closeOnClick: false,
    className: "travis-map-popup",
    maxWidth: "240px",
    offset: 12,
  });

  const onMouseMove = (e: maplibregl.MapMouseEvent) => {
    const features = map.queryRenderedFeatures(e.point, { layers: [LAYER] });
    if (!features.length) {
      popup.remove();
      map.getCanvas().style.cursor = "";
      return;
    }
    // Pick the tallest feature under the pointer (most likely what
    // the user is aiming at).
    let best = features[0];
    for (const f of features) {
      const h = (f.properties as { render_height?: number; height?: number }).render_height
        ?? (f.properties as { render_height?: number; height?: number }).height
        ?? 0;
      const bestH = (best.properties as { render_height?: number; height?: number }).render_height
        ?? (best.properties as { render_height?: number; height?: number }).height
        ?? 0;
      if (h > bestH) best = f;
    }
    const p = best.properties as {
      render_height?: number;
      height?: number;
      name?: string;
      class?: string;
    };
    const height = p.render_height ?? p.height;
    const name = p.name ?? p.class ?? "building";
    const heightHtml = typeof height === "number"
      ? `<div class="travis-map-popup-meta">${Math.round(height)} m</div>`
      : "";
    popup
      .setLngLat(e.lngLat)
      .setHTML(
        `<div class="travis-map-popup-inner">
          <div class="travis-map-popup-kind">// structure</div>
          <div class="travis-map-popup-name">${escapeHtml(name)}</div>
          ${heightHtml}
        </div>`,
      )
      .addTo(map);
    map.getCanvas().style.cursor = "help";
  };
  const onMouseLeave = () => {
    popup.remove();
    map.getCanvas().style.cursor = "";
  };
  map.on("mousemove", LAYER, onMouseMove);
  map.on("mouseleave", LAYER, onMouseLeave);
}

/// v0.28.38 Stage 2 — apply data overlays on the map.
///
/// Idempotent: removes any prior travis-overlay-* layers/sources
/// before adding new ones so follow-up turns replace the previous
/// overlay set rather than stacking. Overlays are keyed by index —
/// order in the array determines draw order (later = on top).
function applyMapOverlays(map: MapLibreMap, overlays?: MapOverlay[]) {
  const layers = map.getStyle().layers ?? [];
  const sources = Object.keys(map.getStyle().sources ?? {});
  for (const layer of layers) {
    if (layer.id.startsWith("travis-overlay-")) {
      try { map.removeLayer(layer.id); } catch { /* ignore */ }
    }
  }
  for (const sid of sources) {
    if (sid.startsWith("travis-overlay-")) {
      try { map.removeSource(sid); } catch { /* ignore */ }
    }
  }
  if (!overlays || overlays.length === 0) return;

  // Insert overlays BELOW the first symbol layer so map labels stay
  // legible on top of them.
  let beforeId: string | undefined;
  for (const layer of layers) {
    if (layer.type === "symbol") { beforeId = layer.id; break; }
  }

  overlays.forEach((ov, i) => {
    try {
      if (ov.kind === "heatmap") applyHeatmap(map, ov, i, beforeId);
      else if (ov.kind === "polygons") applyPolygons(map, ov, i, beforeId);
      else if (ov.kind === "circles") applyCircles(map, ov, i, beforeId);
      else if (ov.kind === "isochrone") void applyIsochrone(map, ov, i, beforeId);
    } catch (e) {
      console.warn(`[map] overlay ${i} (${ov.kind}) add failed:`, e);
    }
  });
}

/// v0.28.40 — isochrone overlay. Async because it hits the cloud
/// /maps/reach endpoint (through the Rust fetch_isochrones command).
/// Each minute cutoff becomes a nested polygon layer: larger cutoffs
/// sit underneath so nested contours read cleanly. Colors gradient
/// from bright violet (short reach) toward faded lavender (long reach).
async function applyIsochrone(
  map: MapLibreMap,
  ov: Extract<MapOverlay, { kind: "isochrone" }>,
  i: number,
  beforeId?: string,
) {
  const SRC = `travis-overlay-isochrone-${i}`;
  const FILL = `travis-overlay-isochrone-${i}-fill`;
  const OUTLINE = `travis-overlay-isochrone-${i}-outline`;
  try {
    const { invoke } = await import("@tauri-apps/api/core");
    const result = await invoke<{
      profile: string;
      features: { minutes: number; areaKm2: number; geometry: unknown }[];
    }>("fetch_isochrones", {
      centerLat: ov.center.lat,
      centerLng: ov.center.lng,
      minutes: ov.minutes,
      profile: ov.profile ?? "driving-car",
    });
    // Sort descending so largest ring renders first (bottom), then
    // smaller rings paint on top. Nested contours read at a glance.
    const features = [...result.features].sort((a, b) => b.minutes - a.minutes);
    if (features.length === 0) return;
    if (map.getLayer(FILL)) map.removeLayer(FILL);
    if (map.getLayer(OUTLINE)) map.removeLayer(OUTLINE);
    if (map.getSource(SRC)) map.removeSource(SRC);

    const maxMin = features[0].minutes || 1;
    const feats = features.map((f) => ({
      type: "Feature" as const,
      // eslint-disable-next-line @typescript-eslint/no-explicit-any
      geometry: f.geometry as any,
      properties: {
        minutes: f.minutes,
        // Small cutoff = darker/more saturated; large = paler.
        color: colorForCutoff(f.minutes, maxMin),
        outline: outlineForCutoff(f.minutes, maxMin),
      },
    }));
    map.addSource(SRC, { type: "geojson", data: { type: "FeatureCollection", features: feats } });
    map.addLayer(
      {
        id: FILL,
        type: "fill",
        source: SRC,
        paint: {
          "fill-color": ["get", "color"],
          "fill-opacity": 0.55,
        },
      },
      beforeId,
    );
    map.addLayer(
      {
        id: OUTLINE,
        type: "line",
        source: SRC,
        paint: {
          "line-color": ["get", "outline"],
          "line-width": 1.5,
        },
      },
      beforeId,
    );
  } catch (e) {
    console.warn(`[map] isochrone overlay ${i} fetch/render failed:`, e);
  }
}

function colorForCutoff(minutes: number, maxMin: number): string {
  const t = Math.max(0, Math.min(1, minutes / maxMin));
  // Bright violet at small cutoffs → soft lavender at large.
  const hue = 268 - t * 6;
  const sat = 80 - t * 25;
  const light = 55 + t * 22;
  const alpha = 0.25 + (1 - t) * 0.28;
  return `hsla(${hue}, ${sat}%, ${light}%, ${alpha})`;
}
function outlineForCutoff(minutes: number, maxMin: number): string {
  const t = Math.max(0, Math.min(1, minutes / maxMin));
  return `hsla(${268 - t * 6}, ${90 - t * 15}%, ${70 + t * 15}%, 0.85)`;
}

function applyHeatmap(
  map: MapLibreMap,
  ov: Extract<MapOverlay, { kind: "heatmap" }>,
  i: number,
  beforeId?: string,
) {
  const SRC = `travis-overlay-heatmap-${i}`;
  const LYR = `travis-overlay-heatmap-${i}-layer`;
  const features = ov.points.map((p) => ({
    type: "Feature" as const,
    geometry: { type: "Point" as const, coordinates: [p.lng, p.lat] },
    properties: { weight: typeof p.weight === "number" ? p.weight : 1 },
  }));
  map.addSource(SRC, {
    type: "geojson",
    data: { type: "FeatureCollection", features },
  });
  map.addLayer(
    {
      id: LYR,
      type: "heatmap",
      source: SRC,
      paint: {
        "heatmap-weight": ["get", "weight"],
        "heatmap-intensity": ["interpolate", ["linear"], ["zoom"], 0, 0.4, 15, 3],
        "heatmap-radius": ["interpolate", ["linear"], ["zoom"], 0, 8, 15, 55],
        "heatmap-opacity": 0.75,
        "heatmap-color": [
          "interpolate", ["linear"], ["heatmap-density"],
          0,   "rgba(38, 30, 62, 0)",
          0.2, "rgba(78, 58, 122, 0.55)",
          0.4, "rgba(126, 90, 190, 0.7)",
          0.6, "rgba(189, 158, 255, 0.82)",
          0.8, "rgba(240, 200, 255, 0.9)",
          1,   "rgba(255, 230, 255, 0.95)",
        ],
      },
    },
    beforeId,
  );
}

function applyPolygons(
  map: MapLibreMap,
  ov: Extract<MapOverlay, { kind: "polygons" }>,
  i: number,
  beforeId?: string,
) {
  const SRC = `travis-overlay-poly-${i}`;
  const FILL = `travis-overlay-poly-${i}-fill`;
  const OUTLINE = `travis-overlay-poly-${i}-outline`;
  const features = ov.features
    .filter((f) => f.points.length >= 3)
    .map((f, idx) => ({
      type: "Feature" as const,
      geometry: {
        type: "Polygon" as const,
        coordinates: [
          [...f.points.map((p) => [p.lng, p.lat] as [number, number]), [f.points[0].lng, f.points[0].lat] as [number, number]],
        ],
      },
      properties: {
        color: f.color ?? "rgba(189, 158, 255, 0.25)",
        outline: f.color ?? "rgba(220, 210, 255, 0.75)",
        label: f.label ?? `region-${idx}`,
      },
    }));
  map.addSource(SRC, { type: "geojson", data: { type: "FeatureCollection", features } });
  map.addLayer(
    {
      id: FILL,
      type: "fill",
      source: SRC,
      paint: {
        "fill-color": ["get", "color"],
        "fill-outline-color": "rgba(220, 210, 255, 0.7)",
      },
    },
    beforeId,
  );
  map.addLayer(
    {
      id: OUTLINE,
      type: "line",
      source: SRC,
      paint: {
        "line-color": ["get", "outline"],
        "line-width": 1.5,
      },
    },
    beforeId,
  );
}

function applyCircles(
  map: MapLibreMap,
  ov: Extract<MapOverlay, { kind: "circles" }>,
  i: number,
  beforeId?: string,
) {
  const SRC = `travis-overlay-circle-${i}`;
  const FILL = `travis-overlay-circle-${i}-fill`;
  const OUTLINE = `travis-overlay-circle-${i}-outline`;
  const features = ov.circles.map((c, idx) => ({
    type: "Feature" as const,
    geometry: geodesicCirclePolygon(c.lat, c.lng, c.radius_km),
    properties: {
      color: c.color ?? "rgba(189, 158, 255, 0.18)",
      outline: c.color ?? "rgba(220, 200, 255, 0.75)",
      label: c.label ?? `circle-${idx}`,
    },
  }));
  map.addSource(SRC, { type: "geojson", data: { type: "FeatureCollection", features } });
  map.addLayer(
    {
      id: FILL,
      type: "fill",
      source: SRC,
      paint: {
        "fill-color": ["get", "color"],
      },
    },
    beforeId,
  );
  map.addLayer(
    {
      id: OUTLINE,
      type: "line",
      source: SRC,
      paint: {
        "line-color": ["get", "outline"],
        "line-width": 1.5,
        "line-dasharray": [2, 2],
      },
    },
    beforeId,
  );
}

/// Approximate a geodesic circle (constant km radius from a lat/lng
/// center) as a 64-vertex Polygon. Accurate enough for on-screen
/// overlay purposes; a Turf-based version would be more precise for
/// tiny radii near the poles.
function geodesicCirclePolygon(lat: number, lng: number, radiusKm: number, steps = 64): { type: "Polygon"; coordinates: number[][][] } {
  const coords: number[][] = [];
  const R = 6371; // Earth radius km
  const dLat = (radiusKm / R) * (180 / Math.PI);
  const dLng = dLat / Math.cos((lat * Math.PI) / 180);
  for (let i = 0; i <= steps; i++) {
    const t = (i / steps) * Math.PI * 2;
    coords.push([lng + dLng * Math.cos(t), lat + dLat * Math.sin(t)]);
  }
  return { type: "Polygon", coordinates: [coords] };
}

/// v0.28.40 Stage 4b — fly-along camera. Animates the camera along
/// the route geometry in a low-altitude following shot. Escape (or a
/// new mapPart) cancels. Duration scales with route length: shorter
/// hops feel snappy, longer routes get more air time.
///
/// Skips silently when there's no geometry or no intent. Bearing is
/// updated per-segment so the camera "faces the direction of travel".
let flyAlongAbort: (() => void) | null = null;

function maybeFlyAlong(map: MapLibreMap, mapPart: MapPart, fetchedGeometry?: unknown | null) {
  if (mapPart.intent !== "fly_along") return;
  const geo = (mapPart.route?.geometry_geojson ?? fetchedGeometry) as
    | { type?: string; coordinates?: number[][] }
    | null
    | undefined;
  const coords = geo?.coordinates;
  if (!coords || coords.length < 2) return;

  // Cancel any prior in-flight tour first.
  if (flyAlongAbort) { flyAlongAbort(); flyAlongAbort = null; }

  const savedPitch = map.getPitch();
  const savedBearing = map.getBearing();
  const savedCenter = map.getCenter();
  const savedZoom = map.getZoom();

  const tourPitch = 68;
  const tourZoom = 15.2;
  // Duration scales: 5s min, 18s max, ~1s per 8 waypoints in between.
  const durationMs = Math.max(5000, Math.min(18000, coords.length * 120));
  const start = performance.now();
  let cancelled = false;

  const onKey = (e: KeyboardEvent) => {
    if (e.key === "Escape") stop(true);
  };
  const onClick = () => stop(true);
  const onDrag = () => stop(true);
  const stop = (restore: boolean) => {
    if (cancelled) return;
    cancelled = true;
    window.removeEventListener("keydown", onKey);
    map.off("click", onClick);
    map.off("dragstart", onDrag);
    if (restore) {
      map.easeTo({
        center: [savedCenter.lng, savedCenter.lat],
        zoom: savedZoom,
        pitch: savedPitch,
        bearing: savedBearing,
        duration: 900,
        essential: true,
      });
    }
    flyAlongAbort = null;
  };
  flyAlongAbort = () => stop(false);
  window.addEventListener("keydown", onKey);
  map.once("click", onClick);
  map.once("dragstart", onDrag);

  const bearingBetween = (a: number[], b: number[]): number => {
    const [lng1, lat1] = a;
    const [lng2, lat2] = b;
    const φ1 = (lat1 * Math.PI) / 180;
    const φ2 = (lat2 * Math.PI) / 180;
    const λ1 = (lng1 * Math.PI) / 180;
    const λ2 = (lng2 * Math.PI) / 180;
    const y = Math.sin(λ2 - λ1) * Math.cos(φ2);
    const x = Math.cos(φ1) * Math.sin(φ2) - Math.sin(φ1) * Math.cos(φ2) * Math.cos(λ2 - λ1);
    const brng = (Math.atan2(y, x) * 180) / Math.PI;
    return (brng + 360) % 360;
  };

  const tick = () => {
    if (cancelled) return;
    const now = performance.now();
    const t = Math.min(1, (now - start) / durationMs);
    // Ease in/out so the camera settles at both ends.
    const eased = t < 0.5
      ? 2 * t * t
      : 1 - Math.pow(-2 * t + 2, 2) / 2;
    const idx = Math.min(coords.length - 2, Math.floor(eased * (coords.length - 1)));
    const frac = (eased * (coords.length - 1)) - idx;
    const a = coords[idx];
    const b = coords[idx + 1];
    const lng = a[0] + (b[0] - a[0]) * frac;
    const lat = a[1] + (b[1] - a[1]) * frac;
    const bearing = bearingBetween(a, b);
    map.jumpTo({
      center: [lng, lat],
      zoom: tourZoom,
      pitch: tourPitch,
      bearing,
    });
    if (t < 1) requestAnimationFrame(tick);
    else stop(true);
  };
  requestAnimationFrame(tick);
}

/// v0.28.37 Stage 4a — progressive route draw.
///
/// Cycles a `line-dasharray` from an all-invisible pattern through to
/// solid over ~1.4s. Uses requestAnimationFrame + eased-out timing so
/// the line grows fast at first then settles, mirroring the way
/// Google Maps' route reveal feels. Silently no-ops if the layer
/// disappears mid-animation (route swap during a follow-up).
function animateRouteDraw(map: MapLibreMap, layerId: string) {
  const start = performance.now();
  const duration = 1400;
  const ease = (t: number) => 1 - Math.pow(1 - t, 3); // easeOutCubic
  const tick = () => {
    if (!map.getLayer(layerId)) return;
    const now = performance.now();
    const t = Math.min(1, (now - start) / duration);
    const eased = ease(t);
    // Interpolate a dash pattern: at t=0, huge gap (line invisible);
    // at t=1, solid line. Values are in line-widths.
    // dashArray of [dashLen, gapLen] — as gap shrinks the line fills.
    const dashLen = 1;
    const gapLen = Math.max(0.001, 60 * (1 - eased));
    try {
      map.setPaintProperty(layerId, "line-dasharray", [dashLen, gapLen]);
    } catch {
      return;
    }
    if (t < 1) requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}

/// v0.28.37 Stage 3 — 3D terrain.
///
/// Adds a raster-dem source (AWS Open Data Terrarium tiles at
/// s3.amazonaws.com/elevation-tiles-prod, terrarium encoding) and
/// enables MapLibre's setTerrain(). Also drops a hillshade layer for
/// subtle shading. Combined with the pitch: 55 default, mountains,
/// coastal cliffs, and river valleys all rise visibly.
///
/// Bail silently if the source add fails — we want the base map to
/// keep working on flaky networks.
function addTerrain(map: MapLibreMap) {
  const SRC = "travis-dem";
  try {
    if (!map.getSource(SRC)) {
      map.addSource(SRC, {
        type: "raster-dem",
        tiles: [
          "https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{z}/{x}/{y}.png",
        ],
        tileSize: 256,
        encoding: "terrarium",
        maxzoom: 15,
        attribution: "Elevation: AWS Open Data",
        // eslint-disable-next-line @typescript-eslint/no-explicit-any
      } as any);
    }
    // Vertical exaggeration 1.4 makes rolling terrain read as
    // "there's actual topography here" without cartoon spikes.
    map.setTerrain({ source: SRC, exaggeration: 1.4 });

    const HL = "travis-hillshade";
    if (!map.getLayer(HL)) {
      // Insert BEFORE the first symbol layer so labels sit above.
      let beforeId: string | undefined;
      for (const layer of map.getStyle().layers ?? []) {
        if (layer.type === "symbol") { beforeId = layer.id; break; }
      }
      map.addLayer(
        {
          id: HL,
          source: SRC,
          type: "hillshade",
          paint: {
            "hillshade-shadow-color": "rgb(0, 0, 20)",
            "hillshade-highlight-color": "rgba(200, 180, 250, 0.35)",
            "hillshade-accent-color": "rgba(120, 80, 180, 0.5)",
            "hillshade-illumination-direction": 315,
            "hillshade-exaggeration": 0.55,
          },
        },
        beforeId,
      );
    }
  } catch (e) {
    console.warn("[map] terrain setup failed:", e);
  }
}

/// v0.28.32 Stage 1 — 3D building extrusions.
///
/// OpenFreeMap tiles carry a `building` source layer with an OSM
/// `render_height` attribute (numeric, meters). We add a
/// `fill-extrusion` layer above the base style. Color interpolates
/// with height: short structures stay ground-tone; towers grade
/// toward a brighter lavender to feel lit. Opacity 0.82 so the
/// glowing route line stays visible when it passes through dense
/// urban blocks.
///
/// Silently no-ops when the style is the raster CartoDB fallback
/// (no vector building layer) or when the layer already exists.
function addBuildingExtrusion(map: MapLibreMap) {
  const LAYER = "travis-3d-buildings";
  try {
    if (map.getLayer(LAYER)) return;
    // OpenFreeMap uses the `openmaptiles` vector source; if it's
    // missing (raster fallback path), bail.
    if (!map.getSource("openmaptiles")) return;

    // Find the first symbol layer so we insert extrusions BELOW
    // labels/road-names. Labels then float on top of the buildings.
    let beforeId: string | undefined;
    for (const layer of map.getStyle().layers ?? []) {
      if (layer.type === "symbol") { beforeId = layer.id; break; }
    }

    map.addLayer(
      {
        id: LAYER,
        source: "openmaptiles",
        "source-layer": "building",
        type: "fill-extrusion",
        minzoom: 13.5,
        paint: {
          "fill-extrusion-color": [
            "interpolate",
            ["linear"],
            ["coalesce", ["get", "render_height"], ["get", "height"], 0],
            0,   "rgba(38, 30, 62, 0.9)",
            10,  "rgba(52, 40, 82, 0.9)",
            30,  "rgba(78, 58, 122, 0.92)",
            80,  "rgba(126, 90, 190, 0.92)",
            200, "rgba(180, 140, 240, 0.95)",
          ],
          "fill-extrusion-height": [
            "coalesce",
            ["get", "render_height"],
            ["get", "height"],
            8,
          ],
          "fill-extrusion-base": [
            "coalesce",
            ["get", "render_min_height"],
            ["get", "min_height"],
            0,
          ],
          "fill-extrusion-opacity": 0.82,
        },
      },
      beforeId,
    );
  } catch (e) {
    console.warn("[map] 3D extrusion setup failed:", e);
  }
}

function escapeHtml(s: string): string {
  return s.replace(/[&<>"']/g, (c) => {
    switch (c) {
      case "&": return "&amp;";
      case "<": return "&lt;";
      case ">": return "&gt;";
      case '"': return "&quot;";
      case "'": return "&#39;";
      default: return c;
    }
  });
}

/// v0.28.42 — visible pip on the info card so the user (and I) can
/// see at a glance whether the line drawn is the real ORS road path,
/// the LLM-supplied one, a straight fallback, or an in-flight fetch.
function PathSourceBadge({ source }: { source: "cloud" | "llm" | "straight" | "loading" | "none" }) {
  if (source === "none") return null;
  const config = {
    cloud:    { color: "rgb(140, 230, 175)", label: "real path" },
    llm:      { color: "rgb(255, 210, 130)", label: "llm path"  },
    straight: { color: "rgb(255, 155, 155)", label: "straight"  },
    loading:  { color: "rgba(236, 236, 241, 0.65)", label: "fetching…" },
  }[source];
  return (
    <span
      style={{
        display: "inline-flex",
        alignItems: "center",
        gap: 4,
        padding: "1px 6px",
        borderRadius: 4,
        border: `1px solid ${config.color}80`,
        background: `${config.color}22`,
        color: config.color,
        fontSize: 8.5,
        letterSpacing: "0.14em",
      }}
      title={`Route geometry source: ${config.label}`}
    >
      <span
        style={{
          width: 5,
          height: 5,
          borderRadius: "50%",
          background: config.color,
          boxShadow: `0 0 6px ${config.color}`,
        }}
      />
      {config.label}
    </span>
  );
}

function BrandMarkerStyles() {
  return (
    <style>
      {`
        .travis-map-marker {
          position: relative;
          width: 32px;
          height: 32px;
          /* v0.28.43 — no animation and no transform on the marker
             root. MapLibre owns this element's transform for
             positioning; anything we add here fights it. Visual
             flourishes live on .travis-marker-inner instead. */
          z-index: 5;
          pointer-events: auto;
        }
        .travis-marker-inner {
          position: absolute;
          inset: 0;
          display: flex;
          align-items: center;
          justify-content: center;
          /* v0.28.37 — endpoint markers drop in from above the target
             with a slight bounce. Read as "landing on the map"
             instead of just appearing. v0.28.43 — moved off the root
             so MapLibre's positioning transform is preserved. */
          animation: travis-marker-drop 0.65s cubic-bezier(0.32, 1.4, 0.5, 1) both;
        }
        @keyframes travis-marker-drop {
          0%   { transform: translateY(-40px) scale(0.4); opacity: 0; }
          70%  { transform: translateY(4px)   scale(1.15); opacity: 1; }
          100% { transform: translateY(0)     scale(1);    opacity: 1; }
        }
        @media (prefers-reduced-motion: reduce) {
          .travis-marker-inner { animation: none; opacity: 1 !important; }
        }
        .travis-marker-dot {
          width: 18px;
          height: 18px;
          border-radius: 50%;
          background: radial-gradient(circle at 30% 30%, rgb(230, 215, 255), rgb(160, 120, 240));
          border: 2.5px solid rgba(255, 255, 255, 0.95);
          box-shadow:
            0 0 0 1.5px rgba(189, 158, 255, 0.45),
            0 0 26px 6px rgba(189, 158, 255, 0.72),
            0 2px 8px 0 rgba(0, 0, 0, 0.55);
          position: relative;
          z-index: 2;
        }
        /* v0.28.31 — endpoint differentiation: green start (from),
           warm accent end (to). Keeps the brand purple for standalone
           place cards. */
        .travis-marker-from .travis-marker-dot {
          background: radial-gradient(circle at 30% 30%, rgb(210, 255, 220), rgb(120, 220, 155));
          box-shadow:
            0 0 0 1px rgba(140, 230, 175, 0.45),
            0 0 20px 4px rgba(140, 230, 175, 0.55);
        }
        .travis-marker-from .travis-marker-pulse {
          background: rgba(140, 230, 175, 0.32);
        }
        .travis-marker-to .travis-marker-dot {
          background: radial-gradient(circle at 30% 30%, rgb(255, 235, 210), rgb(255, 190, 130));
          box-shadow:
            0 0 0 1px rgba(255, 210, 130, 0.5),
            0 0 20px 4px rgba(255, 210, 130, 0.6);
        }
        .travis-marker-to .travis-marker-pulse {
          background: rgba(255, 210, 130, 0.32);
        }
        .travis-marker-pulse {
          position: absolute;
          inset: 0;
          border-radius: 50%;
          background: rgba(189, 158, 255, 0.32);
          animation: travis-marker-pulse 2.2s cubic-bezier(0.22, 1, 0.36, 1) infinite;
          z-index: 1;
        }
        /* v0.28.31 — persistent endpoint label. Offset above the dot
           so the pulse ring doesn't clip it. Small backdrop for
           legibility against light or dark map tiles. */
        .travis-marker-label {
          position: absolute;
          bottom: calc(100% + 10px);
          left: 50%;
          transform: translateX(-50%);
          padding: 3px 8px;
          border-radius: 6px;
          font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
          font-size: 10.5px;
          letter-spacing: 0.03em;
          white-space: nowrap;
          color: rgba(240, 240, 246, 0.95);
          background: rgba(14, 12, 20, 0.82);
          backdrop-filter: blur(6px);
          border: 1px solid rgba(189, 158, 255, 0.35);
          box-shadow: 0 4px 14px -6px rgba(0, 0, 0, 0.6);
          pointer-events: none;
          z-index: 3;
        }
        .travis-marker-from .travis-marker-label {
          border-color: rgba(140, 230, 175, 0.45);
        }
        .travis-marker-to .travis-marker-label {
          border-color: rgba(255, 210, 130, 0.5);
        }
        @keyframes travis-marker-pulse {
          0%   { transform: scale(0.6); opacity: 0.65; }
          70%  { transform: scale(2.2); opacity: 0;    }
          100% { transform: scale(2.2); opacity: 0;    }
        }
        @media (prefers-reduced-motion: reduce) {
          .travis-marker-pulse { animation: none; opacity: 0; }
        }
        /* v0.28.39 — scanlines. 3px band spacing keeps them visible
           without banding hard. Blend + opacity keeps map legibility. */
        .travis-map-scanlines {
          background-image: repeating-linear-gradient(
            to bottom,
            rgba(189, 158, 255, 0.05) 0px,
            rgba(189, 158, 255, 0.05) 1px,
            transparent 1px,
            transparent 3px
          );
          mix-blend-mode: overlay;
          opacity: 0.55;
        }
        /* Hover callout popup. */
        .travis-map-popup .maplibregl-popup-content {
          background: rgba(14, 12, 20, 0.9);
          border: 1px solid rgba(189, 158, 255, 0.4);
          border-radius: 10px;
          padding: 8px 10px;
          box-shadow: 0 6px 20px -8px rgba(0, 0, 0, 0.65);
          backdrop-filter: blur(8px);
        }
        .travis-map-popup .maplibregl-popup-tip {
          border-top-color: rgba(189, 158, 255, 0.4) !important;
        }
        .travis-map-popup-kind {
          font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
          font-size: 9.5px;
          letter-spacing: 0.22em;
          text-transform: uppercase;
          color: rgba(189, 158, 255, 0.85);
          margin-bottom: 2px;
        }
        .travis-map-popup-name {
          font-size: 13px;
          color: rgba(240, 240, 246, 0.95);
          letter-spacing: 0.005em;
        }
        .travis-map-popup-meta {
          font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
          font-size: 11px;
          color: rgba(236, 236, 241, 0.7);
          margin-top: 2px;
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
