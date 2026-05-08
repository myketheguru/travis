//! Domain types and CRUD for the Tutoring pack.
//!
//! Four typed tables: tutor, student, session, progress_report. The
//! corresponding SQL tables are created by this pack's `0001_init.sql`
//! migration (NOT in core — this pack arrived after the core schema was
//! already split into the spine + L2E-legacy shape).
//!
//! Backwards-compat note: there's no `crate::domain::*` re-export for
//! these — that one was a transition aid for the L2E lift. New packs
//! reference their own modules at their canonical pack-internal path.

pub mod tutor;
pub mod student;
pub mod session;
pub mod progress_report;

// Reuse core's DomainError so the validation/db error surface is
// consistent across packs.
pub use crate::domain::DomainError;
