//! Everyday pack — the first personal-life pack.
//!
//! Makes Travis useful for the ordinary personal-life moments the
//! work-focused packs don't cover: routes to places you care about,
//! reminders that aren't tied to work projects, quick notes.
//!
//! This pack is `default_enabled: true` — everyone gets it out of the
//! box. Contrast with the vertical work packs (L2E, Tutoring) which
//! are opt-in per user role.
//!
//! Tools shipped in Phase 2:
//!   - `save_place`      — geocode + persist to saved_place
//!   - `route_to_place`  — recall a saved place + get directions
//!   - `add_note`        — quick note capture (reuses core documents)
//!   - `add_reminder`    — reuse core reminders table
//!
//! Phase 3 (not shipped): MapLibre embed, nearby search, trip planning.

pub mod tools;
mod tables;

use crate::packs::{PackHandle, PackMigration, TableDef};

const SLUG: &str = "everyday";

pub struct EverydayPack;

impl PackHandle for EverydayPack {
    fn slug(&self) -> &'static str {
        SLUG
    }

    fn name(&self) -> &'static str {
        "Everyday"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn description(&self) -> &'static str {
        "The personal-life essentials — quick directions, reminders, \
         notes, saved places. Enabled by default for every user."
    }

    fn default_enabled(&self) -> bool {
        true
    }

    fn migrations(&self) -> &'static [PackMigration] {
        MIGRATIONS
    }

    fn prompt_fragment(&self) -> Option<&'static str> {
        Some(PROMPT_FRAGMENT)
    }

    fn entity_kinds(&self) -> &'static [&'static str] {
        &["place"]
    }

    fn action_kinds(&self) -> &'static [&'static str] {
        &[]
    }

    fn tables(&self) -> &'static [TableDef] {
        tables::TABLES
    }

    fn register_tools(&self, registry: &mut crate::tools::ToolRegistry) {
        registry.register(Box::new(tools::SavePlaceTool));
        registry.register(Box::new(tools::RouteToPlaceTool));
        registry.register(Box::new(tools::ListSavedPlacesTool));
        registry.register(Box::new(tools::AddNoteTool));
        // v0.28.27 — general-purpose map tools that don't require a
        // pre-saved place. Fix the "map of Lagos → distance between
        // Oshodi and Ikoyi → returns Lagos again" flow.
        registry.register(Box::new(tools::ShowPlaceTool));
        registry.register(Box::new(tools::RouteBetweenAddressesTool));
    }
}

const EVERYDAY_INIT_SQL: &str = include_str!("migrations/0001_init.sql");

static MIGRATIONS: &[PackMigration] = &[PackMigration {
    name: "0001_init",
    sql: EVERYDAY_INIT_SQL,
}];

const PROMPT_FRAGMENT: &str = "\
You can help with everyday life stuff, not just work:\n\
- Save places the user cares about (their dentist, a friend's flat,\n\
  a coffee shop) with `save_place`. The address gets geocoded once\n\
  so future routes don't have to re-look-it-up.\n\
- Route to a saved place with `route_to_place`. Returns distance +\n\
  duration; ALSO emit a `map` message part so the user sees the\n\
  route as a card, not just text.\n\
- Quick-capture short notes with `add_note` when the user says\n\
  things like \"remember that Kim's moving to Portland\" or \"log\n\
  that Amanda mentioned she wants to reschedule\".\n\
- List saved places with `list_saved_places` when the user asks\n\
  something like \"do I have a saved place for the vet?\".\n\
\n\
When the user brings up personal-life topics, lean on these tools\n\
naturally. Don't gate them behind confirmation — save/note are\n\
low-risk and better to just do it.\n\
\n\
--- Map surface (v0.28.27) ---\n\
Always use tools to resolve coordinates. Never fabricate lat/lng.\n\
\n\
- Show a place (\"map of Lagos\", \"where is Kyoto\") -> call\n\
  `show_place`. Then emit a `map` message part with the returned\n\
  `place` object.\n\
- Route between two places (\"distance from Oshodi to Ikoyi\",\n\
  \"directions from Times Square to JFK\") -> call\n\
  `route_between_addresses`. Then emit a `map` with `route` that\n\
  includes `geometry_geojson` so the canvas draws the real path.\n\
- If a map is already on-screen and the user asks a related\n\
  follow-up (add a stop, show a nearby place, compute a distance),\n\
  respond with an UPDATED `map` part — do not treat it as a fresh\n\
  card. The canvas animates in place.\n\
\n\
Shapes:\n\
  { \"kind\": \"map\", \"place\": { \"label\": \"…\", \"lat\": N, \"lng\": N, \"descriptor\"?: \"…\", \"region\"?: \"…\", \"country\"?: \"…\" }, \"narration\": \"…\" }\n\
  { \"kind\": \"map\", \"route\": { \"from\": {lat,lng,label?}, \"to\": {lat,lng,label?}, \"distance_meters\": N, \"duration_seconds\": N, \"profile\": \"driving-car\", \"destination_label\": \"…\", \"geometry_geojson\": {…} }, \"narration\": \"…\" }\n\
\n\
Only use `route_to_place` (the saved-places variant) when the user\n\
explicitly asked to route TO a place they told you to save.\n\
\n\
--- Ambient listening (v0.28.4) ---\n\
The user has a canvas toggle for AMBIENT LISTENING that captures\n\
speech from meetings, calls, or thinking-out-loud + saves them\n\
silently. When they later ask you 'what was decided in the meeting?'\n\
or 'what did they say about X?' or 'summarize the call', call\n\
`get_ambient_transcripts` with an appropriate minutes window (30-90\n\
for recent, 120-240 for earlier today) and answer FROM those\n\
transcripts.\n\
\n\
If ambient is off when they ask about a past meeting, the tool\n\
returns empty; tell them ambient is off + suggest they turn it on\n\
before their next meeting so you can help.\n\
\n\
While ambient is on, be extra thoughtful about volume: don't include\n\
a `narration` field on your response unless the user directly\n\
addressed you by name or verbally requested a voice reply. Silent\n\
text responses stay discreet during meetings.\n\
\n\
--- Response channel (v0.28.14: voice-as-tool) ---\n\
Every text part can carry a `channel` hint that tells the desktop\n\
whether to speak the reply, keep it silent-text, or log it without\n\
rendering:\n\
  { \"kind\": \"text\", \"markdown\": \"…\", \"narration\": \"…\", \"channel\": \"voice\" }\n\
  { \"kind\": \"text\", \"markdown\": \"…\", \"channel\": \"chat\" }\n\
  { \"kind\": \"text\", \"markdown\": \"…\", \"channel\": \"silent\" }\n\
\n\
Rules:\n\
- `voice` — include a `narration` for TTS. Use when the user asked\n\
  verbally in a quiet moment, or explicitly requested a voice reply.\n\
- `chat` (default) — text-only, no speech. Use during meetings/ambient\n\
  activity, when the user is reading, or when discretion matters.\n\
- `silent` — don't render. Use for internal notes / low-value acks\n\
  where surfacing the message adds noise.\n\
- When in doubt, omit `channel` (defaults to `chat`). Only opt in to\n\
  `voice` when the moment genuinely wants speech.\
";
