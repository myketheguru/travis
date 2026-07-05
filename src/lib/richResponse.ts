/**
 * Rich response contract — TypeScript mirror of src-tauri/src/rich_response.rs.
 *
 * The LLM emits `{ parts: MessagePart[] }`. The chat renderer routes
 * each part to its card component by `kind`. Cards are the visual
 * layer over Travis's tool outputs — a map response renders a map,
 * a doc_ref opens the doc viewer, etc.
 *
 * See INTERFACE.md in the cloud repo for the design principles.
 */

export interface RichResponse {
  parts: MessagePart[];
}

export type MessagePart =
  | TextPart
  | MapPart
  | DocRefPart
  | EntityPart
  | CalendarPart
  | T2tConvoPart
  | ActionProposalPart
  | ListPart
  | ChartPart
  | MediaPart;

export interface BasePart {
  kind: MessagePart["kind"];
  narration?: string;
}

export interface TextPart extends BasePart {
  kind: "text";
  markdown: string;
}

export interface MapPart extends BasePart {
  kind: "map";
  route: MapRoute;
}

export interface DocRefPart extends BasePart {
  kind: "doc_ref";
  document_id: number;
  snippet?: string;
}

export interface EntityPart extends BasePart {
  kind: "entity";
  entity_id: number;
  display_name: string;
  facts?: unknown;
}

export interface CalendarPart extends BasePart {
  kind: "calendar";
  window_start: string;
  window_end: string;
  events: CalendarEvent[];
}

export interface T2tConvoPart extends BasePart {
  kind: "t2t_convo";
  query_id: string;
  from_display: string;
  to_display: string;
  question: string;
  drafted_response?: string;
  final_response?: string;
  state: T2tConvoState;
}

export interface ActionProposalPart extends BasePart {
  kind: "action_proposal";
  action_kind: string;
  preview_title: string;
  preview_body: string;
  input: unknown;
}

export interface ListPart extends BasePart {
  kind: "list";
  title: string;
  rows: ListRow[];
}

export interface ChartPart extends BasePart {
  kind: "chart";
  chart_kind: "sparkline" | "bar" | "pie";
  series: ChartSeries[];
}

export interface MediaPart extends BasePart {
  kind: "media";
  url: string;
  media_kind: "image" | "video" | "audio";
  caption?: string;
}

// ─── Sub-payloads ─────────────────────────────────────────────────

export interface MapRoute {
  from: LatLng;
  to: LatLng;
  distance_meters: number;
  duration_seconds: number;
  profile?: "driving-car" | "cycling-regular" | "foot-walking";
  destination_label?: string;
  geometry_geojson?: unknown;
}

export interface LatLng {
  lat: number;
  lng: number;
}

export interface CalendarEvent {
  id: string;
  title: string;
  start: string;
  end: string;
  location?: string;
  attendees?: string[];
}

export type T2tConvoState =
  | "sending"
  | "delivered"
  | "considering"
  | "drafted"
  | "answered"
  | "declined";

export interface ListRow {
  id: string;
  label: string;
  meta?: string;
  actions?: RowAction[];
}

export interface RowAction {
  kind: "primary" | "secondary";
  label: string;
  verb: string;
}

export interface ChartSeries {
  label: string;
  points: number[];
}

// ─── Parser ──────────────────────────────────────────────────────

/** Try to parse an LLM response string as a RichResponse. Returns
 *  null if it's not the typed shape — caller should treat as plain
 *  text. */
export function parseRichResponse(raw: string): RichResponse | null {
  try {
    // The LLM may wrap in a ```json fence or leading prose. Try to
    // extract a top-level JSON object first.
    const jsonMatch = raw.match(/\{[\s\S]*\}/);
    if (!jsonMatch) return null;
    const obj = JSON.parse(jsonMatch[0]) as { parts?: unknown };
    if (!Array.isArray(obj.parts)) return null;
    // Cheap validation: every entry has a `kind` string.
    for (const p of obj.parts) {
      if (typeof (p as { kind?: unknown }).kind !== "string") return null;
    }
    return obj as RichResponse;
  } catch {
    return null;
  }
}

/** True if this part kind carries visual content the renderer draws
 *  as a full card (not just a text line). Used to decide narration
 *  playback and layout. */
export function isRichPart(part: MessagePart): boolean {
  return part.kind !== "text";
}
