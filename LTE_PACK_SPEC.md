# LTE Pack Spec — Program Delivery (the "3 A's" + Catalog)

> **Status:** spec, not yet implemented. Extends the shipping
> `lead_to_empower` pack. Source of truth for the domain: the NYC DOE
> **MTAC #R1179** application package + the 2026–27 NYCPS HS Math
> pursuit narrative (digested 2026-05-19).
>
> Read alongside [`PACKS.md`](./PACKS.md) (format spec) and
> [`AUTHORING_PACKS.md`](./AUTHORING_PACKS.md) (build guide). This
> doc only covers the **new** surface; it does not redesign the
> existing pack.

---

## 1. Why this exists

The `lead_to_empower` pack today models the **billing spine**:
`coach`, `school`, `coach_hours`, `signing_sheet`, `invoice` — i.e.
*money out the door*. It models nothing about **what LTE actually
sells or how it delivers it**.

The MTAC digest gave us that missing half:

- **The method is a 3-stage state machine** — LTE calls it the
  "3 A's": **Assessment → Action Planning → Accountable Planning**.
  Every school engagement moves through it, on a roughly fixed
  September→June cadence, with a signed metrics agreement as the gate
  between planning and delivery.
- **The product is a fixed 21-line catalog** (Appendix F) across two
  pillars, each line with a known price, session structure, and
  participant envelope.

This spec encodes both so Travis can (a) track where every engagement
sits in the 3 A's without being told, (b) ground LLM answers in the
real catalog and pricing, and (c) surface the LTE-specific
"what's stuck" metrics — unsigned metrics agreements, overdue
accountability reviews, delivered-but-uninvoiced engagement hours.

It deliberately stays inside the existing pack contract: typed tables
+ `tables.rs` metadata + one new migration + prompt-fragment additions
+ alerts. No core changes.

---

## 2. Design constraints (from project memory)

These are not negotiable for this spec:

