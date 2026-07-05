//! Rich response contract — the typed shape the LLM emits.
//!
//! Everything Travis says back to the user is an array of `MessagePart`s.
//! Each part has a `kind` that the renderer switches on to draw the
//! right visual card. Text is a fallback part type; the LLM is prompted
//! to prefer richer types when the answer maps to one.
//!
//! This module owns the ENVELOPE. The individual card payloads
//! (MapCard, DocRefCard, etc.) live near the code that produces them —
//! e.g., MapCard payload lives near the maps tool.
//!
//! See INTERFACE.md in the cloud repo for the design principles.

use serde::{Deserialize, Serialize};

/// The typed reply Travis emits. Always an array of parts, even if
/// there's only one.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RichResponse {
    pub parts: Vec<MessagePart>,
}

/// A single visual/interactive piece the renderer draws. Kind
/// determines which component renders it; payload carries the
/// component-specific data.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MessagePart {
    /// Prose. Kept for genuinely conversational responses; not the
    /// default. Should carry a short `narration` when speech output
    /// is on.
    Text {
        markdown: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        narration: Option<String>,
    },

    /// Inline map card — route, POIs, ETA.
    Map {
        route: MapRoute,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        narration: Option<String>,
    },

    /// Reference to a document Travis has. Renders as a doc preview
    /// with an Open button that opens the full doc viewer.
    DocRef {
        document_id: i64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        snippet: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        narration: Option<String>,
    },

    /// An entity from Travis's world model — a person, place, thing.
    Entity {
        entity_id: i64,
        display_name: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        facts: Option<serde_json::Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        narration: Option<String>,
    },

    /// A time-window of calendar events. Renders as a timeline strip.
    Calendar {
        window_start: String, // ISO
        window_end: String,   // ISO
        events: Vec<CalendarEvent>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        narration: Option<String>,
    },

    /// A conversation between two Travises (task 311 / T2T rich card).
    T2tConvo {
        query_id: String,
        from_display: String,
        to_display: String,
        question: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        drafted_response: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        final_response: Option<String>,
        state: T2tConvoState,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        narration: Option<String>,
    },

    /// A side effect Travis is proposing (send email, book meeting,
    /// invoice generation). Rendered as a card with big Approve /
    /// Edit / Decline controls.
    ActionProposal {
        action_kind: String,
        preview_title: String,
        preview_body: String,
        input: serde_json::Value,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        narration: Option<String>,
    },

    /// Structured list of items. Renders as rows with per-row actions.
    List {
        title: String,
        rows: Vec<ListRow>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        narration: Option<String>,
    },

    /// A numeric answer with a small chart.
    Chart {
        chart_kind: ChartKind,
        series: Vec<ChartSeries>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        narration: Option<String>,
    },

    /// Image/video preview.
    Media {
        url: String,
        media_kind: MediaKind,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        caption: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        narration: Option<String>,
    },
}

// ─── Sub-payload types ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapRoute {
    pub from: LatLng,
    pub to: LatLng,
    pub distance_meters: f64,
    pub duration_seconds: f64,
    #[serde(default)]
    pub profile: Option<String>, // driving-car | cycling-regular | foot-walking
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub destination_label: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub geometry_geojson: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatLng {
    pub lat: f64,
    pub lng: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    pub start: String,
    pub end: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attendees: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum T2tConvoState {
    Sending,
    Delivered,
    Considering,
    Drafted,
    Answered,
    Declined,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListRow {
    pub id: String,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<RowAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RowAction {
    pub kind: String, // 'primary' | 'secondary'
    pub label: String,
    pub verb: String, // machine name the LLM can call back
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChartKind {
    Sparkline,
    Bar,
    Pie,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChartSeries {
    pub label: String,
    pub points: Vec<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Image,
    Video,
    Audio,
}

// ─── System prompt fragment (injected into every LLM call) ────────

pub const SYSTEM_PROMPT_FRAGMENT: &str = r#"
## Rich response contract

Your responses to the user are NOT free prose. Emit a JSON object
matching this shape, where `parts` is an array of typed message parts:

```
{ "parts": [ { "kind": "...", ... }, ... ] }
```

Available part kinds and when to use each:

- **map** — the user is asking for directions, wants to see a place, or
  a location matters to the answer. Include distance, duration, route
  geometry when you have it.
- **doc_ref** — the user is asking to see a document Travis has. Emit a
  reference; the client renders the doc viewer.
- **entity** — the answer is about a person, place, or thing the user
  knows. Includes name + structured facts.
- **calendar** — the user is asking about upcoming events or a time
  window.
- **t2t_convo** — a Travis-to-Travis interaction is in progress.
- **action_proposal** — you want to take a side effect (send email,
  invoice generation, book meeting) — emit this and let the user
  approve. Never take the action yourself.
- **list** — the answer is multiple items with common shape.
- **chart** — numeric answer that would read better as a sparkline
  or bar.
- **media** — the answer includes an image, video, or audio clip.
- **text** — a genuinely conversational reply (a joke, small talk,
  a follow-up question, an apology). NOT the default.

Prefer the richest kind that fits. A route request → map, not text.
A doc request → doc_ref, not a text summary. A follow-up "yeah go
ahead" → text, no card needed. Every non-text part MUST carry a
short `narration` string that the voice pipeline can speak — this
is the phone-in-pocket fallback.

Combine parts freely: a route can be a map part + an action_proposal
to add a reminder to leave on time. Keep the array small — 1-3 parts
per reply is typical, 5 is the ceiling.
"#;
