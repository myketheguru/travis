//! Household pack — grocery + chores.
//!
//! Default-enabled for every Travis install. Extends the personal-life
//! surface beyond notes with structured lists.

pub mod tools;
mod tables;

use crate::packs::{PackHandle, PackMigration, TableDef};

const SLUG: &str = "household";

pub struct HouseholdPack;

impl PackHandle for HouseholdPack {
    fn slug(&self) -> &'static str { SLUG }
    fn name(&self) -> &'static str { "Household" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn description(&self) -> &'static str {
        "Grocery lists, chores, and errand runs — the personal-life logistics \
         the everyday pack doesn't structure. Enabled by default."
    }
    fn default_enabled(&self) -> bool { true }
    fn migrations(&self) -> &'static [PackMigration] { MIGRATIONS }
    fn prompt_fragment(&self) -> Option<&'static str> { Some(PROMPT_FRAGMENT) }
    fn entity_kinds(&self) -> &'static [&'static str] { &[] }
    fn action_kinds(&self) -> &'static [&'static str] { &[] }
    fn tables(&self) -> &'static [TableDef] { tables::TABLES }
    fn register_tools(&self, registry: &mut crate::tools::ToolRegistry) {
        registry.register(Box::new(tools::AddToGroceryTool));
        registry.register(Box::new(tools::ListGroceryTool));
        registry.register(Box::new(tools::MarkGroceryPurchasedTool));
        registry.register(Box::new(tools::LogChoreTool));
    }
}

const HOUSEHOLD_INIT_SQL: &str = include_str!("migrations/0001_init.sql");

static MIGRATIONS: &[PackMigration] = &[PackMigration {
    name: "0001_init",
    sql: HOUSEHOLD_INIT_SQL,
}];

const PROMPT_FRAGMENT: &str = "\
You maintain the household grocery list and chore tracker.\n\
\n\
- 'add milk, eggs, cardamom to grocery' -> `add_to_grocery` with all three \
  in one call. Categorize when obvious (dairy, produce, pantry, household).\n\
- 'what's on the grocery list?' -> `list_grocery`.\n\
- 'I bought the milk and eggs' -> `mark_grocery_purchased`.\n\
- 'log that I took out the trash' -> `log_chore` with doneNow=true.\n\
- 'add mopping to the chores' -> `log_chore` (no doneNow).\n\
\n\
Don't confirm every add — just do it and give a short line.\
";
