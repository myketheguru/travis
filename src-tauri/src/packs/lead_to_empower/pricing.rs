//! Appendix G cost model — the single source of truth for LTE margin
//! math. Both the read-only `lte_quote_margin` tool and (when a custom
//! quote UI lands) the persisted-quote write path call [`compute`] so
//! the two can't drift. See LTE_QUOTE_SPEC.md §2.
//!
//! All money is integer cents. `margin_pct` is the only float, rounded
//! to one decimal at the display edge, not here.

/// Catalog defaults + per-scenario overrides, already resolved (the
/// caller has applied overrides over the `catalog_module` row).
#[derive(Debug, Clone, Copy)]
pub struct Inputs {
    pub sessions: i64,
    pub hours_per_session: f64,
    pub instructors_per_session: i64,
    /// $/hr per instructor in cents. Appendix G used $100 flat.
    pub facilitator_rate_cents: i64,
    pub ga_cents: i64,
    pub material_cents: i64,
    pub rental_cents: i64,
    /// The price LTE charges for one delivery (catalog list, or a bid
    /// override).
    pub list_price_cents: i64,
    /// Contributed value — reported, never subtracted from cost.
    pub in_kind_cents: i64,
    /// Informational only — not a cost driver in the Appendix G model
    /// (labor is instructor-hours, not headcount). Drives the
    /// per-participant display.
    pub participants: i64,
}

pub const DEFAULT_FACILITATOR_RATE_CENTS: i64 = 10_000; // $100/hr
pub const DEFAULT_GA_CENTS: i64 = 72_500; // $725, modal Appendix G G&A
/// Margins thinner than this get flagged so Travis surfaces them
/// proactively rather than just reporting the number.
pub const THIN_MARGIN_PCT: f64 = 5.0;

#[derive(Debug, Clone, Copy)]
pub struct Breakdown {
    pub labor_hours: f64,
    pub labor_cents: i64,
    pub ga_cents: i64,
    pub material_cents: i64,
    pub rental_cents: i64,
    pub cost_cents: i64,
    pub list_price_cents: i64,
    pub margin_cents: i64,
    pub margin_pct: f64,
    pub in_kind_cents: i64,
    /// `list_price_cents / participants`, or None when participants = 0.
    pub list_per_participant_cents: Option<i64>,
    pub margin_per_participant_cents: Option<i64>,
    pub thin_margin: bool,
}

/// The Appendix G computation. Pure; no I/O.
pub fn compute(i: Inputs) -> Breakdown {
    let labor_hours =
        (i.sessions.max(0) as f64) * i.hours_per_session.max(0.0) * (i.instructors_per_session.max(0) as f64);
    let labor_cents = (labor_hours * i.facilitator_rate_cents.max(0) as f64).round() as i64;
    let cost_cents = labor_cents + i.ga_cents + i.material_cents + i.rental_cents;
    let margin_cents = i.list_price_cents - cost_cents;
    let margin_pct = if i.list_price_cents > 0 {
        (margin_cents as f64) / (i.list_price_cents as f64) * 100.0
    } else {
        0.0
    };
    let (list_pp, margin_pp) = if i.participants > 0 {
        (
            Some(i.list_price_cents / i.participants),
            Some(margin_cents / i.participants),
        )
    } else {
        (None, None)
    };
    Breakdown {
        labor_hours,
        labor_cents,
        ga_cents: i.ga_cents,
        material_cents: i.material_cents,
        rental_cents: i.rental_cents,
        cost_cents,
        list_price_cents: i.list_price_cents,
        margin_cents,
        margin_pct,
        in_kind_cents: i.in_kind_cents,
        list_per_participant_cents: list_pp,
        margin_per_participant_cents: margin_pp,
        thin_margin: margin_pct < THIN_MARGIN_PCT,
    }
}

fn dollars(cents: i64) -> String {
    let neg = cents < 0;
    let c = cents.abs();
    format!("{}${}.{:02}", if neg { "-" } else { "" }, c / 100, c % 100)
}

