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
        "Draft an LTE invoice from a contract's signed sign-in sheet, PO, and \
         (optional) work order. Reconciles hours, dates, and line prices \
         against the catalog. Invoices draw down against the contract ceiling.",
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
            // Internal slot name stays "engagement" because the SQL
            // table is still `engagement` — but the UI label says
            // "Contract" per Taylor's vocabulary (pack v0.7.0).
            name: "engagement",
            label: "Contract",
            required: true,
            kind: SlotKind::Entity { kind: "engagement" },
            ask_hint:
                "Most schools have multiple active contracts (math, science, ELA, \
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
    allow_code_escape: true,
    code_escape_hint: Some(
        "Use run_python (after analyze_document_styling on the sample if one is \
         provided) when:\n\
         - Taylor has dropped a SAMPLE invoice and asked to \"match it\" or \"like \
           this one\". The hardcoded propose_invoice_draft produces the canonical \
           LTE letterhead — for any other layout, you must write Python.\n\
         - The invoice needs to list MORE service dates than billable quantity (e.g. \
           \"11 dates shown, 10 billed at $1,500 each = $15,000\") — the hardcoded \
           path doesn't support that split.\n\
         - You need to solve a constraint (find quantities that close at exactly $X) \
           — write Python that searches catalog rate combinations.\n\
         - Cross-document reconciliation against a pricing sheet uncovered a \
           mislabel/error that affects the invoice — surface and resolve in code.\n\
         Stick with propose_invoice_draft (fast path) for the standard LTE letterhead \
         layout when no sample is provided.",
    ),
};

/// Derive a sign-in sheet for a single school+coach engagement from
/// Taylor's master coach-hours spreadsheet (Google Sheet exported as
/// CSV or XLSX). The master sheet pools entries across every school
/// LTE is contracted with; Travis filters it down to just the rows
/// for one engagement during a specific period, upserts those rows
/// into `coach_hours`, and renders the printable sheet for principal
/// signature.
///
/// Trigger phrasing: "make me a sign-in sheet for math at PS498 for
/// February", "derive a sign-in sheet from the hours sheet", "print
/// the Jan sign-in sheet for Karen's math engagement".
pub const DERIVE_SIGN_IN_SHEET: WorkflowDef = WorkflowDef {
    name: "lte_derive_sign_in_sheet",
    display_name: "Derive sign-in sheet",
    description:
        "Filter a master coach-hours spreadsheet (Google Sheet export, CSV or XLSX) \
         down to one contract over a period, upsert the rows into coach_hours, \
         and render the printable sign-in sheet PDF for principal signature.",
    slots: &[
        Slot {
            name: "source_spreadsheet",
            label: "Master hours spreadsheet",
            required: true,
            kind: SlotKind::Document { kind: "coach_hours_master" },
            ask_hint:
                "Drop the Google Sheet export (CSV or XLSX). If Taylor already \
                 dropped it earlier in the conversation, surface that document \
                 instead of asking again. After ingest, call set_document_kind \
                 with kind:'coach_hours_master' if it isn't already labelled.",
        },
        Slot {
            name: "engagement",
            label: "Contract",
            required: true,
            kind: SlotKind::Entity { kind: "engagement" },
            ask_hint:
                "Which contract should the sheet cover? Surface active \
                 contracts at the named school as selection chips.",
        },
        Slot {
            name: "period",
            label: "Period",
            required: true,
            kind: SlotKind::DateRange,
            ask_hint:
                "Which date range should the sheet cover? Usually a month \
                 (\"January\", \"Jan-Feb\"). Normalise to two ISO dates.",
        },
    ],
    finalize_action: "lte_derive_sign_in_sheet",
    allow_code_escape: true,
    code_escape_hint: Some(
        "Use run_python when Taylor has supplied a sign-in sheet TEMPLATE (e.g. the \
         PS 19 sample format) and wants the output to match it precisely. Call \
         analyze_document_styling on the template first to extract its purple header \
         colour, zebra striping, signature stroke, and column widths, then write \
         reportlab code that mirrors those features. The hardcoded \
         lte_derive_sign_in_sheet action produces a fixed format and won't match a \
         specific sample.",
    ),
};

/// Create a contract by extracting fields from an uploaded PO or WO
/// (Taylor's request 2026-06-04: "Upload the purchase order and Travis
/// can create a contract from it. Also Work Order. Both represent a
/// contract.").
///
/// Trigger phrasing: "create a contract from this PO", "make a contract
/// from this work order", "set up the contract for PS498 based on this
/// PDF" — when the user has dropped a document and is asking to turn
/// it into a tracked contract.
pub const CREATE_CONTRACT_FROM_DOC: WorkflowDef = WorkflowDef {
    name: "lte_create_contract_from_doc",
    display_name: "Create contract from PO/WO",
    description:
        "Extract a contract record from an uploaded Purchase Order or Work Order \
         PDF. Travis reads the document, proposes a contract draft (ref, school, \
         total amount, period), and on confirmation creates the contract + links \
         the source PO/WO to it.",
    slots: &[
        Slot {
            name: "source_document",
            label: "Source PO or WO",
            required: true,
            kind: SlotKind::Document { kind: "po" },
            ask_hint:
                "The PO or WO PDF Taylor wants to derive the contract from. \
                 Document kind can be 'po', 'purchase_order', 'wo', or \
                 'work_order' — Travis treats either as a valid contract source. \
                 If Taylor's already dropped one in this conversation, surface \
                 that document instead of asking again.",
        },
    ],
    finalize_action: "lte_create_contract_from_doc",
    allow_code_escape: false,
    code_escape_hint: None,
};

/// Every workflow this pack contributes. Wired in by the
/// [`crate::packs::lead_to_empower::LeadToEmpowerPack::workflows`]
/// implementation.
pub const WORKFLOWS: &[WorkflowDef] = &[
    GENERATE_INVOICE,
    DERIVE_SIGN_IN_SHEET,
    CREATE_CONTRACT_FROM_DOC,
];
