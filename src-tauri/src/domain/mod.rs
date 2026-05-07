pub mod task;

#[derive(Debug, thiserror::Error)]
pub enum DomainError {
    #[error("validation: {0}")]
    Validation(String),
    #[error(transparent)]
    Db(#[from] sqlx::Error),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl DomainError {
    pub fn invalid<S: Into<String>>(s: S) -> Self {
        DomainError::Validation(s.into())
    }
}

// L2E pack re-exports for backwards compat. The L2E typed modules
// (coach, school, coach_hours, signing_sheet, invoice) plus Stats
// and stats() moved to `crate::packs::lead_to_empower::domain` in
// step 8b of the pack refactor (PACKS_AUDIT.md). Existing core call
// sites — chiefly `domain_cmd` and `pdf` — still reference
// `crate::domain::coach` etc.; the re-exports below keep those paths
// resolving until step 8c moves both into the pack and retires this
// module's L2E surface.
#[cfg(feature = "pack-lead-to-empower")]
pub use crate::packs::lead_to_empower::domain::{
    coach, coach_hours, invoice, school, signing_sheet, stats, Stats,
};