/// Human-readable breakdown for the LLM tool result. `title` is the
/// module name + line; `shape` is e.g. "5 sessions × 4h × 2 instr".
pub fn render(title: &str, shape: &str, b: &Breakdown) -> String {
    let mut s = format!("{title} — {shape}\n");
    s.push_str(&format!(
        "  Labor      {:>11}   ({}h × {})\n",
        dollars(b.labor_cents),
        // trim trailing .0 for whole hours
        if b.labor_hours.fract() == 0.0 {
            format!("{}", b.labor_hours as i64)
        } else {
            format!("{}", b.labor_hours)
        },
        dollars((b.labor_cents as f64 / b.labor_hours.max(1.0)).round() as i64),
    ));
    s.push_str(&format!("  G&A        {:>11}   (estimate)\n", dollars(b.ga_cents)));
    s.push_str(&format!("  Materials  {:>11}\n", dollars(b.material_cents)));
    s.push_str(&format!("  Rental     {:>11}\n", dollars(b.rental_cents)));
    s.push_str("  ─────────────────────\n");
    s.push_str(&format!("  Cost       {:>11}\n", dollars(b.cost_cents)));
    s.push_str(&format!("  List       {:>11}\n", dollars(b.list_price_cents)));
    s.push_str(&format!(
        "  Margin     {:>11}   ({:.1}%, Appendix G \"Profit\"){}\n",
        dollars(b.margin_cents),
        b.margin_pct,
        if b.thin_margin { "  ⚠ THIN MARGIN" } else { "" },
    ));
    s.push_str(&format!(
        "  In-kind    {:>11}   (reported, not in cost)\n",
        dollars(b.in_kind_cents)
    ));
    if let (Some(lpp), Some(mpp)) = (b.list_per_participant_cents, b.margin_per_participant_cents) {
        s.push_str(&format!(
            "  Per participant: list {} / margin {}\n",
            dollars(lpp),
            dollars(mpp)
        ));
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authentic_leadership_matches_appendix_g() {
        // 2 instr × 8h × 1 session × $100 = $1,600 labor; +$725 G&A
        // = $2,325 cost; list $2,556 → margin $231 (9.0%).
        let b = compute(Inputs {
            sessions: 1,
            hours_per_session: 8.0,
            instructors_per_session: 2,
            facilitator_rate_cents: DEFAULT_FACILITATOR_RATE_CENTS,
            ga_cents: DEFAULT_GA_CENTS,
            material_cents: 0,
            rental_cents: 0,
            list_price_cents: 255_600,
            in_kind_cents: 99_100,
            participants: 0,
        });
        assert_eq!(b.labor_cents, 160_000);
        assert_eq!(b.cost_cents, 232_500);
        assert_eq!(b.margin_cents, 23_100);
        assert!((b.margin_pct - 9.038).abs() < 0.01);
        assert!(b.thin_margin == false);
        assert!(b.list_per_participant_cents.is_none());
    }

    #[test]
    fn one_facilitator_widens_margin() {
        let two = compute(Inputs {
            sessions: 5,
            hours_per_session: 4.0,
            instructors_per_session: 2,
            facilitator_rate_cents: DEFAULT_FACILITATOR_RATE_CENTS,
            ga_cents: DEFAULT_GA_CENTS,
            material_cents: 0,
            rental_cents: 0,
            list_price_cents: 476_900,
            in_kind_cents: 0,
            participants: 40,
        });
        let one = compute(Inputs {
            instructors_per_session: 1,
            ..into_inputs(&two)
        });
        assert!(one.margin_cents > two.margin_cents);
        assert_eq!(two.list_per_participant_cents, Some(11_922));
    }

    // helper: reconstruct Inputs from a Breakdown for the override test
    fn into_inputs(b: &Breakdown) -> Inputs {
        Inputs {
            sessions: 5,
            hours_per_session: 4.0,
            instructors_per_session: 2,
            facilitator_rate_cents: DEFAULT_FACILITATOR_RATE_CENTS,
            ga_cents: b.ga_cents,
            material_cents: b.material_cents,
            rental_cents: b.rental_cents,
            list_price_cents: b.list_price_cents,
            in_kind_cents: b.in_kind_cents,
            participants: 40,
        }
    }
}
