# Pack Refactor — Codebase Audit

Working doc for Phase 1 of the [ROADMAP](./ROADMAP.md). Maps every
L2E-specific seam in the current Travis binary so the move into a
pack is mechanical, not a redesign. For the resulting pack format see
[PACKS.md](./PACKS.md).

This doc is **temporary**. Once the L2E pack and the second pack
(tutoring) ship, archive this doc — its job is done.

Audited at: Travis v0.1.3 (commit `1fc6d08`), 2026-05-07.

---

## Two questions this doc answers

1. **What's L2E?** — every file/symbol that names coaches, schools,
   signing sheets, invoices, NYC DoF, or otherwise assumes the
   after-school-enrichment domain.
2. **What core hooks do packs need?** — every place a pack must
   register or contribute, where today the code hard-codes a single
   shape.

---

## Lift wholesale (move into the L2E pack)

These files are 100% L2E. Cut and paste.

| File | Notes |
|---|---|
| `src-tauri/migrations/0003_domain.sql` | The L2E schema **except** the `task` table. Coach, school, coach_hours, signing_sheet, invoice move to the pack. The `task` table moves into a NEW core migration with generalized columns (see "core spine work" below). |
| `src-tauri/src/domain/mod.rs` | Re-exports + `stats()` over L2E tables |
| `src-tauri/src/domain/coach.rs` | Coach CRUD |
| `src-tauri/src/domain/school.rs` | School CRUD |
| `src-tauri/src/domain/coach_hours.rs` | Hours-logging CRUD + `sum_in_period` |
| `src-tauri/src/domain/signing_sheet.rs` | Signing-sheet CRUD |
| `src-tauri/src/domain/invoice.rs` | Invoice CRUD + transitions |
| `src-tauri/src/domain_cmd.rs` | All Tauri commands for the L2E surface |
| `src-tauri/src/pdf/mod.rs` | The NYC DoF invoice PDF generator |
| `src-tauri/src/pdf_cmd.rs` | `export_invoice_pdf` Tauri commands |
| `src/lib/domain.ts` | TS bindings to the above commands |
| `src/manage/tabs/InvoicesTab.tsx` | The Invoices UI |

