//! People pack — contacts as first-class entities.
//!
//! Default-enabled for every Travis install. Powers "who did I promise
//! to email?", "who is Sarah again?", "when's mom's birthday?".

pub mod tools;
mod tables;

use crate::packs::{PackHandle, PackMigration, TableDef};

const SLUG: &str = "people";

pub struct PeoplePack;

impl PackHandle for PeoplePack {
    fn slug(&self) -> &'static str { SLUG }
    fn name(&self) -> &'static str { "People" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn description(&self) -> &'static str {
        "Contacts as first-class records — relationship, org, birthday, \
         last-contact-at. Enabled by default for every user."
    }
    fn default_enabled(&self) -> bool { true }
    fn migrations(&self) -> &'static [PackMigration] { MIGRATIONS }
    fn prompt_fragment(&self) -> Option<&'static str> { Some(PROMPT_FRAGMENT) }
    fn entity_kinds(&self) -> &'static [&'static str] { &["person"] }
    fn action_kinds(&self) -> &'static [&'static str] { &[] }
    fn tables(&self) -> &'static [TableDef] { tables::TABLES }
    fn register_tools(&self, registry: &mut crate::tools::ToolRegistry) {
        registry.register(Box::new(tools::AddContactTool));
        registry.register(Box::new(tools::FindContactTool));
        registry.register(Box::new(tools::LogContactTool));
    }
}

const PEOPLE_INIT_SQL: &str = include_str!("migrations/0001_init.sql");

static MIGRATIONS: &[PackMigration] = &[PackMigration {
    name: "0001_init",
    sql: PEOPLE_INIT_SQL,
}];

const PROMPT_FRAGMENT: &str = "\
You have a contacts book. When the user mentions someone new by name with any \
identifying detail (role, employer, relationship), quietly call `add_contact` \
to persist. When they ask about a person, call `find_contact`. When they \
mention having just talked to / met / emailed someone, call `log_contact_touch` \
to bump their last-contact timestamp.\n\
\n\
Don't nag confirmations for add/log — just do it. Birthdays should be recorded \
even without a year (e.g. '--03-14'). Relationship is one of: friend, family, \
coworker, client, partner, other.\
";
