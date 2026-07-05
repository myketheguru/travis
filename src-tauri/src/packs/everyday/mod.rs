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
low-risk and better to just do it.\
";
