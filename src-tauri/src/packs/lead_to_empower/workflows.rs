//! LTE-pack workflow recipes.
//!
//! Each recipe declares the slots Travis needs to gather before it can
//! finalise the corresponding action. Slot ordering matters — the
//! dialogue manager asks for required-missing slots in declared order.
//!
//! New recipes append to `WORKFLOWS` at the bottom.

use crate::workflows::recipe::{Slot, SlotKind, WorkflowDef};

/// Generate an invoice for a coaching engagement.
///
/// Trigger phrasing Taylor uses: "invoice PS498", "I need to bill for
/// the math engagement Jan-Feb", "draft the next invoice for Karen
/// Henderson". The LLM detects intent; this struct declares what
/// Travis needs.
///
/// Reconciles signed sign-in sheet hours against the governing PO and
/// catalog list prices, then proposes a draft through the existing
/// `propose_invoice_draft` action handler.
pub const GENERATE_INVOICE: WorkflowDef = WorkflowDef {
    name: "lte_generate_invoice",
    display_name: "Generate invoice",
    description:
        "Draft an LTE invoice from a coaching engagement's signed sign-in sheet, \
         PO, and (optional) work order. Reconciles hours, dates, and line prices \
         against the catalog before proposing the draft.",
    slots: &[
        Slot {
            name: "school",
            label: "School",
            required: true,
            kind: SlotKind::Entity { kind: "school" },
            ask_hint:
                "If the user mentioned a school by abbreviation or partial match, \
                 resolve it against the graph; only ask explicitly if there's \
                 genuine ambiguity (two PS 95s, etc.).",
        },
        Slot {
            name: "engagement",
            label: "Engagement",
            required: true,
            kind: SlotKind::Entity { kind: "engagement" },
            ask_hint:
                "Most schools have multiple active engagements (math, science, ELA, \
                 staff coaching). Surface them as selection chips: \"⊙ Math team \
                 coaching ⊙ Science team coaching\".",
        },
        Slot {
            name: "period",
            label: "Period",
            required: true,
            kind: SlotKind::DateRange,
            ask_hint:
                "The month or date range the invoice covers. Examples: \"Jan-Feb\", \
                 \"January 2026\", \"Jan 29 to Feb 24\". Normalise to two ISO dates.",
        },
        Slot {
            name: "purchase_order",
            label: "Purchase order",
            required: true,
            kind: SlotKind::Document { kind: "po" },
            ask_hint:
                "The PO this engagement runs under. If the engagement already has a \
                 linked PO from a prior month, surface it as the confident default. \
                 Otherwise ask Taylor to drop the PDF or type the PO number.",
        },
        Slot {
            name: "signed_sheet",
            label: "Signed sign-in sheet",
            required: true,
            kind: SlotKind::Document { kind: "signed_sheet" },
            ask_hint:
                "The signed sign-in sheet for the period. Taylor can drop the PDF; \
                 Travis will extract dates, hours, signer (principal). If the \
                 engagement has a sheet already linked for this period, surface it.",
        },
        Slot {
            name: "work_order",
            label: "Work order",
            required: false,
            kind: SlotKind::Document { kind: "wo" },
            ask_hint:
                "Optional. Some POs ship with a separate WO. If one's linked, \
                 surface it; never block finalisation on this.",
        },
    ],
    finalize_action: "propose_invoice_draft",
};

/// Every workflow this pack contributes. Wired in by the
/// [`crate::packs::lead_to_empower::LeadToEmpowerPack::workflows`]
/// implementation.
pub const WORKFLOWS: &[WorkflowDef] = &[GENERATE_INVOICE];
