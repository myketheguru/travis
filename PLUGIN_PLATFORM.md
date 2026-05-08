# Plugin Platform — Phase 1.5 (spec)

A pack should be a complete plugin contract. Today's pack format
(v0.2.0 — see [PACKS.md](./PACKS.md)) covers the *backend* extension
points: data model, tools, action handlers, prompt fragments, entity
vocabulary. The plugin platform extends that contract to **everything
needed to function intelligently in a vertical** — schema metadata
that drives auto-UI, custom UI overrides shipped per-pack,
operational alerts, and pack-specific onboarding requirements.

The principle: **core dynamically materialises UI from pack
declarations; packs may override with custom components when they
have a UX opinion that's worth the work.** Auto-UI is the default;
custom UI is the exception.

---

## What a pack contributes after this phase

```
data shape           ← typed tables + spine sync (have ✓)
tools                ← LLM-callable read-only ops (have ✓)
actions              ← user-confirmed write ops (have ✓)
prompt fragment      ← system-prompt context (have ✓)
entity vocabulary    ← what kinds of things exist (have ✓)

schema metadata      ← per-field labels/types/refs/format (NEW)
auto-UI definitions  ← which tables become tabs / list / detail (NEW)
custom-UI overrides  ← optional React components (NEW)
operational queries  ← layer-2 alerts: bottleneck detection (NEW)
onboarding hooks     ← config the pack needs at install time (NEW)
```

The first five exist. The rest are this phase.

---

## Schema metadata

A pack's `PackHandle` gains a new method:

```rust
fn tables(&self) -> &'static [TableDef] {
    &[]
}
```

`TableDef` describes one of the pack's typed tables in enough detail
that auto-UI can render it without any pack-supplied React code:

```rust
pub struct TableDef {
    /// SQLite table name. Must match an actual table the pack's
    /// migrations created.
    pub slug: &'static str,

    /// Plural display name shown in nav and tab labels: "Tutors".
    pub display_name: &'static str,

    /// Singular display name shown in detail views: "Tutor".
    pub singular_name: &'static str,

    /// The field whose value is the row's human-facing identifier.
    /// Used for refs ("Coach Maria"), spine entity registration,
    /// and detail-page titles. Almost always "name".
    pub display_field: &'static str,

    /// When set, every upsert syncs to `entity` with this kind so
    /// cross-pack retrieval finds the row. Replaces the hand-written
    /// `spine::entity::upsert` calls that today live in each pack
    /// module's CRUD path. Match the value to the pack's
    /// `entity_kinds()` declaration.
    pub entity_kind: Option<&'static str>,

    /// Per-field metadata.
    pub fields: &'static [FieldDef],

    /// Should this table appear as a top-level tab in Manage?
    /// Secondary tables (e.g. join tables, audit logs) set this
    /// false and are reached only through ref links from primary
    /// tables.
    pub primary: bool,

    /// List-view configuration: which columns are visible, default
    /// sort, etc.
    pub list_view: ListViewDef,
}

pub struct FieldDef {
    /// SQL column name.
    pub slug: &'static str,

    /// Form label.
    pub label: &'static str,

    pub field_type: FieldType,

    /// Whether the field is required at create time.
    pub required: bool,

    /// Help text shown under the form input.
    pub help: Option<&'static str>,

    /// Whether to include this field in the list view by default.
    pub default_in_list: bool,
}

pub enum FieldType {
    /// Single-line free text.
    Text,

    /// Multi-line free text → renders as `<textarea>`.
    LongText,

    Email,
    Phone,

    /// 32-bit integer.
    Integer,

    /// Floating-point number.
    Number,

    /// Stored as integer cents; rendered as $X.YY.
    Currency,

    /// ISO 8601 date (YYYY-MM-DD).
    Date,

    /// ISO 8601 timestamp.
    DateTime,

    /// Boolean — checkbox in forms, yes/no in list.
    Bool,

    /// One of a fixed set of values — dropdown.
    Enum(&'static [&'static str]),

    /// Foreign key into another pack table (or core's `entity` for
    /// spine references). Renders as a typeahead picker; list view
    /// shows the referenced row's `display_field`.
    Ref { table: &'static str },

    /// Free-form JSON — only the auto-CRUD's "show me" view shows
    /// this; not editable from auto-form. Pack ships custom UI when
    /// it wants to expose JSON editing.
    Json,

    /// Read-only field populated by the database (e.g. `created_at`).
    Timestamp,
}

pub struct ListViewDef {
    /// Columns to show, by field slug, in order. If empty, defaults
    /// to every field where `default_in_list = true`.
    pub columns: &'static [&'static str],

    /// Default sort column (field slug).
    pub default_sort: Option<&'static str>,

    pub default_sort_dir: SortDir,

    /// Number of rows per page.
    pub page_size: u32,
}

pub enum SortDir {
    Asc,
    Desc,
}
```

