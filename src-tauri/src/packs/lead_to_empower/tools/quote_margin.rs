//! `lte_quote_margin` — compute the Appendix G margin for a catalog
//! module with optional staffing/price overrides. Read-only: reads
//! `catalog_module`, writes nothing. The conversational surface of
//! LTE_QUOTE_SPEC.md (the default; the `quote` table is for persisting
//! scenarios). All math goes through `super::super::pricing` so the
//! tool and any future persisted-quote path can't drift.

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::llm::ToolDef;
use crate::tools::{Tool, ToolContext};

use crate::packs::lead_to_empower::pricing;

pub struct QuoteMarginTool;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    /// Catalog line number ("16") or a name substring ("developing data").
    module: String,
    #[serde(default)]
    participants: Option<i64>,
    #[serde(default)]
    instructors: Option<i64>,
    #[serde(default)]
    sessions: Option<i64>,
    #[serde(default)]
    hours_per_session: Option<f64>,
    #[serde(default)]
    facilitator_rate_cents: Option<i64>,
    #[serde(default)]
    ga_cents: Option<i64>,
    #[serde(default)]
    material_cents: Option<i64>,
    #[serde(default)]
    rental_cents: Option<i64>,
    /// Override the list/bid price (default = catalog list price).
    #[serde(default)]
    list_price_cents: Option<i64>,
    #[serde(default)]
    in_kind_cents: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct ModuleRow {
    line_no: i64,
    name: String,
    list_price_cents: i64,
    sessions: i64,
    hours_per_session: f64,
    instructors_per_session: i64,
}

#[async_trait]
impl Tool for QuoteMarginTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "lte_quote_margin".into(),
            description: "Compute the Lead to Empower cost/margin for one \
                catalog module delivery using the Appendix G model \
                (labor = sessions × hours × instructors × facilitator \
                rate, + G&A + materials + rental; margin = list − cost). \
                Use when the user asks what a module would cost, what \
                margin a bid leaves, or to compare staffing options \
                (e.g. one vs two facilitators). Identify the module by \
                Appendix F line number or a name substring."
                .into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "module": { "type": "string", "description": "Catalog line number (e.g. \"16\") or a name substring (e.g. \"developing data\")." },
                    "participants": { "type": "integer", "description": "Headcount — informational + per-participant figures; not a cost driver in the model." },
                    "instructors": { "type": "integer", "description": "Override instructors per session (catalog default otherwise; coaching = 1, workshops = 2)." },
                    "sessions": { "type": "integer", "description": "Override number of sessions." },
                    "hoursPerSession": { "type": "number", "description": "Override hours per session." },
                    "facilitatorRateCents": { "type": "integer", "description": "Per-instructor $/hr in cents. Default 10000 ($100)." },
                    "gaCents": { "type": "integer", "description": "Flat per-delivery G&A in cents. Default 72500 ($725, an estimate)." },
                    "materialCents": { "type": "integer", "description": "Materials cost in cents. Default 0." },
                    "rentalCents": { "type": "integer", "description": "Rental/equipment cost in cents. Default 0." },
                    "listPriceCents": { "type": "integer", "description": "Override the list/bid price in cents. Default = catalog list price." },
                    "inKindCents": { "type": "integer", "description": "Contributed in-kind value in cents — reported, not subtracted from cost." }
                },
                "required": ["module"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        let key = p.module.trim();
        if key.is_empty() {
            anyhow::bail!("module is required (line number or name substring)");
        }

        // Resolve the module: numeric → line_no, else name LIKE. Catalog
        // is global reference data (seeded in the Personal workspace);
        // not workspace-filtered here by design.
        let row: Option<ModuleRow> = if let Ok(line) = key.parse::<i64>() {
            sqlx::query_as::<_, ModuleRow>(
                "SELECT line_no, name, list_price_cents, sessions, \
                        hours_per_session, instructors_per_session \
                 FROM catalog_module WHERE line_no = ?1 LIMIT 1",
            )
            .bind(line)
            .fetch_optional(&ctx.db.pool)
            .await?
        } else {
            let like = format!("%{}%", key.to_lowercase());
            sqlx::query_as::<_, ModuleRow>(
                "SELECT line_no, name, list_price_cents, sessions, \
                        hours_per_session, instructors_per_session \
                 FROM catalog_module \
                 WHERE LOWER(name) LIKE ?1 ORDER BY line_no LIMIT 1",
            )
            .bind(like)
            .fetch_optional(&ctx.db.pool)
            .await?
        };

        let Some(m) = row else {
            // Help the LLM recover with the available names.
            let names: Vec<(i64, String)> = sqlx::query_as(
                "SELECT line_no, name FROM catalog_module ORDER BY line_no",
            )
            .fetch_all(&ctx.db.pool)
            .await
            .unwrap_or_default();
            let list = names
                .iter()
                .map(|(n, nm)| format!("{n}. {nm}"))
                .collect::<Vec<_>>()
                .join("\n");
            anyhow::bail!(
                "No catalog module matched \"{key}\". Available modules:\n{list}"
            );
        };

        let sessions = p.sessions.unwrap_or(m.sessions);
        let hours = p.hours_per_session.unwrap_or(m.hours_per_session);
        let instructors = p.instructors.unwrap_or(m.instructors_per_session);
        let list_price = p.list_price_cents.unwrap_or(m.list_price_cents);

        let b = pricing::compute(pricing::Inputs {
            sessions,
            hours_per_session: hours,
            instructors_per_session: instructors,
            facilitator_rate_cents: p
                .facilitator_rate_cents
                .unwrap_or(pricing::DEFAULT_FACILITATOR_RATE_CENTS),
            ga_cents: p.ga_cents.unwrap_or(pricing::DEFAULT_GA_CENTS),
            material_cents: p.material_cents.unwrap_or(0),
            rental_cents: p.rental_cents.unwrap_or(0),
            list_price_cents: list_price,
            in_kind_cents: p.in_kind_cents.unwrap_or(0),
            participants: p.participants.unwrap_or(0),
        });

        let shape = format!(
            "{sessions} session{} × {}h × {instructors} instr",
            if sessions == 1 { "" } else { "s" },
            if hours.fract() == 0.0 {
                format!("{}", hours as i64)
            } else {
                format!("{hours}")
            },
        );
        let title = format!("{} (line {})", m.name, m.line_no);
        let prose = pricing::render(&title, &shape, &b);

        let structured = json!({
            "module": { "lineNo": m.line_no, "name": m.name },
            "laborCents": b.labor_cents,
            "gaCents": b.ga_cents,
            "materialCents": b.material_cents,
            "rentalCents": b.rental_cents,
            "costCents": b.cost_cents,
            "listPriceCents": b.list_price_cents,
            "marginCents": b.margin_cents,
            "marginPct": (b.margin_pct * 10.0).round() / 10.0,
            "inKindCents": b.in_kind_cents,
            "thinMargin": b.thin_margin,
            "listPerParticipantCents": b.list_per_participant_cents,
            "marginPerParticipantCents": b.margin_per_participant_cents,
        });

        Ok(format!("{prose}\n{}", structured))
    }
}
