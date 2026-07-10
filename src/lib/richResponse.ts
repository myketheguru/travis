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
  /** Shell 7 — shape-shifting resume. When Travis wants to bring
   *  archived cards back to the canvas (e.g., user asked "bring back
   *  what I was doing on the CX hire"), include the message IDs here.
   *  The renderer marks them as resurrected in the card lifecycle
   *  store, overriding the 24h archival. */
  resurrect_ids?: string[];
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
  | MediaPart
  | ThreadPart
  // v0.28.28 — Phase A rich response types
  | TablePart
  | KeyValuePart
  | CalloutPart
  | QuickReplyPart
  | StepperPart
  | CodeSnippetPart
  | ContactCardPart;

export interface BasePart {
  kind: MessagePart["kind"];
  narration?: string;
  /// v0.28.14 — response channel. When "voice", the frontend speaks
  /// `narration` via Piper. When "chat" (default), text renders only.
  /// When "silent", the part is not rendered — useful for internal
  /// acks. The LLM decides based on ambient/meeting context.
  channel?: "voice" | "chat" | "silent";
}

export interface TextPart extends BasePart {
  kind: "text";
  markdown: string;
}

export interface MapPart extends BasePart {
  kind: "map";
  /// v0.28.2 — route is optional. When the user asks about a PLACE
  /// (e.g. "show me a map of Lagos") the LLM emits a map part with
  /// just `place` populated; when they ask about a ROUTE (A to B)
  /// the LLM emits `route`. Both are valid map surfaces.
  route?: MapRoute;
  place?: MapPlace;
}

export interface MapPlace {
  label: string;
  lat?: number;
  lng?: number;
  region?: string;
  country?: string;
  /// Short human descriptor for the place ("City in Nigeria",
  /// "Neighborhood in Brooklyn").
  descriptor?: string;
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

/** A first-class thread. Long-running open-ended context. Collapses
 *  to a summary in the canvas; expands to full scrollable chat with
 *  embedded sub-cards + a thread-local composer. */
export interface ThreadPart extends BasePart {
  kind: "thread";
  thread_id?: string;
  title: string;
  summary?: string;
  turns: ThreadTurn[];
  pinned?: boolean;
}

export interface ThreadTurn {
  author: "user" | "travis";
  parts: MessagePart[];
}

// ─── Phase A (v0.28.28) new part types ──────────────────────────

/** Structured tabular data. Cell values are rendered by column type;
 *  numbers right-align, currency/date formats when set. */
export interface TablePart extends BasePart {
  kind: "table";
  title?: string;
  columns: TableColumn[];
  rows: (string | number | null)[][];
  /** Optional per-row link/action for "click through". */
  row_actions?: RowAction[];
}
export interface TableColumn {
  key: string;
  label: string;
  align?: "left" | "right" | "center";
  format?: "text" | "number" | "currency" | "date" | "duration" | "percent";
  width?: number;
}

/** Compact strip of labeled facts. Better than a Table for a single
 *  entity's attributes. */
export interface KeyValuePart extends BasePart {
  kind: "keyvalue";
  title?: string;
  items: { label: string; value: string; hint?: string }[];
}

/** Semantic message box — info / warn / success / error. */
export interface CalloutPart extends BasePart {
  kind: "callout";
  severity: "info" | "warn" | "success" | "error";
  title?: string;
  body: string;
}

/** Pill options user can click to answer without typing. */
export interface QuickReplyPart extends BasePart {
  kind: "quickreply";
  prompt?: string;
  options: QuickReplyOption[];
}
export interface QuickReplyOption {
  id: string;
  label: string;
  /** When clicked, this string is submitted to Travis as the next
   *  user turn (defaults to label). */
  value?: string;
}

/** Named workflow steps with status. Use for slot-fill progress
 *  ("gathering → drafting → preview → send"). */
export interface StepperPart extends BasePart {
  kind: "stepper";
  title?: string;
  steps: StepperStep[];
}
export interface StepperStep {
  label: string;
  status: "done" | "active" | "pending" | "failed";
  detail?: string;
}

/** Syntax-highlighted code snippet with copy button. */
export interface CodeSnippetPart extends BasePart {
  kind: "code_snippet";
  code: string;
  language?: string;
  filename?: string;
}

/** Person as a first-class card — replaces prose "here's Sarah's info".
 *  Fields mirror the people pack contact schema. */
export interface ContactCardPart extends BasePart {
  kind: "contact_card";
  display_name: string;
  relationship?: string;
  organization?: string;
  email?: string;
  phone?: string;
  birthday?: string;
  notes?: string;
  last_contact_at?: string;
  /** Quick actions the card exposes ("Email", "Log a call"). */
  actions?: RowAction[];
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