Every L2E and tutoring table gets a `TableDef` declaration. The
existing per-table Rust files (`coach.rs`, `tutor.rs`, etc.) stay
*for now* — auto-CRUD generates SQL, but pack-specific logic that
needs to live in code (validation beyond the metadata, computed
fields, special spine wiring) can stay there until it's clear we
don't need it.

---

## Auto-CRUD

Core gains four generic Tauri commands that drive every pack table:

```
pack_table_list(pack_slug, table_slug, filter, sort, limit, offset) → Row[]
pack_table_get(pack_slug, table_slug, id) → Row
pack_table_upsert(pack_slug, table_slug, payload) → Row
pack_table_delete(pack_slug, table_slug, id) → ()
```

Each looks up the pack's `TableDef`, validates the payload against
field metadata, builds SQL using only field slugs from the metadata
(SQL-injection-safe by construction — slugs are `&'static str` from
compile time), executes, and on `upsert` for a table with
`entity_kind` set, syncs to the spine `entity` table with the row's
`display_field` value.

`Row` is a generic `serde_json::Map<String, Value>` — the shape is
implied by the table's metadata, not by a static Rust type. This is
how core can serve every pack's tables without per-table Rust glue.

The hand-written commands the L2E pack ships today
(`list_coaches` / `upsert_coach` / etc.) **continue to work** — they
take payloads of typed structs and exercise pack-specific Rust. The
auto-CRUD is additive. New packs (tutoring) skip writing them
entirely.

### What auto-CRUD doesn't do

- **No validation beyond `required` / `field_type` checks.** Pack
  authors that need richer validation keep a typed `upsert` command.
- **No transactional multi-table writes.** If a pack's create-tutor
  flow needs to also link the tutor to a default tutor-pool, that's
  a typed action.
- **No history / audit beyond what core's `event` table provides.**

Auto-CRUD covers the simple 80%; typed commands cover the rest.

---

## Generic UI components

Frontend ships a small library of components in `src/lib/autoCRUD/`:

```
ListView         — sortable, filterable table; reads the table's
                   list_view config and renders cells via FieldCell
DetailView       — full-row read-only layout with refs as links
FormView         — create/edit form with one input per field
FieldCell        — renders a value by FieldType (currency, date, etc)
FieldInput       — renders an input by FieldType
RefPicker        — typeahead picker for FK fields
```

Plus a hook:

```ts
function usePackTable(packSlug: string, tableSlug: string)
  → { schema, rows, loading, refresh }
```

The Manage tab list becomes data-driven: instead of hard-coded
`{ id: "invoices", label: "Invoices", requiresPack: "lead-to-empower" }`,
it reads `pack_schemas()` and renders one tab per
`primary = true` table from each enabled pack. The corresponding
view is `<ListView packSlug={...} tableSlug={...} />`.

---

## Custom UI overrides

When auto-UI isn't enough, the pack ships a React component and the
manifest says "use this instead":

```rust
// in tutoring/mod.rs:
fn ui_overrides(&self) -> &'static [UIOverride] {
    &[
        UIOverride {
            table: "session",
            view: ViewKind::List,
            component: "SessionCalendar",  // by name
        },
    ]
}
```

The pack ships a TypeScript file at:

```
src/packs/<slug>/ui/SessionCalendar.tsx
src/packs/<slug>/ui/index.ts          (component registry)
```

`src/packs/<slug>/ui/index.ts` re-exports every component the pack
provides, keyed by name:

```ts
export { default as SessionCalendar } from "./SessionCalendar";
```

A central frontend pack registry (`src/lib/packRegistry.ts`)
imports each pack's `ui/index.ts` conditionally based on a build-
time feature flag (mirroring the Cargo features) and exposes
`getOverride(packSlug, table, view) → Component | null`.

**At render time:**

1. Frontend looks up override for `(pack, table, view)`.
2. If override exists, render it.
3. Otherwise, render the auto-CRUD component for that view.

The L2E pack's existing `InvoicesTab.tsx` becomes
`src/packs/lead_to_empower/ui/InvoicesTab.tsx` (file move + import-
path update + export), declared as the override for
`(invoice, list)`. No more hand-written tabs in core.

### Bundling

For v0.2.x, pack UI is **compile-time bundled**. Each pack's
TypeScript lives under `src/packs/<slug>/` in the source tree;
TypeScript's path resolution + Vite's tree-shaking handles inclusion.
A build-time feature flag (probably set via Vite env var matched to
the Cargo features) controls which packs' UIs are bundled. Disabled
packs' code is dead-stripped.

Runtime-loaded TypeScript (drag-and-drop pack install) needs a JS
sandbox + bundler at runtime. Phase 3 territory.

---

## Operational queries

Each pack declares the layer-2 alerts that surface bottleneck
states — *the metric to capitalise on* per `MARKET.md`'s "what makes
each vertical sticky" framing:

