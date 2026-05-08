//! Tutoring pack — 1:1 tutoring agency operations.
//!
//! Vertical: tutors paired with students (in-person or remote), session
//! logs with what was covered + homework, periodic progress reports
//! emailed to parents, billing the parent for hours. Tier A vertical #3
//! in MARKET.md. WTP $200–500/mo per agency.
//!
//! This is the second pack — its purpose is to validate that the pack
//! abstraction landed in v0.2.0 isn't accidentally L2E-shaped. If
//! writing this pack feels mechanical (PackHandle impl + typed tables
//! + spine sync), the abstraction is right.
//!
//! Default-OFF Cargo feature `pack-tutoring`. To build a Travis with
//! this pack, run `cargo build --features pack-tutoring`. Once the
//! runtime pack-selection layer lands (proposed Phase 1.5), packs will
//! be bundled in and toggled at runtime by the user — at which point
//! `default = ["pack-lead-to-empower", "pack-tutoring"]` and the
//! Cargo flag becomes a build-trim option only.

pub mod domain;

use crate::packs::{PackHandle, PackMigration};

const SLUG: &str = "tutoring";

pub struct TutoringPack;

impl PackHandle for TutoringPack {
    fn slug(&self) -> &'static str {
        SLUG
    }

    fn name(&self) -> &'static str {
        "Tutoring"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn description(&self) -> &'static str {
        "1:1 tutoring agency operations — tutors paired with students, \
         session logs, periodic progress reports to parents, billing \
         the parent for hours."
    }

    // default_enabled defaults to false — new packs require the user to
    // opt in explicitly via onboarding or Settings → Packs.

    fn migrations(&self) -> &'static [PackMigration] {
        MIGRATIONS
    }

    fn prompt_fragment(&self) -> Option<&'static str> {
        Some(PROMPT_FRAGMENT)
    }

    fn entity_kinds(&self) -> &'static [&'static str] {
        &["tutor", "student"]
    }

    fn action_kinds(&self) -> &'static [&'static str] {
        &[]
    }
}

const TUTORING_INIT_SQL: &str = include_str!("migrations/0001_init.sql");

static MIGRATIONS: &[PackMigration] = &[PackMigration {
    name: "0001_init",
    sql: TUTORING_INIT_SQL,
}];

const PROMPT_FRAGMENT: &str = "\
You also help with 1:1 tutoring agency ops:\n\
- Track tutors and the students they're paired with, plus parent\n\
  contacts (parents are the payer).\n\
- Log sessions with subject, duration, and what was covered. Sessions\n\
  have status: scheduled / completed / cancelled / no_show — only\n\
  `completed` sessions bill.\n\
- Draft periodic progress reports per student that the tutor sends\n\
  to the parent.\n\
\n\
When the user mentions a student or tutor by name, prefer recording\n\
the mention even if no specific action is requested. The user might\n\
say things like \"finished a session with Maria today\" or \"send\n\
Carlos's parents the March report.\"\
";
