//! Finance pack — bills + subscriptions.
//!
//! Default-enabled for every Travis install. Handles the "what am I
//! paying for?" question and recurring-bill awareness.

pub mod tools;
mod tables;

use crate::packs::{PackHandle, PackMigration, TableDef};

const SLUG: &str = "finance";

pub struct FinancePack;

impl PackHandle for FinancePack {
    fn slug(&self) -> &'static str { SLUG }
    fn name(&self) -> &'static str { "Finance" }
    fn version(&self) -> &'static str { "0.1.0" }
    fn description(&self) -> &'static str {
        "Recurring bills, subscriptions, and their monthly cost. Answers \
         'what am I paying for?' Enabled by default."
    }
    fn default_enabled(&self) -> bool { true }
    fn migrations(&self) -> &'static [PackMigration] { MIGRATIONS }
    fn prompt_fragment(&self) -> Option<&'static str> { Some(PROMPT_FRAGMENT) }
    fn entity_kinds(&self) -> &'static [&'static str] { &[] }
    fn action_kinds(&self) -> &'static [&'static str] { &[] }
    fn tables(&self) -> &'static [TableDef] { tables::TABLES }
    fn register_tools(&self, registry: &mut crate::tools::ToolRegistry) {
        registry.register(Box::new(tools::LogBillTool));
        registry.register(Box::new(tools::ListBillsTool));
        registry.register(Box::new(tools::LogSubscriptionTool));
        registry.register(Box::new(tools::ListSubscriptionsTool));
    }
}

const FINANCE_INIT_SQL: &str = include_str!("migrations/0001_init.sql");

static MIGRATIONS: &[PackMigration] = &[PackMigration {
    name: "0001_init",
    sql: FINANCE_INIT_SQL,
}];

const PROMPT_FRAGMENT: &str = "\
You track recurring bills and subscriptions. Amounts in cents (150.00 = 15000).\n\
\n\
- 'log rent — 2100 a month, due the 1st' -> `log_bill` name='Rent' \
  amountCents=210000 cadence='monthly'.\n\
- 'add Netflix — 15.49/month' -> `log_subscription` amountCents=1549.\n\
- 'what bills are coming up?' -> `list_bills`.\n\
- 'what am I paying for?' / 'am I doubling up on streaming?' -> \
  `list_subscriptions` (returns monthly total).\n\
\n\
When the user mentions a recurring cost, quietly log it. Don't ask for \
confirmation on updates — treat as upsert.\
";
