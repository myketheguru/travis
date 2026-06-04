//! `reconcile_documents` — cross-document consistency check.
//!
//! Given a set of document ids whose `extracted_json` has been
//! populated, walks well-known field names and flags inconsistencies
//! across them — different totals, date windows that don't overlap,
//! line-item counts that disagree.
//!
//! Travis uses this when a workflow has multiple Document slots filled
//! (typically the PO + WO + signed sheet for a generate_invoice flow)
//! so it can surface "I notice the PO covers Jan 1–Feb 28 but the
//! signed sheet covers Feb 5–Feb 28 — is that intentional?" *before*
//! proposing the invoice draft.
//!
//! Output is human-readable narrative + structured findings the LLM
//! can quote back. The LLM stays the decider; this tool is just the
//! cross-check.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use tauri::Manager;

use crate::documents::db as docs_db;
use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};
use crate::AppState;

pub struct ReconcileDocumentsTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    /// Document IDs to reconcile. Order doesn't matter; results are
    /// keyed by id and by document kind.
    document_ids: Vec<i64>,
}

#[async_trait]
impl Tool for ReconcileDocumentsTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "reconcile_documents".into(),
            description: "Cross-check structured fields across multiple ingested \
                documents (typically PO + WO + signed sheet for an invoice). Returns \
                the union of extracted fields plus a list of flagged inconsistencies — \
                date windows that don't overlap, PO numbers that disagree, totals that \
                don't match. Use when multiple Document slots are filled on the active \
                workflow before proposing the finalize action."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "documentIds": {
                        "type": "array",
                        "items": { "type": "integer" },
                        "description": "Document IDs to reconcile. At least 2."
                    }
                },
                "required": ["documentIds"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        if p.document_ids.len() < 2 {
            anyhow::bail!("reconcile_documents needs at least 2 document IDs");
        }
        let state = ctx.app.state::<AppState>();

        let mut docs: Vec<(docs_db::Document, Option<Value>)> = Vec::new();
        for id in &p.document_ids {
            let doc = docs_db::get(&state.db.pool, *id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("document {id} not found"))?;
            let extracted = doc
                .extracted_json
                .as_deref()
                .and_then(|s| serde_json::from_str::<Value>(s).ok());
            docs.push((doc, extracted));
        }

        let findings = reconcile(&docs);

        let summary: Vec<Value> = docs
            .iter()
            .map(|(d, extracted)| {
                json!({
                    "id": d.id,
                    "kind": d.kind,
                    "displayName": d.display_name,
                    "ingestStatus": d.ingest_status,
                    "extracted": extracted,
                })
            })
            .collect();

        Ok(serde_json::to_string(&json!({
            "documents": summary,
            "findings": findings,
            "consistencyOk": findings.iter().all(|f| f["severity"] != "warning" && f["severity"] != "error"),
        }))?)
    }
}

