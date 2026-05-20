//! Read-only LLM tools contributed by the Lead to Empower pack.
//!
//! Registered via `LeadToEmpowerPack::register_tools` into the core
//! read-only registry, so the LLM can call them autonomously with no
//! confirmation gate (they never write).

pub mod find_contract;
pub mod find_engagement;
pub mod find_school;
pub mod quote_margin;
pub mod summarize_context;
pub mod validate_invoice;