**`task`/`domain/task.rs` graduates to core as a thin generic
opt-in.** Tasks fit ~75% of MARKET.md's verticals (Tiers A, B, D)
where the work-item is "title + status + due + entity link." For
verticals where the primary work-item is a different shape (HVAC
`job` with dispatch + geo, therapy `case` as a long-running
relationship), the pack ships its own typed table and ignores
`task`. The current `link_kind` CHECK
(`'coach','school','coach_hours','signing_sheet','invoice'`) drops;
`link_kind`/`link_id` become `entity_id` (soft FK to `entity`,
no constraint). (PACKS.md decision #5.)

## Core spine work (new, not L2E-specific)

Three core tables make Travis truly shape-flexible. None of these
move with the L2E pack — they're new core infrastructure that lands
alongside the pack refactor.

| Table | Migration | What it holds |
|---|---|---|
| `entity` | Generalize existing `entity_index` (drop kind allowlist; add `pack_slug`, `attrs_json`) | Generic rendezvous for every domain object — id, kind, name, normalized_name, pack_slug, attrs_json |
| `relation` | New | Typed edges — from_entity, to_entity, type, attrs_json |
| `event` | New | Anything that happened — entity_id, kind, occurred_at, attrs_json |

Pack code becomes responsible for writing to `entity` / `relation` /
`event` whenever its own typed-table state changes. Explicit writes,
not triggers — easier to reason about. The L2E pack's coach/school/
sheet/invoice CRUD picks up ~50 LOC of "also write to entity / event"
plumbing.

## Lift partially (extract L2E from generic file)

These files mix core and L2E. Each needs surgery.

### `src-tauri/src/actions.rs`

Action dispatcher hard-codes every action kind. The L2E-specific bits:

- **`apply_propose_invoice_draft`** (lines 248–329) — knows about
  `coach`, `school`, `invoice`, calls `coach_hours::sum_in_period`.
  Hardcoded `"NYC Department of Finance"` recipient (line 290) and
  `"L2E-{year}-"` invoice-number prefix (line 218).
- **`resolve_or_create_coach`** (line 166), **`resolve_or_create_school`**
  (line 190), **`next_invoice_number`** (line 214) — helpers used only
  by `apply_propose_invoice_draft`.
- **`supported_kinds()`** (line 753) — static array including
  `"propose_invoice_draft"`.
- **`dispatch`** match arm (line 765) — routes
  `"propose_invoice_draft"` to `apply_propose_invoice_draft`.

**Refactor:** Replace `dispatch`'s static match with a runtime
`ActionRegistry`. Pack-supplied action handlers register at startup
(see PACKS.md). The L2E-specific code (above) moves into the pack
and registers itself. Built-in actions (`defer_task`, `set_reminder`,
etc.) stay in core and pre-register.

### `src-tauri/src/journal.rs`

The journal extraction prompt (line ~36 onwards) and its JSON schema
(line ~304) hard-code `entities.{coaches, schools, depts}` and the
`proposedActions` enum (line 350) lists `propose_invoice_draft`.

Extraction follows-up (`extraction.entities.coaches`, line 784) feed
`identity::record_mention("coach", ...)` etc.

**Refactor (v0.2 minimum):**
- Rename buckets to **`people / orgs / agencies`** (already
  half-genericized with the comment "Apply them sensibly to the
  user's domain"). Pack documentation explains that `people` ≈ coach,
  `orgs` ≈ school, `agencies` ≈ dept for L2E.
- The `proposedActions.kind` enum becomes dynamically built from
  registered action kinds (gather from `ActionRegistry`).
- `identity::record_mention` calls pass through to whatever entity
  kinds the installed pack(s) declare (decision below).

`tools::read_only_registry()` is called at line 568. That call should
become `pack_registry::read_only_registry_for(&installed_packs)`.

### `src-tauri/src/identity/mod.rs`

- `record_mention` (line ~47) rejects kinds not in
  `["coach", "school", "dept"]`. **Drop the allowlist** — packs
  declare kinds, and the journal extraction is the only ingress; if a
  bad kind sneaks in we log a warning and discard, not error.
  Alternative: check against the union of `entity_kinds` from
  installed packs.
- `top_names` queries (line ~111) fetch top "coach", "school", "dept".
  This function is used to build the user-context block for the LLM.
  **Refactor:** parameterize over kind, called once per
  pack-declared kind. Or generalize to "top entities of any kind"
  with kind grouping.

### `src-tauri/src/summary/mod.rs`

`DayContext.coach_hours` field (line 61) and the JOIN against `coach`
(line 116). Both daily and weekly summary collection touch L2E.

**Refactor:** `DayContext` keeps generic fields (journal entries,
completed tasks). Each installed pack contributes a
`SummaryContribution` callback that adds its own context block to the
day's render. The L2E pack's contribution is "Coach hours logged: …".

For v0.2 this can be cut: the pack contributes nothing to summaries
in the first iteration; the daily/weekly summary just renders journal
entries + completed tasks. Adding pack-supplied summary contributions
is a small follow-up.

### `src-tauri/src/email_cmd.rs`

Has `send_invoice_email` (L2E). Move to the pack as a tool. Generic
`send_email_gmail` and `send_email_outlook` stay in core.

### `src-tauri/src/memory_cmd.rs::ask_travis`

Around line 230, the system prompt names operational examples:
`"set a reminder, draft an invoice, send an email"`. The "draft an
invoice" example is L2E-flavored.

**Refactor:** the example list is built from registered actions (each
action has a short example phrase). For v0.2: change "draft an
invoice" to a more neutral "create a draft" and call it good.

### `src-tauri/src/tools/list_open_tasks.rs`

References tasks (generic) but probably formats `link_kind` /
`link_id` references in human-readable form. If it stringifies
`coach`/`school` IDs into "Coach Maria" / "School PS 142", that
formatter belongs in the pack. Quick check needed when refactoring.

## New extension points needed in core

These are the **only** new mechanisms the pack format requires. Each
is a small refactor of an existing seam.

| Hook | Where today | Refactor |
|---|---|---|
| Pack registry | (none) | New module `src-tauri/src/packs/mod.rs`. Holds the list of compiled-in packs and their `register_*` callbacks. |
| Pack migrations | `db.rs:81` (`sqlx::migrate!`) | After core migrations, run each enabled pack's migrations from its directory, tracking applied versions in `meta.pack.<slug>.schema_version`. ~50 LOC. |
| Core spine: `entity` | `0009_identity.sql` (existing `entity_index`) | New core migration generalizes: drops kind allowlist, adds `pack_slug` + `attrs_json`. Backfill from existing rows. |
| Core spine: `relation`, `event` | (none) | New core migration. Two tables, ~30 LOC SQL. |
| Spine sync from packs | (none) | Pack code calls `entity::upsert(...)`, `event::record(...)`, `relation::link(...)` from its CRUD paths. Helper functions live in core; calls live in pack code. ~50 LOC per pack. |
| Tool registry | `tools/mod.rs::read_only_registry` (hard-coded list) | Function takes a `&[PackHandle]`, walks each and asks for tools. |
| Action registry | `actions.rs::dispatch` (match) | New `ActionRegistry` with `HashMap<String, ActionHandler>`. Built-in actions register at startup; packs add their own. |
| Entity-kind allowlist | `identity::record_mention` (line ~47) | Drop allowlist OR check against `pack.entity_kinds` union. |
| System prompt assembly | `memory_cmd::ask_travis`, `summary/mod.rs`, `proactive::build_system_prompt` (each builds prompts independently) | Each call site calls `pack_registry::system_prompt_fragments()` and concatenates per the manifest's `system_prompt.mode`. Small change to each call site. |
| UI tab gating (frontend) | `src/manage/Manage.tsx` (hard-coded tab list) | Tab list filters by "is the pack that owns this tab installed?" — for v0.2, just check a runtime flag from `app_status`. Phase 2 makes this a real plugin system. |

## Decisions to make (sanity-check items from PACKS.md)

Recap, with my recommendation in **bold**:

1. **Cargo feature flag for v0.2 pack enablement.** ✓ Cheap.
2. **Per-pack migration counter in `meta`.** ✓ Independence > sqlx
   integration.
3. **Action registry as runtime `HashMap`.** ✓ Small refactor.
4. **Entity buckets renamed to people/orgs/agencies; pack-declared
   kinds graduate to Phase 2.** ✓ Pragmatic.
5. **Universal spine + opt-in `task`.** ✓ Spine (entity/relation/
   event) handles cross-pack retrieval; `task` is core but
   opt-in; packs ship their own work-item table when needed.
6. **L2E UI tabs stay in binary, gated on pack-installed.** ✓ Defer
   real pack UI to Phase 2.

Confirm or change before any code moves.

## Out of scope for v0.2

These are real concerns but not for the first pack:

- **Pack signing / verification.** Native packs are written by you;
  trust is implicit. Marketplace signing matters once user-installed
  packs land (Phase 2).
- **Pack permissions / sandboxing.** Native packs have full DB
  access. Granular permissions matter once remote-tool packs land
  (Phase 3).
- **Pack uninstall (destructive).** Disabling a pack via Cargo
  feature flag stops it from running but leaves migrations applied
  and data intact. Real `pack uninstall` (with optional
  destructive cleanup) is a UI feature for Phase 2.
- **Cross-pack data sharing.** A future therapy pack and a future
  scheduling pack might want to share entities. Defer until two
  installed packs actually overlap in real customer use.
- **Per-pack config UI.** When the L2E pack's invoice number prefix
  ("L2E-") needs to be configurable per-customer, that's a pack
  config feature. Not for v0.2 — hardcode and revisit when the
  second L2E customer wants different numbering.

## Minimum diff to make Travis pack-shaped (the actual work)

In execution order:

1. **Add the pack-registry skeleton** (~150 LOC). New
   `src-tauri/src/packs/mod.rs` with `PackHandle` trait,
   `enabled_packs()` returning a `Vec<&dyn PackHandle>`. No packs
   registered yet.
2. **Add the core spine: `entity` (generalized), `relation`,
   `event` tables** (~80 LOC SQL + ~120 LOC Rust helpers
   `entity::upsert`, `event::record`, `relation::link`). New core
   migration. Backfill `entity` from existing `entity_index`.
3. **Refactor `actions.rs::dispatch` into a registry** (~100 LOC
   net change). Built-in handlers pre-register; the static match
   becomes a HashMap lookup.
4. **Refactor `tools::read_only_registry` to take a pack list**
   (~20 LOC).
5. **Add per-pack migration runner** (~50 LOC) with `meta`-tracked
   versions.
6. **Move `task` to core**, drop the L2E-specific
   `link_kind` CHECK, replace `link_kind`/`link_id` with
   `entity_id` (soft FK into `entity`). Migration that copies
   existing task data. (~40 LOC SQL.)
7. **Drop the entity-kind allowlist in `identity::record_mention`**;
   accept any kind a registered pack declares. (~20 LOC.)
8. **Move `domain/{coach,school,coach_hours,signing_sheet,invoice}`,
   `pdf/`, `domain_cmd.rs`, `pdf_cmd.rs`, the L2E pieces of
   `0003_domain.sql`, and the L2E pieces of `actions.rs`** into
   `src-tauri/packs/lead-to-empower/`. Wire as a pack. Cargo
   feature `pack-lead-to-empower` gates compilation. **Add spine
   sync calls to all CRUD paths** — every coach insert writes a
   matching `entity` row, every coach_hours insert writes an
   `event` row, etc. (Bulk of the work — 2–3 days of mechanical
   refactoring.)
9. **Generalize the journal extraction prompt** — rename buckets
   `coaches/schools/depts` to `people/orgs/agencies`, make
   `proposedActions.kind` enum dynamic from registered actions.
   (~50 LOC.)
10. **Generalize the system-prompt assembly call sites** — each
    site asks the pack registry for fragments + the primary
    work-item label. (~30 LOC across 3-4 files.)
11. **Frontend: gate `InvoicesTab` on pack-installed flag.** Add the
    flag to `app_status`. (~20 LOC.)
12. **Verify no regressions:** Travis still works exactly as today
    when L2E pack is enabled. Travis still starts and is usable
    when L2E pack is disabled (no Invoices tab, no coach
    extraction; just journal + tasks + reminders + summaries on top
    of the user's profile blurb). Spine populated correctly:
    `entity`, `relation`, `event` rows match what the L2E typed
    tables hold.

That's the v0.2 ship. Estimate: **~2.5 weeks of focused work** (the
spine adds ~3 days vs the original plan; matches the ROADMAP Phase
1 weeks 2–3 with a small slip into week 4). Drag-and-drop install
flow follows. Week 5–6 build the tutoring pack from scratch as the
abstraction validation — the tutoring pack will write to `entity` /
`event` for tutors, students, sessions, and rely on core's `task`
for "follow up with parent" style items, validating that both the
spine and the opt-in work-item story actually feel right.

## Things to verify when you've reviewed this

- [ ] Are the six decisions in PACKS.md the right calls?
- [ ] Is the "task graduates to core" call right? (Or should tasks be
  per-pack — e.g., a therapy "case" is a different shape from a
  cleaning "job"?)
- [ ] Should the entity-kind allowlist be dropped entirely, or kept
  with the union of pack-declared kinds? (Decided: dropped.)
- [ ] Is bundling the L2E pack via Cargo feature flag (rather than
  drag-and-drop install) acceptable for v0.2? (Decided: yes —
  drag-and-drop is week 4.)
