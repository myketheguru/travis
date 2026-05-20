# LTE Invoicing Spec — the document layer + validators + PDFs

> **Status:** spec. Follow-on to [`LTE_PACK_SPEC.md`](./LTE_PACK_SPEC.md)
> and [`LTE_QUOTE_SPEC.md`](./LTE_QUOTE_SPEC.md). Extends the
> `lead_to_empower` pack (next minor: v0.4.0). Source of truth: the
> COO's recorded walkthrough + the four PS/MS 498 documents (PO
> `WR260363316`, work order, sign-in sheet, invoice `LTE2064981`)
> she shared on 2026-05-20, plus her two follow-up emails.

---

## 1. Why this exists

`LTE_PACK_SPEC` modeled the *what* (catalog) and *how* (3 A's).
`LTE_QUOTE_SPEC` modeled the *pre-sale* margin question. Neither
covered the **post-sale billing chain** that consumes most of the
COO's day: turning delivered work into a paid invoice via NYC DOE's
five-document gauntlet — Contract → Work Order → Purchase Order →
Sign-in Sheet → Invoice → Polaris submission.

The COO walked the workflow on camera. The PS 498 submission she
sent us has **five distinct errors** that Travis can catch
deterministically once the document layer exists:

1. Leadership Coaching billed at `$5,013.30/unit`; catalog says
   `$2,993`. The `$5,013.30` is exactly Instructional Coaching's
   total — a copy-paste from the wrong PO line.
2. The 4/30 Data Coaching row in the sign-in sheet is recorded
   under Leadership Coaching on the invoice.
3. Work Order date `02/15/2025` against an engagement whose PO
   period is `02/02/2026 – 06/30/2026` — a typo.
4. Invoice qty 2 for Leadership but subtotal `$5,013.30` — the
   internal arithmetic on the invoice doesn't even agree with
   itself.
5. The 4/30 Data Coaching session is on the sign-in sheet but
   absent from the invoice's Data Coaching date list.

Compounded across the 3-schools-a-week cadence the COO described,
the error surface is huge. Every error above is catchable from
typed data the pack already has (catalog prices, engagement
modules, sign-in sheet hours) **once we add the document tables
that wrap them**. That's what this spec is for.

The second motivation is in her follow-up email:

> *Jacob doesn't have a proper system in place for tracking when
> the last invoice was submitted, so we need Travis to understand
> this. Jacob goes from Memory and we need to make sure we are
> able to follow the dates properly to avoid overlaps.*

A persistent ledger of "what's been billed against what PO" turns
Jacob's memory into a database query. The
`overlapping_invoice_period` alert (§7) is the surface for that.

---

## 2. Design constraints (from project memory)

- **Track everything; ask only to refine.** When the COO mentions a
  PO arriving, Travis records it against the right engagement
  without asking permission. Same for invoice draft proposals — the
  draft is built, surfaced for review, never auto-sent.
- **Keep Manage minimal; tracking is internal magic.** New tabs are
  warranted *only* for things the user actively manages. **Work
  Orders, Purchase Orders, Invoices** get tabs (already do, mostly
  — invoice has one). **Validators, alerts, the spine event log**
  do not — they surface in conversation or as alerts on Splash.
- **Jarvis north star.** Validators don't just refuse; they propose
  the fix. *"Leadership Coaching list is $2,993, you have
  $5,013.30 — want me to correct it?"* The point is to make Travis
  feel like a colleague checking your math, not a form that won't
  submit.
- **Don't abstract on n=1.** A sibling consulting firm is the next
  beta. Templates parameterize company branding from
  `company_profile` (single row, workspace-scoped) — same PDFs,
  swap the profile. **No new pack** until a second tenant actually
  onboards.
- **Operator-mode UX.** The COO processes 3 schools a session; the
  UI surfaces bulk drafting and cross-engagement filters, not
  one-engagement-at-a-time forms.

---

## 3. Data model

Three new tables, one new join table, several columns added to
existing tables. No new entity_kinds — work orders, POs, and
invoice lines are documents/line-items, not graph entities (they
live on the engagement, which is already an entity).

```
                       ┌────────────────────┐
                       │  company_profile   │  (1 row per workspace)
                       └────────────────────┘
                                ▲
                                │ branding pulled by every PDF

   engagement ──< work_order >──< purchase_order >──< invoice
       │                                                 │
       │  1:N                                            │  1:N
       ▼                                                 ▼
   engagement_module (+qty)  ◀────── invoice_line ───────┘
       │                                  (qty, unit_price snapshot)
       │  evidence rolls up via
       ▼
   coach_hours (existing — engagement_id, +engagement_module_id)
```

### 3.1 `company_profile` — the brand strip

Single row per workspace. Default values cribbed from the LTE
sample on first install; the COO edits her own values via the
auto-CRUD form on the Settings → Company tab.

| field | type | notes |
|---|---|---|
| `id` | Integer | PK |
| `workspace_id` | Integer | unique — one profile per workspace |
| `name` | Text | "Lead to Empower" |
| `legal_name` | Text | "Lead to Empower LLC" |
| `address_line_1` | Text | "533 West 141st St." |
| `address_line_2` | Text | nullable |
| `city` | Text | "New York" |
| `state` | Text | "NY" |
| `zip` | Text | "10031" |
| `phone` | Text | "212-222-4275" |
| `email` | Text | "jacob@leadtoempower.com" |
| `website` | Text | "www.leadtoempower.com" |
| `ein` | Text | "82-4991893" |
| `nyc_doe_vendor_number` | Text | "LEA991893" — used on every PO/invoice |
| `default_contract_ref` | Text | "QR179CF" — pre-filled on new engagements |
| `tagline` | Text | "Because Leading is Not Enough" |
| `logo_path` | Text | local path to PNG/SVG for PDF rendering |
| `default_invoice_signature_authority` | Text | name pre-filled into invoice "Authorized by" |
| `created_at`/`updated_at` | Timestamp | |

`TableDef`: `display_name "Company"`, `singular "Profile"`,
`display_field "name"`, `entity_kind None`, `primary false` (it
shows up via Settings rather than as a main tab — there's only one
row), list view limited to `["name","nyc_doe_vendor_number",
"default_contract_ref"]`.

### 3.2 `work_order` — the engagement contract artifact

Vendor-issued, school-countersigned. One per engagement. The
contractual scope agreement that triggers PO issuance.

| field | type | notes |
|---|---|---|
| `id` | Integer | PK |
| `workspace_id` | Integer | |
| `engagement_id` | Ref `engagement` | required |
| `contract_ref` | Text | copy of `engagement.contract_ref` at WO time |
| `date_issued` | Date | the vendor-side issue date |
| `vendor_signed_at` | DateTime | nullable until vendor signs |
| `vendor_signed_by_name` | Text | nullable — defaults to Jacob |
| `school_signed_at` | DateTime | **principal signs all docs** (Q4) — nullable until then |
| `school_signed_by_name` | Text | nullable — principal name |
| `total_cents` | Currency | denormalised — sum of `engagement_module.qty × agreed_price_cents` for the engagement, at the time of signing |
| `pdf_path` | Text | nullable — path to the generated WO PDF |
| `notes` | LongText | |
| `created_at`/`updated_at` | Timestamp | |

`TableDef`: `display_name "Work Orders"`, `singular "Work Order"`,
`display_field` constructed (`engagement.name + " WO"`),
`entity_kind None`, `primary true`, list columns
`["engagement_id","date_issued","total_cents",
"school_signed_at"]`, default sort `date_issued` Desc.

`workspace_id` from start; auto-CRUD stamps it.

### 3.3 `purchase_order` — the school's authorization to bill

School-issued (signed by principal + DOE), received by LTE. One
per work order, typically. Carries the `WR…` PO number — the
external identifier everything downstream references.

| field | type | notes |
|---|---|---|
| `id` | Integer | PK |
| `workspace_id` | Integer | |
| `engagement_id` | Ref `engagement` | required |
| `work_order_id` | Ref `work_order` | nullable — link to the WO that triggered it |
| `po_number` | Text | unique within workspace — `WR260363316` |
| `suffix` | Text | default `"01"` — per Q2, always `01` in observed data |
| `tracking_number` | Text | nullable |
| `po_date` | Date | DOE-side issue date |
| `activity_start` | Date | required — the billable window opens |
| `activity_end` | Date | required — the billable window closes |
| `deliver_to_attention` | Text | "Carol Ann Gilligan" |
| `deliver_to_address` | Text | full school address |
| `deliver_to_phone` | Text | |
| `special_delivery` | LongText | "ROOM 105…" |
| `authorized_by` | Text | the principal/DOE name on the PO |
| `authorized_at` | DateTime | the signature date |
| `total_cents` | Currency | reconciles to `engagement_module` totals |
| `pdf_path` | Text | the PDF the school sent — uploaded, not generated |
| `notes` | LongText | |
| `created_at`/`updated_at` | Timestamp | |

`TableDef`: `display_name "Purchase Orders"`, `singular
"Purchase Order"`, `display_field "po_number"`, `entity_kind None`
(POs aren't graph entities), `primary true`, list columns
`["po_number","engagement_id","po_date","activity_start",
"activity_end","total_cents"]`, default sort `po_date` Desc.

PO PDFs are *inbound* — Taylor uploads the PDF she gets from DOE.
We don't author POs.

### 3.4 `engagement_module.qty` — billable units per module

Additive column on the existing `engagement_module` table:

```sql
ALTER TABLE engagement_module ADD COLUMN qty REAL NOT NULL DEFAULT 1.0;
```

Captures the scope quantity per module — `2.0` for "2 days Data
Coaching", `1.7` for "1.7 days Instructional Coaching". Existing
rows backfill to `1.0`; new rows default to `1.0`. The PO and WO
both render line items from `engagement_module`; this is the field
that makes that rendering accurate.

### 3.5 `invoice_line` — multi-line invoices, the bridge to scope

New table. Each invoice has 1..N lines; each line points to one
`engagement_module` row and snapshots its qty + price *at billing
time* (so later edits to the scope don't retroactively rewrite
sent invoices).

| field | type | notes |
|---|---|---|
| `id` | Integer | PK |
| `workspace_id` | Integer | |
| `invoice_id` | Ref `invoice` | required, ON DELETE CASCADE |
| `engagement_module_id` | Ref `engagement_module` | required, ON DELETE RESTRICT |
| `description` | Text | denormalised "DATA COACHING" string for the PDF |
| `qty` | Number | how much of this module is billed on *this* invoice (can be partial — e.g. 1 of 2 days) |
| `unit_price_cents` | Currency | snapshot of `engagement_module.agreed_price_cents` at billing |
| `subtotal_cents` | Currency | denormalised — `qty × unit_price_cents`, rounded to cents |
| `date_list` | LongText | the rendered "Jan: 29, Feb: 24…" string for the PDF (computed from coach_hours filtered to this module + invoice period) |
| `created_at` | Timestamp | |

`TableDef`: not primary (reached via the invoice detail), but
*does* have auto-CRUD for editing — Taylor needs to be able to
add/remove a line by hand when proposing a draft.

### 3.6 New columns on `invoice`

```sql
ALTER TABLE invoice ADD COLUMN engagement_id INTEGER
    REFERENCES engagement(id) ON DELETE SET NULL;
ALTER TABLE invoice ADD COLUMN purchase_order_id INTEGER
    REFERENCES purchase_order(id) ON DELETE SET NULL;
ALTER TABLE invoice ADD COLUMN school_signed_at TEXT;
ALTER TABLE invoice ADD COLUMN school_signed_by_name TEXT;
ALTER TABLE invoice ADD COLUMN submitted_to_polaris_at TEXT;
CREATE INDEX IF NOT EXISTS idx_invoice_engagement ON invoice(engagement_id);
CREATE INDEX IF NOT EXISTS idx_invoice_po ON invoice(purchase_order_id);
```

The existing `coach_id`, `signing_sheet_id`, `period_start`,
`period_end`, `hours_total`, `rate_cents`, `amount_cents`, `status`
all stay. The single-rate `rate_cents`/`hours_total` model
continues to support the current after-school enrichment use case;
the multi-module program-delivery case uses `invoice_line` and the
header `rate_cents = 0`. Both shapes coexist via `invoice_line`'s
presence/absence.

Validator: when `invoice_line` rows exist, `invoice.amount_cents`
must equal `SUM(invoice_line.subtotal_cents)`. Recomputed on every
write. Mismatch → can't transition to `sent`.

### 3.7 New column on `coach_hours`

```sql
ALTER TABLE coach_hours ADD COLUMN engagement_module_id INTEGER
    REFERENCES engagement_module(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_coach_hours_engagement_module
    ON coach_hours(engagement_module_id);
```

Already has `engagement_id` from v0.3.0. Adding
`engagement_module_id` lets a sign-in row say *which scope item it
served* — e.g. *this 8-hour Tuesday was for Data Coaching, not for
Leadership Coaching*. This is what makes the date-list per invoice
line computable, and what would have caught the 4/30
misclassification in the PS 498 sample.

Nullable; rows without it just don't contribute to per-module date
lists (they still count in `coach_hours` aggregates).

---

## 4. Migration file

`src-tauri/src/packs/lead_to_empower/migrations/0003_invoicing.sql`

`CREATE TABLE IF NOT EXISTS` for `company_profile`, `work_order`,
`purchase_order`, `invoice_line`. `ALTER TABLE` for
`engagement_module` (qty), `invoice` (engagement_id,
purchase_order_id, school_signed_*, submitted_to_polaris_at), and
`coach_hours` (engagement_module_id). All FK indexes. One seed
INSERT for the default `company_profile` row using the LTE
constants (Taylor edits later via Settings).

`tables.rs`: append four `TableDef`s in the order
`company_profile, work_order, purchase_order, invoice_line`.
Existing `engagement_module` TableDef gets a new `qty` field added
to its `FieldDef` array.

`mod.rs`: add the `0003` `PackMigration`. No new entity_kinds.
Bump `version()` → `"0.4.0"`.

---

## 5. Spine sync

Per [`PACKS.md`](./PACKS.md) and the v0.3.0 pattern:

- `work_order` insert → `event` `kind="work_order_issued"` on the
  engagement entity (attributes: total_cents, date_issued).
- `work_order.school_signed_at` flip → `event`
  `kind="work_order_signed"`.
- `purchase_order` insert → `event`
  `kind="purchase_order_received"` (attributes: po_number,
  total_cents, activity_start, activity_end).
- `invoice` status transition (`draft → sent`) → existing
  `event` (no change).
- `invoice.submitted_to_polaris_at` set → `event`
  `kind="invoice_submitted_to_polaris"`.

No `relation` rows beyond what already exists. Engagements are the
graph anchor; documents hang off as events.

`company_profile` does **not** spine-sync — it's pack config, not
domain data.

---

## 6. Validators

Three deterministic checks that run on the `draft → sent`
transition for invoices. Each is **specific** — surfaces the
issue, proposes the fix, never silently mutates.

| slug | when | the check | what it says |
|---|---|---|---|
| `invoice_unit_price_matches_scope` | every invoice_line | `unit_price_cents == engagement_module.agreed_price_cents`, OR if `agreed_price_cents = 0`, equals `catalog_module.list_price_cents` | *"Line 2: Leadership Coaching is $2,993 in the catalog, not $5,013.30. The $5,013.30 looks like Instructional Coaching's total ($2,949 × 1.7) — likely a copy from the wrong PO line. Want me to fix it?"* |
| `invoice_subtotal_arithmetic` | every invoice_line + header | `subtotal_cents == round(qty × unit_price_cents)` AND `amount_cents == SUM(subtotal_cents)` | *"Leadership Coaching qty 2 × $2,993 = $5,986, but the line shows $5,013.30. The math on the invoice doesn't agree with itself."* |
| `invoice_within_po_window` | invoice with linked PO | every `coach_hours.session_date` in the invoice's `[period_start, period_end]` falls inside `[purchase_order.activity_start, activity_end]` | *"3 sign-in days (4/30, 5/1, 5/5) fall outside the PO's activity window (ends 6/30/2026 — wait, this is fine; example only when out of window)."* |

One more validator runs on the `work_order` insert/update:

| slug | when | the check | what it says |
|---|---|---|---|
| `wo_date_within_school_year` | every WO save | `date_issued`'s year ∈ `engagement.school_year` (parsed as `"2026-2027"` → `[2026, 2027]`) | *"WO date is 02/15/**2025** but this engagement's school year is 2026-2027 — probably a typo, should be 2026?"* |

Validators live in `src-tauri/src/packs/lead_to_empower/domain/
invoice.rs` (extend the existing module) and a new
`domain/work_order.rs`. They return a `Vec<ValidationIssue>`; the
status-transition command refuses with that list if non-empty,
which the frontend renders as actionable fixes.

---

## 7. Alerts

One new alert, scoped to the engagement so multi-engagement
schools don't false-positive (Q3):

| slug | severity | the question | SQL shape |
|---|---|---|---|
| `overlapping_invoice_period` | `Money` | two non-void invoices for the same engagement cover overlapping date ranges, OR an invoice period falls outside its linked PO's activity window | join `invoice i1` to `invoice i2 ON i1.engagement_id = i2.engagement_id AND i1.id < i2.id AND i1.period_end >= i2.period_start AND i2.status != 'void' AND i1.status != 'void'`; UNION with `invoice` where `period_start < purchase_order.activity_start OR period_end > purchase_order.activity_end` |

Returns the one-row `count / sample_label / sample_id` contract;
sample points at the engagement so Splash deep-links to it.

The existing `uninvoiced_hours` alert keeps its current SQL — it
already does the "money waiting to be invoiced" job. After this
spec lands, it benefits from the `engagement_id` on `coach_hours`
to label *which engagement* has uninvoiced money.

---

## 8. Document templates

Three PDF generators, all in `src-tauri/src/packs/lead_to_empower/
pdf/`. Each pulls `company_profile` for branding and produces an
A4-portrait single-page (multi-page if line items overflow) PDF
using `printpdf` (already in deps, used by the existing invoice
PDF generator). All three accept `(pool, entity_id) -> PathBuf`.

### 8.1 `pdf::work_order::render(pool, work_order_id)`

Layout matches NYC DOE's *Systemwide Professional Services
Requirements Contract Work Order* (the form Taylor sent):

- Header: NYC DOE seal + "OFFICE OF THE CHANCELLOR" strip
- Title: "SYSTEMWIDE PROFESSIONAL SERVICES REQUIREMENTS CONTRACT
  WORK ORDER"
- Vendor block (from `company_profile`): name, address, contact,
  phone, email
- District/School block (from `engagement.school`): name, address,
  district #
- Contract # and Vendor # (from `company_profile` +
  `engagement.contract_ref`)
- "I hereby certify…" certification block
- Scope of Work table: rendered from `engagement_module` rows
  (name, unit, unit_cost, qty, total)
- Total Cost line
- Vendor signature block (from `vendor_signed_at`,
  `vendor_signed_by_name`)
- Principal signature block (from `school_signed_at`,
  `school_signed_by_name`) — blank if unsigned
- "FOR DEPT. OF EDUCATION USE ONLY" footer with blank PO Number and
  Location Code lines (DOE fills in)

Output: `<downloads>/lte-wo-<engagement-slug>-<date>.pdf`.

### 8.2 `pdf::sign_in_sheet::render(pool, engagement_id, period_start, period_end)`

Replaces Taylor's Excel-cleanup dance entirely. Reads
`coach_hours` rows in the period for the engagement, groups by
`engagement_module_id`, renders one consolidated table per the
LTE-internal layout from the sample:

| Date of Work | School | Scope of Work | Staff Supported / Included | Brief Description | Quantity of Time |

Bottom of table:
- Total hours
- Total billable days (sum of hours ÷ each module's
  `sessions × hours_per_session × instructors_per_session` from
  catalog → fractional deliveries, summed)
- Principal signature block (`signing_sheet.signed_at`,
  `signed_by` if linked)

Pre-formatted column widths (`Date 25mm`, `School 25mm`, `Scope
35mm`, etc. — the format Taylor spends time on every week is baked
in). Output to Downloads. No Excel involved.

### 8.3 `pdf::invoice::render(pool, invoice_id)` — rewrite

The existing `pdf/mod.rs` renders a single-line invoice. Rewrite
to support the LTE letterhead + multi-line case. Layout from the
LTE2064981 sample:

- Header: company logo (from `company_profile.logo_path`) + name
  on a teal/gold strip
- Tagline ("Because Leading is Not Enough")
- 533 West 141st St, NY, NY 10031 + `www.leadtoempower.com`
- "INVOICE" + invoice number on the right
- From block: company name, address, phone, vendor #, contract #,
  EIN
- To block: school name + address (from
  `engagement.school` lookup)
- Line items table (one row per `invoice_line`):
  - UNIT (always `1` in observed data)
  - QTY (`invoice_line.qty`)
  - DESCRIPTION + date list inline (`"Jan: 29 Feb: 24 Mar: 6, 18 Apr: 17, 24"` — the
    `invoice_line.date_list` field, computed from `coach_hours`
    filtered to that engagement_module in the invoice period)
  - UNIT PRICE
  - TOTAL (subtotal)
- TOTAL row
- "Please send two copies of your invoice." + "Enter this order…"
  footer (verbatim from LTE template)
- PO number stamp (bottom-left, from `purchase_order.po_number`)
- "Authorized by ___ Date ___" signature block

The Canva tool exits the picture entirely. Data flows from
`engagement_module` → `coach_hours` → `invoice_line` → PDF without
manual transcription.

### 8.4 Branding parameterisation

Every template renders `company_profile.name`, `address_*`,
`phone`, `ein`, `nyc_doe_vendor_number`, `default_contract_ref`,
`tagline`, and `logo_path` at the relevant positions. None of
these strings are hardcoded. When the next consulting firm onboards
(Q13), the same templates render their brand by swapping the
profile row.

---

## 9. Auto-CRUD UI

Three new Manage tabs (workspace-scoped):

- **Work Orders** — list + detail. Detail shows the WO header,
  signature state, the engagement's `engagement_module` rows as
  read-only scope items, and a "Generate PDF" button.
- **Purchase Orders** — list + detail. Detail shows PO metadata,
  the linked WO, the engagement's scope items, and an upload-PDF
  field for the school's PDF.
- **Invoice detail** — the existing Invoices tab gets a richer
  detail view: line items become editable, a "Build from Engagement"
  button drafts lines from the engagement's modules + sign-in
  hours, and a "Validate before send" button runs §6's checks
  surfacing issues inline.

**Settings → Company** — single-row edit form for
`company_profile`. Default values seeded at install; Taylor edits
hers on first run.

No tabs for `invoice_line` (lives inside Invoice detail) or
validator results (live as validation messages on Invoice detail).

---

## 10. Open questions

1. **Generated WO PDF approval flow.** Today the WO PDF Taylor
   sends includes only Jacob's signature; she emails it for the
   school to sign and return. Does Travis render the unsigned WO
   for outbound, then accept an uploaded `pdf_path` once it's
   countersigned? Spec assumes yes: `pdf_path` on `work_order` is
   the *latest* version, whether unsigned (vendor-only) or
   countersigned. Validator surfaces "not yet countersigned" if
   `school_signed_at IS NULL` and the engagement has moved to
   `accountable`.

2. **Date-list format on invoice.** The LTE sample uses
   `"Jan: 29 Feb: 24 Mar: 6, 18 Apr: 17, 24"` — month names
   abbreviated, dates comma-separated within month, no year (per
   Jacob's preference per the transcript). Spec uses this format.
   If Taylor wants the year added later, it's a one-line change.

3. **Invoice number generation.** Current shape from the sample is
   `LTE2064981` — appears to be `LTE` + year-suffix + a 4-digit
   sequence. The transcript described it differently: `LTE`+year+
   school#+invoice-seq. Spec defers: keep the existing free-text
   `invoice.number` field; auto-suggest a value on draft using
   whichever pattern Taylor confirms. Travis offers; Taylor
   approves.

4. **Multi-PO-per-engagement.** Spec assumes one active PO per
   engagement at a time. Engagements that renew across school
   years will have multiple POs in their history; the spec treats
   each PO as covering a non-overlapping activity period.

5. **Failure mode of Jacob's memory (Q11 still open).** Spec
   builds the broadest reasonable check: same date double-billed,
   period overlap, period outside PO window. When Taylor names the
   actual failure, narrow the alert message — the SQL stays.

---

## 11. What this is NOT

- **Not a Polaris submission integration.** Polaris is a NYC DOE
  vendor portal; programmatic submission requires DOE API access
  we don't have. Spec adds `submitted_to_polaris_at` as a manual
  checkbox + date so Taylor can mark submissions and Travis can
  surface "unsubmitted invoices" as a follow-up alert if needed.
- **Not Excel export.** Polaris accepts PDF (Q9); the
  Excel-from-Google-Sheets dance the COO walked us through goes
  away because the data is in Travis already. If Polaris later
  hard-requires Excel for the sign-in sheet, we add an export
  command — but until evidence demands it, PDF only.
- **Not a payment tracker.** `invoice.paid_at` already exists; we
  don't model DOE's payment workflow, ACH receipts, or aging
  buckets. That's a separate spec if/when the business wants it.
- **Not a contract table.** `engagement.contract_ref` stays
  free-text. The day a second multi-school solicitation lands and
  cross-references break, we promote it. Not before.
- **Not a new pack.** The next consultancy gets the same pack with
  a different `company_profile` row. Spec deliberately
  parameterizes branding rather than splitting code.

---

## 12. Schema migration safety

The `0003_invoicing.sql` migration touches three pre-existing
tables (`engagement_module`, `invoice`, `coach_hours`) via
`ALTER`. All adds are nullable or have safe defaults — existing
rows in production instances stay intact. The `seed INSERT` for
`company_profile` uses `INSERT OR IGNORE` keyed on `workspace_id`
so re-runs are idempotent and pre-existing rows (if a user has
manually inserted one) aren't clobbered.

Per the convention in `0001_program_delivery.sql`, the pack
migration runs after core's `_sqlx_migrations`, so the invoice
table's existence is guaranteed.

---

*Spec written 2026-05-20 from the COO's recorded walkthrough +
the PS/MS 498 document set (PO `WR260363316`, work order, sign-in
sheet, invoice `LTE2064981`). Implements as `lead_to_empower`
v0.4.0.*
