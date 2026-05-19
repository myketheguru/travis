//! Read-only LLM tools contributed by the Lead to Empower pack.
//!
//! Registered via `LeadToEmpowerPack::register_tools` into the core
//! read-only registry, so the LLM can call them autonomously with no
//! confirmation gate (they never write).

pub mod quote_margin;