```rust
fn alerts(&self) -> &'static [AlertDef] {
    &[
        AlertDef {
            slug: "uninvoiced_hours",
            label: "Uninvoiced billable hours",
            severity: AlertSeverity::Money,
            // SQL returns one row with {count, sample_label, sample_id}
            // count is the headline number; sample_* link to a representative
            // affected row for "show me" navigation.
            sql: "SELECT
                    COUNT(*) AS count,
                    MAX(c.name) AS sample_label,
                    MAX(c.id) AS sample_id
                  FROM coach_hours h
                  JOIN coach c ON c.id = h.coach_id
                  WHERE NOT EXISTS (
                    SELECT 1 FROM invoice i
                    WHERE i.coach_id = h.coach_id
                      AND h.session_date BETWEEN i.period_start AND i.period_end
                  )",
        },
    ]
}
```

A new core Tauri command `pack_alerts() → Vec<AlertResult>` runs
every enabled pack's alert SQL and returns the results. The Splash
screen renders these as the "headline number that matters" — for
L2E, it's "$X in uninvoiced hours" rather than "12 invoices · 5
coaches."

This is also what feeds Travis-the-LLM via the proactive nudge
loop: when an alert's count is non-zero and rising, the proactive
prompt mentions it. ("Maria's hours have been pending invoicing for
3 weeks — want me to draft one?")

---

## Onboarding hooks

Some packs need user input at install time (e.g., L2E needs the
default invoice prefix; HVAC needs the default labour rate).
PackHandle gains:

```rust
fn onboarding_fields(&self) -> &'static [FieldDef] {
    &[]
}
```

If non-empty, the onboarding flow inserts a "Configure {pack name}"
step after the pack picker, with one field per declaration. Values
are stored in `meta.pack.<slug>.config.<field>`. Pack code reads via
`crate::packs::config::get(slug, field) → Option<String>`.

Settings → Packs gains an "Edit configuration" button per pack with
non-empty `onboarding_fields()`.

---

## Migration path from current state

1. **L2E InvoicesTab** moves from `src/manage/tabs/InvoicesTab.tsx`
   to `src/packs/lead_to_empower/ui/InvoicesTab.tsx`. Declared as
   `UIOverride { table: "invoice", view: List }`. The Manage tab
   list stops hard-coding it.
2. **Hand-written L2E tabs** for coach, school, etc. don't exist
   today (just InvoicesTab). Auto-CRUD covers them out of the box
   once the platform lands.
3. **Tutoring pack** gets full UI for free — just declare schema
   metadata; auto-CRUD does the rest. No `domain_cmd.rs` required
   unless the pack needs typed actions later.
4. **Existing typed commands** (`list_coaches`, `upsert_coach`, etc.)
   stay for now. They can deprecate gradually as auto-CRUD proves
   itself; or they stay forever if they exercise per-table Rust
   logic that's hard to put in metadata.

---

## Implementation slices

In dependency order; each slice is independently useful and
shippable.

| Slice | Description | Estimate |
|---|---|---|
| 1 | Schema metadata Rust types + `tables()` on PackHandle. L2E + tutoring declare tables. | 4h |
| 2 | `pack_schemas()` Tauri command → frontend types | 1h |
| 3 | Auto-CRUD Tauri commands: `pack_table_list` / `_get` / `_upsert` / `_delete` | 6h |
| 4 | Frontend auto-CRUD: ListView component + usePackTable hook + Manage tab integration | 6h |
| 5 | DetailView + FormView + FieldInput components | 4h |
| 6 | Custom UI override mechanism (move InvoicesTab into pack) | 3h |
| 7 | Operational queries: `alerts()` + `pack_alerts()` + Splash integration | 3h |
| 8 | Onboarding hooks: `onboarding_fields()` + onboarding step + config storage | 2h |

Total: ~30h focused. **This session: slices 1–4** (the foundation —
schema-driven auto list views, replacing the bones of how every
pack's UI gets rendered). 5–8 are follow-ups.

---

## Why this is the right shape

**For pack authors:** writing a new vertical pack drops to:
declare schema metadata → ship migrations → done. No UI code, no
Tauri commands. The pack file structure is small enough to hand to
a non-Rust contributor.

**For Travis-the-LLM:** schema metadata is also LLM context.
Tells the model what fields exist, what they mean, how they're
labelled. Improves extraction quality for free.

**For the user:** every pack's UI looks consistent (auto-CRUD has
one design). When a pack ships custom UI, it's because the UX
genuinely needs it — not because someone had to write CRUD by hand.

**For the Jarvis arc:** packs become a real plugin format —
declarative, self-describing, dynamically loadable (eventually).
This is what turns "Travis is a pack-aware app" into "Travis is the
Linux kernel of operational AI; packs are its standard library."