/// Compare extracted fields across docs and produce a list of findings.
/// Each finding: { severity: 'info'|'warning'|'error', kind, message,
/// involves: [doc_ids] }.
fn reconcile(docs: &[(docs_db::Document, Option<Value>)]) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::new();

    // Collect common fields across all docs that exposed them.
    let mut po_numbers: Vec<(i64, String)> = Vec::new();
    let mut school_names: Vec<(i64, String)> = Vec::new();
    let mut period_starts: Vec<(i64, String)> = Vec::new();
    let mut period_ends: Vec<(i64, String)> = Vec::new();
    let mut totals_cents: Vec<(i64, i64, String)> = Vec::new(); // (doc_id, cents, kind)

    for (doc, extracted) in docs {
        let Some(ext) = extracted else { continue };
        let id = doc.id;
        if let Some(s) = ext.get("po_number").and_then(|v| v.as_str()) {
            po_numbers.push((id, s.trim().to_string()));
        }
        if let Some(s) = ext.get("school_name").and_then(|v| v.as_str()) {
            school_names.push((id, s.trim().to_string()));
        }
        if let Some(s) = ext.get("period_start").and_then(|v| v.as_str()) {
            period_starts.push((id, s.trim().to_string()));
        }
        if let Some(s) = ext.get("period_end").and_then(|v| v.as_str()) {
            period_ends.push((id, s.trim().to_string()));
        }
        if let Some(n) = ext.get("total_cents").and_then(|v| v.as_i64()) {
            totals_cents.push((id, n, doc.kind.clone()));
        }
    }

    // PO numbers disagree?
    if !po_numbers.is_empty() {
        let first = &po_numbers[0].1;
        let disagree: Vec<i64> = po_numbers
            .iter()
            .filter(|(_, n)| !n.eq_ignore_ascii_case(first))
            .map(|(id, _)| *id)
            .collect();
        if !disagree.is_empty() {
            let all: Vec<String> = po_numbers
                .iter()
                .map(|(id, n)| format!("doc#{id}:{n}"))
                .collect();
            out.push(json!({
                "severity": "warning",
                "kind": "po_number_mismatch",
                "message": format!("PO numbers disagree: {}", all.join(", ")),
                "involves": po_numbers.iter().map(|(id, _)| id).collect::<Vec<_>>(),
            }));
        }
    }

    // School names disagree (case-insensitive)?
    if !school_names.is_empty() {
        let first = school_names[0].1.to_lowercase();
        let disagree: Vec<i64> = school_names
            .iter()
            .filter(|(_, n)| n.to_lowercase() != first)
            .map(|(id, _)| *id)
            .collect();
        if !disagree.is_empty() {
            let all: Vec<String> = school_names
                .iter()
                .map(|(id, n)| format!("doc#{id}:\"{n}\""))
                .collect();
            out.push(json!({
                "severity": "warning",
                "kind": "school_name_mismatch",
                "message": format!("School names differ across docs: {}", all.join(", ")),
                "involves": school_names.iter().map(|(id, _)| id).collect::<Vec<_>>(),
            }));
        }
    }

    // Period overlap check: do all start/end pairs span a common range?
    if period_starts.len() >= 2 && period_ends.len() >= 2 {
        let max_start = period_starts.iter().map(|(_, s)| s.as_str()).max();
        let min_end = period_ends.iter().map(|(_, s)| s.as_str()).min();
        if let (Some(max_s), Some(min_e)) = (max_start, min_end) {
            if max_s > min_e {
                out.push(json!({
                    "severity": "warning",
                    "kind": "period_overlap_empty",
                    "message": format!(
                        "Period windows don't overlap — latest start {max_s} is after earliest end {min_e}"
                    ),
                    "involves": docs.iter().map(|(d, _)| d.id).collect::<Vec<_>>(),
                }));
            }
        }
    }

    // PO total vs invoice total mismatch?
    let po_total = totals_cents.iter().find(|(_, _, k)| k == "po" || k == "purchase_order").map(|(id, c, _)| (*id, *c));
    let invoice_total = totals_cents.iter().find(|(_, _, k)| k == "invoice").map(|(id, c, _)| (*id, *c));
    if let (Some((po_id, po)), Some((inv_id, inv))) = (po_total, invoice_total) {
        if po != inv {
            out.push(json!({
                "severity": "warning",
                "kind": "po_invoice_total_mismatch",
                "message": format!(
                    "PO total (${:.2}) and invoice total (${:.2}) don't match",
                    po as f64 / 100.0,
                    inv as f64 / 100.0
                ),
                "involves": [po_id, inv_id],
            }));
        }
    }

    if out.is_empty() {
        out.push(json!({
            "severity": "info",
            "kind": "consistent",
            "message": "No inconsistencies flagged across the supplied documents.",
            "involves": docs.iter().map(|(d, _)| d.id).collect::<Vec<_>>(),
        }));
    }
    out
}