- **Track everything; ask only to refine.** Stage transitions in the
  3 A's are *inferred from journal/conversation*, recorded silently.
  Travis never asks "should I track this engagement?" — it already
  did. Clarifying questions only disambiguate ("is the Bronx Science
  walkthrough the assessment for the Algebra engagement, or a new
  one?").
- **Keep Manage minimal; tracking is internal magic.** The catalog,
  engagements, and their sub-records *are* things the user actively
  manages (book of business + contract scope) → they get Manage tabs,
  same justification as coaches/invoices. The 3 A's *stage-inference
  loop*, mention timelines, and graph edges do **not** get a tab —
  Travis advances stages and nudges through conversation.
- **Jarvis north star.** The alerts (§6) exist so Travis is
  *proactive* about accountability and cash, not so the user has a
  dashboard to police. Stage advancement is a *collaboration* surface
  ("looks like Roosevelt is ready to move from assessment to action
  planning — want me to draft the scope?"), not a form.

---

## 3. Data model

Five new typed tables, plus one additive column on the existing
`coach_hours`. New `entity_kinds`: `module`, `engagement`.

```
                        ┌─────────────────┐
                        │ catalog_module  │  (reference: the 21 lines)
                        └────────┬────────┘
                                 │ referenced by
   school ───< engagement >──────┴──────────────┐
   (existing)      │                             │
                   │ 1:N                         │ N:M via
        ┌──────────┼───────────────┐      engagement_module
        ▼          ▼               ▼            │
   assessment  engagement_module  accountability_review
   (the A1)    (the A2 scope)     (the A3 checkpoint)
                   │
                   │ delivery rolls up via
                   ▼
   coach_hours.engagement_id  (existing table, +1 column)
```

### 3.1 `catalog_module` — the priced product lines

Reference data. Seeded from Appendix F (§8). User-editable because
LTE re-prices between solicitations, but mostly read.

| field | type | notes |
|---|---|---|
| `id` | Integer | PK |
| `line_no` | Integer | Appendix F line # (1–21), stable external ref |
| `name` | Text | e.g. "Authentic Leadership Module" |
| `pillar` | Enum | `leadership_development` \| `dddm_teacher_effectiveness` |
| `grade_band` | Text | "Elementary, Middle, High" (free text; usually all) |
| `audience` | LongText | target roles |
| `description` | LongText | Appendix F mandatory description |
| `list_price_cents` | Currency | total price per delivery (G col) |
| `sessions` | Integer | sessions per delivery (H) |
| `hours_per_session` | Number | (I) |
| `duration_weeks` | Integer | (J) |
| `min_participants` | Integer | (L) |
| `max_participants` | Integer | (M) |
| `instructors_per_session` | Integer | (N) — 2 for workshops, 1 for coaching |
| `kind` | Enum | `workshop` \| `coaching` \| `school_assessment` |
| `created_at`/`updated_at` | Timestamp | conventions |

`TableDef`: `display_name "Catalog"`, `singular "Module"`,
`display_field "name"`, `entity_kind Some("module")`, `primary true`,
list columns `["line_no","name","pillar","list_price_cents"]`, sort
`line_no` Asc.

### 3.2 `engagement` — the 3 A's state machine

The unit everything else hangs off. One per contracted scope of work
at a school (an engagement *is* a 3 A's run).

| field | type | notes |
|---|---|---|
| `id` | Integer | PK |
| `name` | Text | e.g. "Roosevelt HS — Algebra I implementation 26-27" |
| `school_id` | Ref `school` | required |
| `stage` | Enum | `assessment` \| `action_planning` \| `accountable` \| `closed` |
| `contract_ref` | Text | e.g. "MTAC R1179" / "NYCPS HS Math — Supt. White" |
| `school_year` | Text | "2026-2027" |
| `metrics_agreement_signed` | Bool | the gate between planning & delivery |
| `metrics_signed_on` | Date | nullable |
| `summary` | LongText | rolling narrative |
| `created_at`/`updated_at` | Timestamp | |

`entity_kind Some("engagement")`, `primary true`, list
`["name","school_id","stage","metrics_agreement_signed"]`, default
sort `updated_at` **Desc** (most-active first).

**Stage semantics** (the actual 3 A's):

| stage | meaning | entered when | exit gate |
|---|---|---|---|
| `assessment` | diagnosing the school | engagement created | ≥1 `assessment` row recorded |
| `action_planning` | scoping the work | assessment shared w/ SLT | ≥1 `engagement_module` row |
| `accountable` | delivering + monitoring | `metrics_agreement_signed = 1` | school year ends / goals met |
| `closed` | reflection done | final `accountability_review` `met` set | — |

Stage is **inferred and advanced by Travis**, confirmed in
conversation — never a required form field the user fills manually.

### 3.3 `assessment` — the A1 record

The diagnostic. Multiple per engagement (a walkthrough + a survey + a
data pull all count).

| field | type | notes |
|---|---|---|
| `engagement_id` | Ref `engagement` | required |
| `method` | Enum | `leadership_survey` \| `personal_eval` \| `walkthrough` \| `observation` \| `interview` \| `data_analysis` |
| `conducted_on` | Date | |
| `rubric_score` | Number | nullable — comprehensive leadership rubric score |
| `recommended_focus` | LongText | the "areas to concentrate on" output |
| `summary` | LongText | |

`primary false` (reached via the engagement detail), `entity_kind None`
(it's an event on the engagement, mirrored to the spine `event` table
as `kind = "assessment_recorded"` — see §5).

### 3.4 `engagement_module` — the A2 scope

Join table: which catalog modules are in scope for this engagement,
at what agreed terms (price/participant count deviate from list — the
rate card is a starting point, not a fixed SKU).

| field | type | notes |
|---|---|---|
| `engagement_id` | Ref `engagement` | required |
| `module_id` | Ref `catalog_module` | required |
| `planned_start` | Date | |
| `planned_end` | Date | |
| `participant_count` | Integer | drives per-head economics |
| `agreed_price_cents` | Currency | defaults to module list price |
| `coaching_sessions_planned` | Integer | ~10–11/participant for the full arc |
| `notes` | LongText | |

`primary false`, mirrored to spine as a `relation`
(`engagement` —`includes_module`→ `module`).

### 3.5 `accountability_review` — the A3 checkpoint

The metrics reviews. Cadence from the MTAC: ~3/year.

| field | type | notes |
|---|---|---|
| `engagement_id` | Ref `engagement` | required |
| `period` | Enum | `baseline_sep` \| `mid_jan` \| `reflection_jun` |
| `review_date` | Date | |
| `metrics_json` | Json | goals / metrics / milestones + actuals |
| `met` | Bool | nullable until reflection |
| `notes` | LongText | |

`primary false`, mirrored to spine `event`
(`kind = "accountability_review"`).

### 3.6 Bridge to billing — `coach_hours.engagement_id`

Additive migration on the **existing** `coach_hours` table:

```sql
ALTER TABLE coach_hours ADD COLUMN engagement_id INTEGER
    REFERENCES engagement(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_coach_hours_engagement
    ON coach_hours(engagement_id);
```

Nullable, soft FK — pre-existing hours keep `NULL` and render `—`.
This is the join that makes delivered program work roll up to the
engagement and into the money-at-risk alert. **No edit to the
shipped `0001_init.sql`** — new migration file `0002_program_delivery.sql`,
appended to the `MIGRATIONS` slice, pack `version()` → minor bump.

---

## 4. Migration file

`src-tauri/src/packs/lead_to_empower/migrations/0002_program_delivery.sql`
— `CREATE TABLE IF NOT EXISTS` for the five new tables (id PK,
created_at/updated_at conventions, FKs `ON DELETE CASCADE` for the
engagement children, `ON DELETE RESTRICT` from `engagement_module`
into `catalog_module` so a module in use can't be deleted), the
`coach_hours` ALTER, indexes on every FK and on `engagement(stage)`,
then the catalog seed `INSERT`s (§8). Re-runnable; LF-pinned.

`tables.rs`: append five `TableDef`s + their `FieldDef` arrays to the
`TABLES` slice, in the order
`catalog_module, engagement, assessment, engagement_module,
accountability_review`. `mod.rs`: add `"module"`, `"engagement"` to
`entity_kinds()`, add the `0002` `PackMigration`, extend
`PROMPT_FRAGMENT` (§7), add the new `AlertDef`s (§6).

---

## 5. Spine sync

Per [`PACKS.md`](./PACKS.md) §"data model" — explicit writes, no
triggers:

- `catalog_module` upsert → `entity` (`kind="module"`,
  `pack_slug="lead-to-empower"`, `attributes_json` = pillar + price).
  Auto-CRUD handles this via `entity_kind`.
- `engagement` upsert → `entity` (`kind="engagement"`), **and** a
  `relation` `engagement` —`at_school`→ `school`.
- `assessment` insert → `event` `kind="assessment_recorded"` on the
  engagement entity.
- `engagement_module` insert → `relation` `engagement`
  —`includes_module`→ `module`.
- `accountability_review` insert → `event`
  `kind="accountability_review"` (attributes: period, met).
- Stage change on `engagement` → `event`
  `kind="stage_changed"` (attributes: from, to). This is the trail
  Travis reasons over for proactivity.

`assessment`, `engagement_module`, `accountability_review` need typed
domain modules (they have logic beyond field validation — stage
inference, the signed-metrics gate). `catalog_module` and
`engagement` can ride auto-CRUD; if a typed `engagement` module is
added later for stage logic, drop its `entity_kind` to avoid
double-sync (per AUTHORING_PACKS.md warning).

---

## 6. Alerts — the LTE "what's stuck"

Existing money alert (`uninvoiced_hours`) stays. Add three, scoped to
the 3 A's. One concern per alert (no mega-alert).

| slug | severity | the question | SQL shape |
|---|---|---|---|
| `unsigned_metrics_agreement` | `Action` | scope built, delivery started, but the metrics agreement isn't signed — accountability debt and a contract-risk gap | engagements where `stage IN ('action_planning','accountable')` AND `metrics_agreement_signed = 0` |
| `overdue_accountability_review` | `Money` | an active engagement is past a period checkpoint with no review for it — unreviewed metrics is what loses the renewal | `stage = 'accountable'`, school_year current, and the period's expected month has passed with no matching `accountability_review` row |
| `stalled_assessment` | `Action` | engagement opened, no assessment recorded in 21 days — the diagnostic stalled | `stage = 'assessment'` AND no `assessment` row in 21d |

Each returns the one-row `count / sample_label / sample_id` contract;
`sample_*` points at the engagement so Splash deep-links to it.
`uninvoiced_hours` can now read `coach_hours.engagement_id` to label
*which engagement* has money on the table.

---

## 7. Prompt-fragment additions

Appended to the existing L2E fragment (keep total < ~150 words of
*new* text):

```
LTE delivery runs the "3 A's": every school engagement moves
Assessment → Action Planning → Accountable Planning → closed.
- Assessment: surveys, walkthroughs, observations, data analysis
  against the leadership rubric. Record each as an assessment on the
  engagement; this is also when the diagnostic is happening.
- Action Planning: the scope of work — which catalog modules, for
  whom, when. The signed metrics agreement gates the move into
  delivery.
- Accountable: delivering modules + ~3 metrics reviews/year
  (Sept baseline, Jan mid, May/June reflection).
The catalog is 21 priced modules across two pillars (Leadership
Development; Data-Driven Decision-Making & Teacher Effectiveness).
When the user mentions a school, walkthrough, module, or metrics
review, record it against the right engagement even if no action is
asked — and if the mention implies the engagement has moved stages,
note it and confirm the transition in conversation rather than
asking permission to track.
```

This makes the journal extractor route "did the Lincoln walkthrough"
into an `assessment` against the Lincoln engagement, and lets Travis
*propose* the stage advance instead of waiting to be told.

---

## 8. Catalog seed data (Appendix F, verbatim)

`pillar`: **LD** = `leadership_development`, **DD** =
`dddm_teacher_effectiveness`. Price in dollars (store ×100 as cents).
`kind`: workshop unless noted. Grade band is "Elementary, Middle,
High" for all. Audience: lines 1–12,17,20 = "Lead Teachers, AP,
coaches, principals, other school-based instructional/admin staff";
13–16,18,19,21 = "New & experienced teachers / school / admin staff".

| # | name | pillar | $ | sess×hr | wks | min–max | instr | kind |
|--|--|--|--|--|--|--|--|--|
| 1 | Authentic Leadership | LD | 2,556 | 1×8 | 1 | 2–25 | 2 | workshop |
| 2 | Visionary Leadership | LD | 3,435 | 3×4 | 4 | 2–25 | 2 | workshop |
| 3 | Servant Leadership | LD | 2,556 | 1×8 | 1 | 2–25 | 2 | workshop |
| 4 | Instructional Leadership Pt 1 | LD | 3,655 | 4×4 | 5 | 2–25 | 2 | workshop |
| 5 | Instructional Leadership Pt 2 | LD | 3,655 | 4×4 | 5 | 2–25 | 2 | workshop |
| 6 | Instructional Leadership Pt 3 | LD | 3,655 | 4×4 | 5 | 2–25 | 2 | workshop |
| 7 | Instructional Leadership Pt 4 | LD | 3,655 | 4×4 | 4 | 2–25 | 2 | workshop |
| 8 | Instructional Leadership Pt 5 | LD | 3,655 | 4×4 | 4 | 2–25 | 2 | workshop |
| 9 | Transformational Leadership Pt 1 | LD | 2,996 | 3×4 | 3 | 2–25 | 2 | workshop |
| 10 | Transformational Leadership Pt 2 | LD | 2,996 | 3×4 | 3 | 2–25 | 2 | workshop |
| 11 | Transformational Leadership Pt 3 | LD | 2,996 | 3×4 | 4 | 2–25 | 2 | workshop |
| 12 | Adaptive Leadership | LD | 3,655 | 2×8 | 2 | 2–25 | 2 | workshop |
| 13 | Self-Reflection Methods for Learning & Effectiveness | DD | 3,216 | 2×5 | 1 | 2–25 | 2 | workshop |
| 14 | Equity & Access to Drive Relationship Building | DD | 3,670 | 2×6 | 2 | 2–25 | 2 | workshop |
| 15 | Assessment & Analysis | DD | 3,889 | 4×4 | 6 | 5–50 | 2 | workshop |
| 16 | Developing Data-Driven Practices | DD | 4,769 | 5×4 | 6 | 2–25 | 2 | workshop |
| 17 | Leadership Coaching | LD | 2,993 | 4×4 | 4 | 1–5 | 1 | coaching |
| 18 | Instructional Coaching | DD | 2,949 | 4×4 | 4 | 1–5 | 1 | coaching |
| 19 | Data Coaching | DD | 3,532 | 5×4 | 4 | 1–5 | 1 | coaching |
| 20 | School Assessment (Leadership) | LD | 3,461 | 2×7 | 1 | 30–200 | 2 | school_assessment |
| 21 | School Assessment (Data & Teacher Effectiveness) | DD | 3,461 | 2×7 | 1 | 30–200 | 2 | school_assessment |

Descriptions/audience copy verbatim from Appendix F live in the
migration's `INSERT`s (omitted here for length; the rate-card
extraction in session history is the source).

Per-delivery cost model (Appendix G, for a future margin tool — not
in this spec's tables): labor = (sessions × hours × instructors) ×
**$100/hr**; add G&A (~$725 typical), profit (~9%), tracked in-kind
(~$991). e.g. Authentic Leadership: 2 instr × 8h × $100 = $1,600
labor + $725 G&A + $231 profit = $2,556 list. Flagged in §10.

---

## 9. What this is NOT (scope guard)

- Not a margin/quoting calculator. The Appendix G cost model is
  documented (§8) but not modeled — that's a follow-on `quote` table
  if the user wants pricing scenarios.
- Not a coaching-session tracker per participant. The ~10–11
  sessions/participant are captured as a planned count on
  `engagement_module` and delivered as `coach_hours` rows tagged with
  `engagement_id`. A per-session table is a Phase-2 add if granular
  coaching logs are needed.
- Not a new pack. This is `lead_to_empower` v(minor)+1.
- No new Manage tab for the inference loop, stage timeline, or graph
  edges (memory: minimal surfaces). Only `Catalog` and `Engagements`
  become tabs; assessments/scope/reviews render inside the engagement
  detail.

---

## 10. Open questions

1. **Stage auto-advance vs. confirm.** Spec says Travis infers and
   *proposes* the transition in conversation. Should crossing a gate
   (e.g. metrics agreement signed) auto-advance `accountable` with a
   passive notice, or always wait for a confirm? Leaning auto-advance
   with notice — it matches "track everything," and the confirm
   surface is reserved for irreversible/external actions.
2. **`metrics_agreement_signed` source of truth.** Bool on the
   engagement, or derived from an `accountability_review` of
   `period = baseline_sep` existing? Bool is simpler and is the alert
   driver; derived is DRYer. Spec picks the Bool; revisit if signing
   the baseline review should be the single act.
3. **Margin tool.** Worth a `quote` table seeded from the §8 cost
   model so Travis can answer "what's our margin if we run
   Developing Data-Driven Practices for 40 kids over 6 weeks"? Likely
   yes given the live 46-school NYCPS bid, but out of scope here —
   flag for the next spec.
4. **Contract grouping.** The NYCPS HS Math pursuit is 46 schools
   under one solicitation. Is `contract_ref` (free text on each
   engagement) enough, or does a `contract` parent table earn its
   place? Free text for v1; a real table when a second multi-school
   contract appears (the "3 verticals" bar, applied to LTE: don't
   abstract on n=1).

---

*Spec written 2026-05-19 from the MTAC #R1179 digest. The business
model and the live pursuit are in project memory
(`project_lte_business`); compliance facts in
`reference_lte_compliance`.*
