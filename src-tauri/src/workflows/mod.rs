//! Workflow recipes + dialogue manager.
//!
//! Taylor's mental model is "I tell Travis what I want, and Travis
//! drives the workflow — asking me for inputs it needs, finding what
//! it already has, and delivering the output." That's a slot-filling
//! dialogue manager layered on top of the existing action handlers.
//!
//! This module is the framework: declarative `WorkflowDef`s with typed
//! `Slot`s, a registry of built-in recipes, per-conversation persisted
//! state, and a prompt block that surfaces "what workflow is active,
//! what's filled, what's missing" to the LLM each turn.
//!
//! The LLM itself drives transitions via the `workflowOps` field on
//! the journal extraction schema; this module persists what it emits
//! and renders the next state back into the next prompt.
//!
//! See [[feedback-workflow-led]] and [[feedback-docs-first]] for the
//! design context.

pub mod cmd;
pub mod dialogue;
pub mod recipe;
pub mod registry;
pub mod state;

pub use recipe::{Slot, SlotKind, WorkflowDef};
pub use registry::find_recipe;
pub use state::{WorkflowSlotValue, WorkflowState};
