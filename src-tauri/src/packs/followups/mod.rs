//! Follow-ups pack — commitment tracker.
//!
//! Auto-captures promises the user makes ('I'll send you X', 'let me
//! get back to you'). Default-enabled for every Travis install.

pub mod tools;
mod tables;

use crate::packs::{PackHandle, PackMigration, TableDef};

const SLUG: &str = "followups";

pub struct FollowupsPack;

impl PackHandle for FollowupsPack {
    fn slug(&self) -> &'static str { SLUG }
    fn name(&self) -> &'static str { "Follow-ups" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn description(&self) -> &'static str {
        "Every commitment the user makes captured as a first-class row. \
         Powers 'who did I promise to email?' Enabled by default."
    }
    fn default_enabled(&self) -> bool { true }
    fn migrations(&self) -> &'static [PackMigration] { MIGRATIONS }
    fn prompt_fragment(&self) -> Option<&'static str> { Some(PROMPT_FRAGMENT) }
    fn entity_kinds(&self) -> &'static [&'static str] { &[] }
    fn action_kinds(&self) -> &'static [&'static str] { &[] }
    fn tables(&self) -> &'static [TableDef] { tables::TABLES }
    fn register_tools(&self, registry: &mut crate::tools::ToolRegistry) {
        registry.register(Box::new(tools::LogFollowupTool));
        registry.register(Box::new(tools::ListFollowupsTool));
        registry.register(Box::new(tools::CompleteFollowupTool));
    }
}

const FOLLOWUPS_INIT_SQL: &str = include_str!("migrations/0001_init.sql");

static MIGRATIONS: &[PackMigration] = &[PackMigration {
    name: "0001_init",
    sql: FOLLOWUPS_INIT_SQL,
}];

const PROMPT_FRAGMENT: &str = "\
You track follow-ups — commitments the user makes to send / do / check on \
something for someone. Whenever the user says 'I'll send you', 'let me get \
back to you', 'I owe them', 'I'll follow up', 'I need to circle back', \
'I promised', 'gotta email them' — silently call `log_followup` with the \
commitment as `title` and the person if mentioned.\n\
\n\
When the user asks 'who did I promise to email?', 'what's open with Sarah?', \
'what am I forgetting?' — call `list_followups`. When they say they finished / \
sent / did it, call `complete_followup`.\
";
