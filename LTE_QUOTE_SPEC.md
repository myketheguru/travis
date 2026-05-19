# LTE Quote / Margin Spec

> **Status:** spec. Follow-on to [`LTE_PACK_SPEC.md`](./LTE_PACK_SPEC.md)
> §10 Q3. Extends the `lead_to_empower` pack (next minor). Source of
> truth: Appendix G cost-budget model, MTAC #R1179.

---

## 1. Why

`LTE_PACK_SPEC` modeled *what* LTE sells (catalog) and *how* it
delivers (the 3 A's) but explicitly **scoped out** pricing scenarios.
The live 46-school NYCPS HS Math pursuit makes that gap expensive:
LTE needs to answer "what's our margin if we run *Developing
Data-Driven Practices* for 40 participants over 6 weeks with one
facilitator instead of two?" before it commits a bid. The Appendix G
cost budgets already encode the model; this spec turns it into
something Travis can compute and persist.

Two surfaces, deliberately split:

1. **A read-only LLM tool** (`lte_quote_margin`) — answers margin
   questions in conversation, computes nothing persistent. This is
   the common case ("ballpark this for me") and the Jarvis-shaped
   one: ask in chat, get the number, no dashboard.
2. **A `quote` typed table** — persists a scenario when the user
   wants to keep/compare it (auto-CRUD, like every other pack table).
   This is the *actively-managed* surface that earns a tab.

Per the minimal-surfaces memory: the tool is the default; the table
exists for the cases the user explicitly says "save this."

---

## 2. The cost model (Appendix G, generalized)

Per single delivery instance:

```
labor_cents     = sessions × hours_per_session
                          × instructors_per_session
                          × facilitator_rate_cents
ga_cents        = flat per-delivery general & admin            (input)
material_cents  = texts / physical materials                   (input, default 0)
rental_cents    = equipment / rental                           (input, default 0)
cost_cents      = labor + ga + material + rental
margin_cents    = list_price_cents − cost_cents        (Appendix G "Profit")
margin_pct      = margin_cents / list_price_cents × 100
in_kind_cents   = contributed value — REPORTED, NOT subtracted (input)
```

**Defaults / assumptions** (reasonable, from the digest; all
overridable):

- `facilitator_rate_cents = 10_000` ($100/hr — every Appendix G
  budget used this for facilitator *and* co-facilitator).
- `ga_cents` default `72_500` ($725 — the modal Appendix G G&A line).
  Flagged as an estimate in the tool output and field help; it does
  not scale with module size in the source budgets, so a flat default
  is faithful, not lazy.
- `material_cents = rental_cents = 0` (every sampled budget had these
  at $0).
- `instructors_per_session`, `sessions`, `hours_per_session` default
  to the catalog row's values; the whole point is overriding them.
- Sanity check against the source: Authentic Leadership — 2 instr ×
  8h × 1 session × $100 = $1,600 labor + $725 G&A = $2,325 cost;
  list $2,556 → margin $231 (9.0%). Matches Appendix G exactly.

Cents everywhere (matches `Currency` fields). Integer math; round
`margin_pct` to one decimal for display.

---

## 3. `lte_quote_margin` — read-only tool

Native Rust tool, registered via the pack's `register_tools`
(currently defaulted). Read-only → no confirm card; the LLM calls it
autonomously when the user asks a pricing question.

**Input schema:**

| field | type | required | notes |
|---|---|---|---|
| `module` | string | yes | catalog line # ("16"), or a name substring ("developing data") — resolved against `catalog_module` |
| `participants` | integer | no | informational + per-head rate; does not change labor in the Appendix G model |
| `instructors` | integer | no | overrides catalog `instructors_per_session` |
| `sessions` | integer | no | overrides catalog `sessions` |
| `hours_per_session` | number | no | overrides catalog `hours_per_session` |
| `facilitator_rate_cents` | integer | no | default 10000 |
| `ga_cents` | integer | no | default 72500 |
| `material_cents` | integer | no | default 0 |
| `rental_cents` | integer | no | default 0 |
| `list_price_cents` | integer | no | default = catalog row's list price (override to test a bid price) |

**Behavior:** resolve the module (error with the closest matches if
ambiguous/none), apply overrides over catalog defaults, compute §2,
return a compact human-readable breakdown **and** a structured block:

```
Developing Data-Driven Practices (line 16) — 5 sessions × 4h × 2 instr
  Labor      $4,000.00   (40h × $100)
  G&A        $  725.00   (estimate)
  Materials  $    0.00
  Rental     $    0.00
  ─────────────────────
  Cost       $4,725.00
  List       $4,769.00
  Margin     $   44.00   (0.9%)        ← thin; flag if < 5%
  In-kind    $  991.00   (not in cost)
  Per participant @ 40: list $119.23 / margin $1.10
```

The LLM gets both the prose and the numbers so it can reason
("margin's under 5% — dropping to one facilitator brings it to
$2,044 / 42.9%"). When `margin_pct < 5`, the tool annotates `THIN
MARGIN` so Travis proactively flags it rather than just reporting.

No DB writes. Reads `catalog_module` only.

---

## 4. `quote` table — persisted scenario

For "save this so I can compare three staffing options on the NYCPS
bid." Auto-CRUD, `primary: true` (it's actively managed), workspace
scoped, follows every pack convention.

| field | type | notes |
|---|---|---|
| `id` | Integer | PK |
| `name` | Text | req — "NYCPS Algebra — 1 facilitator option" |
| `module_id` | Ref `catalog_module` | req |
| `engagement_id` | Ref `engagement` | optional — ties a scenario to a real deal |
| `participants` | Integer | |
| `instructors` | Integer | override (default copied from module at create) |
| `sessions` | Integer | override |
| `hours_per_session` | Number | override |
| `facilitator_rate_cents` | Currency | default 10000 |
| `ga_cents` | Currency | default 72500 |
| `material_cents` | Currency | default 0 |
| `rental_cents` | Currency | default 0 |
| `list_price_cents` | Currency | bid price (default = module list) |
| `in_kind_cents` | Currency | reported, not in cost |
| `notes` | LongText | |
| `created_at`/`updated_at` | Timestamp | |
| ~~`labor/cost/margin_cents`, `margin_pct`~~ | — | **deferred** (option A follow-up); computed live by the tool, not stored |

`display_field "name"`, `entity_kind None` (a quote is not a
cross-pack entity — it's a working document; no spine sync, matches
"don't abstract on n=1"). list columns
`["name","module_id","list_price_cents","margin_cents","margin_pct"]`,
sort `updated_at` Desc.

**Computed fields — what shipped (v0.3.0).** Auto-CRUD can't compute
on write. Three options were considered:

- **A:** a typed `quote` upsert command that recomputes
  `labor/cost/margin*` server-side, bypassing auto-CRUD for this
  table. Faithful to the L2E invoice precedent but requires a custom
  quote form (frontend override) so the auto-CRUD form doesn't write
  past it — out of this slice's scope.
- **B:** store only inputs; compute on read.
- **C (shipped):** the `quote` table stores **inputs only** (no
  stored `labor/cost/margin*` columns); the §3 `lte_quote_margin`
  tool is the compute engine and can be pointed at a saved quote's
  inputs ("margin on the NYCPS 1-facilitator quote" → Travis reads
  the row via auto-CRUD `pack_table_*`, calls the tool). Zero core or
  frontend changes; the tool and table can't drift because there's
  one compute path (`pricing::compute`).

C is B with the compute centralized in `pricing.rs` and exposed
conversationally rather than in the list view. The trade-off vs A:
saved scenarios don't show margin as a sortable column yet — you ask
Travis. **Follow-up to reach A:** add a typed `upsert_quote` command
(calls `pricing::compute`, persists the totals) + a custom quote
form override; then add the `labor_cents/cost_cents/margin_cents/
margin_pct` columns via `0003_quote_computed.sql` and surface them in
`list_view`. Tracked here, not done.

The compute logic is one function — `pricing::compute(Inputs) ->
Breakdown` in `pricing.rs`, with unit tests pinning it to the
Appendix G numbers (Authentic Leadership → $231 / 9.0%). The tool
and any future typed upsert both call it, so they can't drift.

---

## 5. Scope guard

- One facilitator-rate model. No tiered rates by role/seniority — the
  Appendix G budgets used a flat $100 for everyone; honor that until
  there's evidence otherwise.
- No quote→invoice generation. A quote is pre-sale modeling; the
  invoice path (existing) is post-delivery billing. They meet only
  via the optional `engagement_id`.
- No PDF/export for quotes in this slice (the pack's PDF machinery is
  invoice-shaped; revisit if the user wants a client-facing quote
  sheet).
- `ga_cents` flat default is an explicit assumption, surfaced in
  field help + tool output, not hidden.

---

## 6. Open questions (answered, per "make reasonable assumptions")

1. **G&A scaling.** Source budgets don't scale it; flat $725 default,
   overridable. If a future budget shows it scaling with labor, add a
   `ga_pct_of_labor` mode then — not now.
2. **Is `participants` a cost driver?** Not in the Appendix G model
   (labor is instructor-hours, not headcount). Kept as
   informational + per-head rate display only. The per-participant
   number is what LTE actually negotiates on, so it's worth showing
   even though it doesn't move cost.
3. **Profit vs margin naming.** Appendix G calls the residual
   "Profit"; we call it `margin` (clearer, and "profit" overloads
   with the firm's actual P&L). Tool output says "Margin (Appendix G
   'Profit')" once so the mapping is unambiguous to an LTE reader.

---

*Spec written 2026-05-19. Implements as `lead_to_empower` v0.3.0.*
