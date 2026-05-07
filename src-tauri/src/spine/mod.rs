//! Pack spine — generic primitives every vertical has in common, used as a
//! rendezvous point between core and pack-typed tables.
//!
//! See PACKS.md and PACKS_AUDIT.md for the architectural rationale.
//!
//! Three concepts:
//! - [`entity`]   — generic record (coach, client, case, job, invoice, …)
//! - [`relation`] — typed edge between two entities
//! - [`event`]    — anything that happened, optionally tied to an entity
//!
//! Pack code calls these helpers from its own CRUD paths to keep the spine
//! in sync with its typed tables. Cross-pack retrieval reads the spine;
//! pack-internal queries hit the typed tables directly.

pub mod entity;
pub mod event;
pub mod relation;
