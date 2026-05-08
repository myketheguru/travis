# Authoring a Pack

A **pack** is the unit of vertical extension in Travis. It bundles
schema, prompts, tools, actions, and (optionally) custom UI for one
operational shape — after-school programs, tutoring agencies, home
care, therapy practices, HVAC contractors, anything that fits the
"contractors-at-sites + billable hours + signed proof + payer
invoiced" pattern or its variants.

This document is the developer guide for **building a new pack** and
**updating an existing one**. It assumes you've read
[`PACKS.md`](./PACKS.md) (the format spec) and
[`PLUGIN_PLATFORM.md`](./PLUGIN_PLATFORM.md) (the auto-UI / plugin
contract).

The current shipping packs — `lead_to_empower` and `tutoring` — are
the canonical references. Read their source alongside this doc; the
patterns below all have working examples there.

---

## Mental model

Travis core is the kernel. It owns: identity, profile, conversations,
tasks, reminders, calendar, email, journal extraction, the LLM loop,
the tool dispatch surface, the action confirmation surface, the
spine (`entity` / `relation` / `event` tables), and the auto-CRUD
machinery.

A pack contributes:

| Surface | What it adds | Required? |
|---|---|---|
| **Migrations** | SQLite tables for the pack's typed records | Yes (if it has any data) |
| **Schema metadata** | Per-field type info that drives auto-UI | Yes |
| **Prompt fragment** | Vertical-specific guidance the LLM reads | Recommended |
| **Entity kinds** | What kinds of named things exist (`coach`, `student`, …) | Recommended |
| **Spine sync** | Mirrors typed rows into `entity` / `event` | Recommended |
| **Operational alerts** | The "metric to capitalise on" — money/work-at-risk | Recommended |
| **Action handlers** | User-confirmable writes the LLM can propose | Optional |
| **LLM tools** | Read-only ops the LLM can call autonomously | Optional |
| **Custom UI overrides** | React components that replace auto-CRUD | Optional |
| **Onboarding fields** | Config the pack needs at install time | Optional (spec'd, deferred) |

Build the **required** ones and you have a working pack. Add the
**recommended** ones to make Travis genuinely useful in the vertical.
Reach for the **optional** ones only when the auto-rendered surface
isn't enough.

---

## Picking your vertical

Before writing code, validate the fit. From [`MARKET.md`](./MARKET.md),
Travis serves verticals that pass all four:

1. **Structured operations** — the work has steps that repeat.
2. **Non-technical operator** — the user isn't a developer; UX matters.
3. **Painful or expensive existing tools** — there's something to displace.
4. **Clear ROI per seat** — the vertical pays for software.

If your vertical only passes 3 of 4, the pack will work but won't
sell. If it passes all 4, the rest is mechanical.

---

## Anatomy of a pack

Every pack lives at `src-tauri/src/packs/<slug>/`. The slug is
lowercase, hyphenated, globally unique. Use the directory layout:

```
src-tauri/src/packs/<slug>/
├── mod.rs                  required — PackHandle impl
├── tables.rs               required — TableDef metadata
├── domain/                 typed CRUD modules per table
│   ├── mod.rs
│   ├── <entity>.rs         (one file per typed table)
│   └── …
├── migrations/             pack's own SQL migrations
│   └── 0001_init.sql
├── actions.rs              optional — action handlers
├── tools/                  optional — LLM tools
│   ├── mod.rs
│   └── <tool>.rs
├── pdf/                    optional — PDF generation
└── ui/                     optional — custom React components
    ├── index.ts
    └── <Component>.tsx
```

The **frontend** half of the pack (when there's custom UI) lives at
`src/packs/<slug>/ui/` to mirror the Rust side. Both halves are
gated by the same Cargo feature flag, so a build with the feature
disabled has neither.

---

## Step 1 — Cargo feature flag

Open `src-tauri/Cargo.toml` and add a feature for your pack:

```toml
[features]
default = ["pack-lead-to-empower", "pack-tutoring", "pack-<your-slug>"]
pack-lead-to-empower = []
pack-tutoring        = []
pack-<your-slug>     = []
```

Default features include every shipped pack so users can pick at
runtime. To build a binary with only your pack:

```sh
cargo build --no-default-features --features pack-<your-slug>
```

---

## Step 2 — Module skeleton

Create `src-tauri/src/packs/<slug>/mod.rs`:

```rust
//! <Pack name> — <one-line vertical description>.
//!
//! Vertical: <2–3 sentences on the operational shape>. Tier <X>
//! vertical #<N> in MARKET.md. WTP $<X>–$<Y>/mo.

mod tables;

use crate::packs::{AlertDef, PackHandle, PackMigration, TableDef};

const SLUG: &str = "<your-slug>";

pub struct <YourPack>Pack;

impl PackHandle for <YourPack>Pack {
    fn slug(&self) -> &'static str {
        SLUG
    }

    fn name(&self) -> &'static str {
        "<Display Name>"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn description(&self) -> &'static str {
        "<One-paragraph user-facing description shown in onboarding \
         and Settings → Packs.>"
    }

    fn default_enabled(&self) -> bool {
        // New packs default OFF — users opt in via onboarding or
        // Settings → Packs. Returning true is for legacy compat
        // with packs that shipped before runtime selection landed.
        false
    }

    fn migrations(&self) -> &'static [PackMigration] {
        MIGRATIONS
    }

    fn prompt_fragment(&self) -> Option<&'static str> {
        Some(PROMPT_FRAGMENT)
    }

    fn entity_kinds(&self) -> &'static [&'static str] {
        &[/* "<kind1>", "<kind2>", ... */]
    }

    fn tables(&self) -> &'static [TableDef] {
        tables::TABLES
    }

    fn alerts(&self) -> &'static [AlertDef] {
        ALERTS
    }
}

// Migrations bundled at compile time via include_str!.
const INIT_SQL: &str = include_str!("migrations/0001_init.sql");

static MIGRATIONS: &[PackMigration] = &[PackMigration {
    name: "0001_init",
    sql: INIT_SQL,
}];

const PROMPT_FRAGMENT: &str = "\
You also help with <vertical> ops:\n\
- <Bullet about a domain object the user tracks>.\n\
- <Bullet about a workflow that needs the user's attention>.\n\
- <Bullet about a money-flow / value-flow specific to this vertical>.\n\
\n\
When the user mentions a <thing> by name, prefer recording the\n\
mention even if no specific action is requested.\
";

static ALERTS: &[AlertDef] = &[/* see step 7 */];
```

Then register the pack in `src-tauri/src/packs/mod.rs`:

```rust
#[cfg(feature = "pack-<your-slug>")]
pub mod <your_slug_underscored>;

pub fn compiled_in_packs() -> &'static [&'static dyn PackHandle] {
    &[
        #[cfg(feature = "pack-lead-to-empower")]
        &lead_to_empower::LeadToEmpowerPack,
        #[cfg(feature = "pack-tutoring")]
        &tutoring::TutoringPack,
        #[cfg(feature = "pack-<your-slug>")]
        &<your_slug_underscored>::<YourPack>Pack,
    ]
}
```

> **Naming.** The Cargo feature uses hyphens (`pack-tutoring-agency`).
> The Rust module path uses underscores (`tutoring_agency`). Both
> map to the same canonical slug (`tutoring-agency`).

---

## Step 3 — Initial migration

Write `src-tauri/src/packs/<slug>/migrations/0001_init.sql`. This is
the schema for every typed table the pack owns. Example shape:

```sql
-- <Pack name> pack — initial schema.
--
-- Tracked in `meta.pack.<slug>.schema_version`, independent of core's
-- `_sqlx_migrations`. Per-pack migrations run for every compiled-in
-- pack regardless of runtime-enabled state — so toggling a pack on
-- later doesn't trigger a migration that could fail.

CREATE TABLE IF NOT EXISTS <table_name> (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    name        TEXT NOT NULL,
    -- ... other fields ...
    created_at  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_<table_name>_name ON <table_name>(name);
```

**Conventions every pack follows:**

- Every table has `id INTEGER PRIMARY KEY AUTOINCREMENT`.
- Every table has `created_at` and `updated_at` (TEXT, default `CURRENT_TIMESTAMP`).
- Foreign keys use `REFERENCES <other_table>(id)` with `ON DELETE CASCADE` for "child" rows or `ON DELETE RESTRICT` to prevent orphaning.
- Use `IF NOT EXISTS` so re-running the migration on a partially-applied DB is safe.
- Pin migration files to LF (`.gitattributes` does this for `**/*.sql`).

**Adding a new migration later:** create `0002_<change>.sql` next to
the first one, append a `PackMigration` entry to the `MIGRATIONS`
static slice in `mod.rs`, increment the pack's `version()`. The
runner picks up new migrations automatically on next launch.

> **Never edit a migration that's already shipped.** The runner
> tracks which migration number is highest-applied per pack; editing
> an applied file silently breaks no one (no checksum yet on pack
> migrations) but wreaks havoc when the schema changes diverge from
> what users have. Always add a new file.

---

## Step 4 — Schema metadata (auto-CRUD wiring)

Create `src-tauri/src/packs/<slug>/tables.rs`. This file is the
contract that tells Travis core's auto-CRUD how to render every
list, detail, and form for your pack. **Get this right and you don't
write any TypeScript** — the entire UI materialises from the metadata.

```rust
//! Schema metadata for the <Pack> pack's typed tables. Drives the
//! auto-CRUD UI and generic Tauri commands (PLUGIN_PLATFORM.md).

use crate::packs::{FieldDef, FieldType, ListViewDef, SortDir, TableDef};

static <ENTITY>_FIELDS: &[FieldDef] = &[
    FieldDef {
        slug: "id",
        label: "ID",
        field_type: FieldType::Integer,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "name",
        label: "Name",
        field_type: FieldType::Text,
        required: true,
        help: None,
        default_in_list: true,
    },
    FieldDef {
        slug: "rate_cents",
        label: "Hourly Rate",
        field_type: FieldType::Currency,
        required: false,
        help: Some("What this person charges per hour."),
        default_in_list: true,
    },
    // Always include created_at/updated_at as Timestamp — they're
    // skipped from forms but visible in detail views.
    FieldDef {
        slug: "created_at",
        label: "Created",
        field_type: FieldType::Timestamp,
        required: false,
        help: None,
        default_in_list: false,
    },
    FieldDef {
        slug: "updated_at",
        label: "Updated",
        field_type: FieldType::Timestamp,
        required: false,
        help: None,
        default_in_list: false,
    },
];

static <ENTITY>: TableDef = TableDef {
    slug: "<table_name>",
    display_name: "<Plural>",
    singular_name: "<Singular>",
    display_field: "name",
    entity_kind: Some("<kind>"),
    fields: <ENTITY>_FIELDS,
    primary: true,
    list_view: ListViewDef {
        columns: &["name", "rate_cents"],
        default_sort: Some("name"),
        default_sort_dir: SortDir::Asc,
        page_size: 50,
    },
};

pub static TABLES: &[TableDef] = &[<ENTITY>, /* ... */];
```

### `FieldType` reference

| Variant | SQL type | UI input | Display |
|---|---|---|---|
| `Text` | TEXT | `<input type="text">` | Plain |
| `LongText` | TEXT | `<textarea>` | Truncated in list, full in detail |
| `Email` | TEXT | `<input type="email">` | Plain |
| `Phone` | TEXT | `<input type="tel">` | Plain |
| `Integer` | INTEGER | Number input | Monospace |
| `Number` | REAL | Number with decimals | Monospace |
| `Currency` | INTEGER (cents) | Dollar input ($X.YY ↔ cents) | `$X.YY` |
| `Date` | TEXT (YYYY-MM-DD) | `<input type="date">` | Date string |
| `DateTime` | TEXT (ISO 8601) | `<input type="datetime-local">` | Trimmed |
| `Bool` | INTEGER (0/1) | Checkbox | Yes/No |
| `Enum { options }` | TEXT | `<select>` | Chip with value |
| `Ref { table }` | INTEGER | Numeric id input | `<table>#<id>` |
| `Json` | TEXT | `<textarea>` (raw) | Truncated mono |
| `Timestamp` | TEXT (ISO 8601) | Read-only display | Trimmed |

### `TableDef` fields

- **`slug`** — SQLite table name. Must match the migration.
- **`display_name`** — Plural label shown in tabs ("Tutors").
- **`singular_name`** — Singular label shown in detail/form titles ("Tutor").
- **`display_field`** — The field whose value identifies the row to humans. Used for refs, spine entity registration, and detail-page titles. Almost always `"name"`.
- **`entity_kind`** — When `Some("kind")`, every auto-CRUD upsert syncs to the spine `entity` table with this kind. Match the value to one of your pack's `entity_kinds()`.
- **`fields`** — Per-field metadata; order in this list is the form-field order.
- **`primary`** — `true` makes the table a top-level Manage tab; `false` hides it (it's reachable only via refs from primary tables).
- **`list_view`** — Default sort, default columns, page size.

### `ListViewDef`

- **`columns`** — Slugs in display order. Empty = use every field where `default_in_list = true`.
- **`default_sort`** — Slug to sort by initially. Often `"name"` for primary tables, `"created_at"` for transactional ones.
- **`default_sort_dir`** — `SortDir::Asc` or `SortDir::Desc`. Use `Desc` for things where "newest first" is right (sessions, invoices).
- **`page_size`** — Rows per page. 50 for primary tables, 100 for high-volume transactional ones.

### What auto-CRUD does for you

Once `TableDef::fields` is populated, the frontend automatically gives
you:

- A sortable list view at `Manage → <display_name>`.
- Click-to-detail navigation.
- Edit / Delete from detail.
- "+ New <singular>" button on the list view.
- Form with one input per field, type-appropriate, with required-field
  validation.
- Spine sync on save (when `entity_kind` is set) — the row's
  `display_field` value is registered as an entity for cross-pack
  retrieval.

You write zero TypeScript for any of this.

---

## Step 5 — Typed CRUD modules (optional)

If your pack only ever uses auto-CRUD, you don't need typed Rust
domain modules. The auto-CRUD Tauri commands (`pack_table_list`,
`pack_table_get`, `pack_table_upsert`, `pack_table_delete`) handle
generic SQL using the metadata.

You'll still want typed domain modules when you have:
- **Pack-specific business logic** beyond field-level validation
  (e.g., L2E invoice's "validate signing sheet covers period"
  pre-condition before status transitions to `sent`).
- **Computed fields** that aren't simple SQL columns
  (e.g., `coach_hours::sum_in_period`).
- **Action handlers** that build domain state programmatically
  (e.g., L2E's `propose_invoice_draft` constructs an `InvoiceInput`
  from journal-extracted parameters).

When you do need typed code, mirror the L2E pattern:

```
src-tauri/src/packs/<slug>/domain/
├── mod.rs
├── <entity>.rs       (one file per table — Coach, School, Invoice…)
└── …
```

Each entity file has:

```rust
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use super::DomainError;

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct <Entity> { /* … */ }

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct <Entity>Input { /* … */ }

pub async fn list(pool: &SqlitePool) -> Result<Vec<<Entity>>, DomainError> { … }
pub async fn upsert(pool: &SqlitePool, input: <Entity>Input) -> Result<<Entity>, DomainError> {
    // … SQL upsert …
    // Spine sync — register as entity for cross-pack retrieval:
    if let Err(e) = crate::spine::entity::upsert(
        pool,
        crate::spine::entity::UpsertParams {
            kind: "<entity-kind>",
            display_name: &row.name,
            pack_slug: Some("<your-slug>"),
            attributes_json: None,
        },
    ).await { tracing::warn!(...); }
    Ok(row)
}
pub async fn delete(pool: &SqlitePool, id: i64) -> Result<(), DomainError> { … }
```

The `domain/mod.rs` re-exports the submodules and pulls in
`DomainError`:

```rust
pub mod <entity>;
// … one per table

pub use crate::domain::DomainError;
```

> **Auto-CRUD already does spine sync** when `entity_kind` is set on
> the `TableDef`. If you write a typed `upsert` AND let auto-CRUD
> through to the same table, both will fire spine sync. Pick one
> path: either use auto-CRUD exclusively (drop the typed module), or
> have your typed module bypass auto-CRUD by registering custom Tauri
> commands (and skip the metadata's `entity_kind` so auto-CRUD
> doesn't double-sync).

---

## Step 6 — System-prompt fragment

Your pack's `prompt_fragment()` returns text that's appended to
Travis's system prompt for journal extraction, summary generation,
proactive nudges, and the Ask surface. **This is what makes Travis
intelligent in your vertical.**

Good fragments:

- Name the domain objects in the user's vocabulary (not yours).
- Tell the LLM what counts as the work-flow's pulse — what events
  matter, what state transitions are meaningful.
- Include 1–2 examples of how the user might phrase things.
- Stay under ~150 words. Travis has a long core prompt already; pack
  fragments are additive context, not a redo.

Example (the L2E pack):

```
You also help with after-school enrichment program ops:
- Track coaches placed at schools, their hourly rates, and hours worked.
- Maintain signed timesheets (signing_sheets) — these are how the
  Department of Finance authorizes payment.
- Draft NYC DoF-shaped invoices when hours have been signed off.

When the user mentions a coach by name, prefer recording the mention
even if no specific action is requested.
```

The journal extractor will then route mentions of "Maria" into the
`coaches` bucket because the L2E pack declared `entity_kinds = ["coach", "school", "dept"]`.

---

## Step 7 — Operational alerts

Each pack declares the layer-2 alerts that surface bottleneck states
— **the metric to capitalise on per vertical**. Splash renders these
prominently with severity-coded colour:

```rust
static ALERTS: &[AlertDef] = &[
    AlertDef {
        slug: "uninvoiced_hours",
        label: "Hours not yet invoiced",
        severity: AlertSeverity::Money,
        sql: "SELECT COUNT(*) AS count, \
                     NULL AS sample_label, \
                     NULL AS sample_id \
              FROM coach_hours h \
              WHERE NOT EXISTS ( \
                SELECT 1 FROM invoice i \
                WHERE i.coach_id = h.coach_id \
                  AND h.session_date BETWEEN i.period_start AND i.period_end \
                  AND i.status != 'void' \
              )",
    },
];
```

### Designing the alert

The question to ask: **"What is the single number that, if it goes
wrong, the user's business is in trouble?"** That's the headline alert.

For each vertical:
- **L2E (after-school):** uninvoiced billable hours. Money the user has earned but hasn't billed.
- **Tutoring:** unsent progress reports. Customer-facing communication that's overdue.
- **Therapy:** unsigned session notes. Compliance debt.
- **HVAC:** completed jobs not yet invoiced. Cash conversion lag.
- **Legal:** matters with no time entry in N days. Hours leaking out the door.

Most packs ship 1–3 alerts. Don't ship more — they stop being
"the metric" if there are eight of them.

### Alert SQL contract

The query must return **exactly one row** with three columns:

| Column | Type | Meaning |
|---|---|---|
| `count` | INTEGER NOT NULL | The headline number (rendered in pulse colour) |
| `sample_label` | TEXT NULL | Optional label for a representative affected row |
| `sample_id` | INTEGER NULL | Optional id for navigating to that row |

Use `NULL` for the sample fields if you don't have a representative
to surface. The frontend hides the sample link when both are null.

### Severity

- **`Money`** — dollars at risk. Highest priority; warn-yellow.
- **`Action`** — work that needs doing. Medium; pulse-blue.
- **`Info`** — informational state. Low; bone-grey.

### What alerts are NOT

Alerts aren't "show me a count of X". That's what the auto-CRUD list
view is for. Alerts answer **"what's stuck?"** They join state across
tables to find bottlenecks the user wouldn't notice by browsing one
table at a time.

---

## Step 8 — Optional: Action handlers

Action handlers are how the LLM proposes user-confirmable writes —
"draft an invoice", "send an email", "set a reminder". The user
sees a confirm card; clicking Confirm runs your handler.

Use action handlers when:
- The LLM should propose the operation but the user must approve.
- The operation has irreversible consequences (sending email, charging
  money, deleting history).
- The operation depends on multiple typed-table writes that need to
  succeed atomically.

Don't use action handlers for:
- Simple CRUD on a single record — that's auto-CRUD's job.
- Read-only operations the LLM should call autonomously — those are
  tools (next step).

### Implementing

Create `src-tauri/src/packs/<slug>/actions.rs`:

```rust
use serde::Deserialize;
use sqlx::SqlitePool;
use tauri::AppHandle;

use crate::actions::{ActionHandler, Applied};

pub struct <YourAction>Handler;

#[async_trait::async_trait]
impl ActionHandler for <YourAction>Handler {
    fn kind(&self) -> &'static str {
        "<your_action_kind>"  // matches what action_kinds() returns
    }

    async fn apply(
        &self,
        pool: &SqlitePool,
        app: &AppHandle,
        params_json: &str,
    ) -> anyhow::Result<Applied> {
        apply(pool, params_json).await
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Params { /* … */ }

async fn apply(pool: &SqlitePool, params_json: &str) -> anyhow::Result<Applied> {
    let p: Params = serde_json::from_str(params_json)?;
    // … do the work …
    Ok(Applied {
        message: format!("Did the thing for {}", p.something),
        json: serde_json::json!({ "id": new_id }).to_string(),
    })
}
```

Register the handler in your pack's `register_actions`:

```rust
fn action_kinds(&self) -> &'static [&'static str] {
    &["your_action_kind"]
}

fn register_actions(&self, registry: &mut crate::actions::ActionRegistry) {
    registry.register(Box::new(actions::<YourAction>Handler));
}
```

The journal extractor's proposed-action enum is built dynamically
from every enabled pack's `action_kinds()`, so the LLM gets a fresh
schema with your action listed.

---

## Step 9 — Optional: LLM tools

Tools are read-only operations the LLM can call **autonomously**
during a turn — no user confirmation. Use them for grounding the LLM
in fresh data: looking up the latest invoice for a coach, fetching a
URL, searching past notes.

```rust
// src-tauri/src/packs/<slug>/tools/<tool>.rs
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use crate::tools::{Tool, ToolContext};
use crate::llm::ToolDef;

pub struct <YourTool>;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input { query: String }

#[async_trait]
impl Tool for <YourTool> {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "<your_tool>".into(),
            description: "Brief, one-sentence description for the LLM.".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            }),
        }
    }

    async fn execute(&self, ctx: &ToolContext, input: Value) -> anyhow::Result<String> {
        let p: Input = serde_json::from_value(input)?;
        // … do the read-only work …
        Ok("Result string the LLM sees".into())
    }
}
```

Register in your pack's `register_tools`:

```rust
fn register_tools(&self, registry: &mut crate::tools::ToolRegistry) {
    registry.register(Box::new(tools::<YourTool>));
}
```

---

## Step 10 — Optional: Custom UI overrides

If auto-CRUD isn't enough for a particular table, ship a React
component. **Custom UI takes priority over auto-CRUD** when declared.

1. Drop the component at `src/packs/<slug>/ui/<Component>.tsx`.
2. Re-export it from `src/packs/<slug>/ui/index.ts`:
   ```ts
   export { default as <Component> } from "./<Component>";
   ```
3. Declare the override in `src/lib/packRegistry.ts`:
   ```ts
   import * as <slug>UI from "../packs/<slug>/ui";

   const PACK_UI_BUNDLES: Record<string, PackUIRegistry> = {
     "<your-slug>": <slug>UI as unknown as PackUIRegistry,
   };

   const OVERRIDES: OverrideDecl[] = [
     {
       packSlug: "<your-slug>",
       tableSlug: "<table>",
       view: "list",  // or "detail" or "form"
       component: "<Component>",
     },
   ];
   ```

The component receives `id` (for detail/form views) and `onClose`
(when wired up by the parent). Reach for the typed Tauri commands
(via your pack's typed CRUD modules) instead of `pack_table_*` —
they give you the typed objects without serde dance.

When to ship custom UI:
- **Calendar / Kanban / timeline views** — shapes auto-CRUD's table
  layout doesn't fit.
- **Domain-specific workflow** — e.g., the L2E invoice list shows PDF
  preview thumbnails that the auto ListView wouldn't.
- **Multi-step wizard** that doesn't fit a single form.

When NOT to ship custom UI:
- "Auto-CRUD is mostly fine but I want to tweak the list-view
  columns" → just edit `TableDef::list_view::columns`.
- "I want a different label for one field" → edit `FieldDef::label`.
- "I want a help tooltip on this input" → set `FieldDef::help`.

---

## Step 11 — Wiring it up

Once your code is in place, three more files need updates:

**`src-tauri/Cargo.toml`** — add to default features (so it bundles in default builds):

```toml
default = ["pack-lead-to-empower", "pack-tutoring", "pack-<your-slug>"]
pack-<your-slug> = []
```

**`src-tauri/src/packs/mod.rs`** — declare the module + add to `compiled_in_packs`:

```rust
#[cfg(feature = "pack-<your-slug>")]
pub mod <your_slug_underscored>;

pub fn compiled_in_packs() -> &'static [&'static dyn PackHandle] {
    &[
        // existing packs …
        #[cfg(feature = "pack-<your-slug>")]
        &<your_slug_underscored>::<YourPack>Pack,
    ]
}
```

**`src/packs/<slug>/ui/index.ts`** + **`src/lib/packRegistry.ts`** —
only if you have custom UI (step 10).

That's it. No core file changes needed beyond these registration
points. Run `cargo check`, fix any errors, and you're done.

---

## Step 12 — Testing your pack

```sh
# Make sure it compiles with all packs
cargo check --manifest-path src-tauri/Cargo.toml

# Make sure it compiles in isolation (only your pack)
cargo check --manifest-path src-tauri/Cargo.toml \
  --no-default-features --features pack-<your-slug>

# Make sure it compiles with the pack disabled
cargo check --manifest-path src-tauri/Cargo.toml \
  --no-default-features --features pack-tutoring

# Frontend type-check
./node_modules/.bin/tsc --noEmit
```

Then run the app:

```sh
npm run tauri dev
```

Verification checklist:

1. **Onboarding** (fresh install) shows your pack on step 8 with the
   description you wrote.
2. **Manage tabs** include your pack's primary tables, in declaration
   order.
3. **List views** show the columns from `list_view::columns` with the
   right cell formatting per `FieldType`.
4. **"+ New X"** opens a form with the right inputs in field order.
5. **Save** persists, returns to detail view showing the row.
6. **Edit** modifies it. **Delete** with confirm removes it.
7. **Splash** shows your alerts when their conditions trigger.
8. **Cmd+J** captures and the LLM extracts entities into your
   declared kinds.

---

## Step 13 — Versioning & evolution

### Adding a new field to an existing table

1. Write a new migration `0002_add_<field>.sql`:
   ```sql
   ALTER TABLE <table> ADD COLUMN <field> TEXT;
   ```
2. Add the migration to your pack's `MIGRATIONS` slice in `mod.rs`.
3. Add a `FieldDef` for it in `tables.rs`.
4. Bump the pack's `version()`.
5. Done. Auto-CRUD picks up the new field automatically; existing
   rows have NULL for it, which renders as `—`.

### Adding a new table

1. Write the migration.
2. Add a `TableDef` in `tables.rs`, append it to the `TABLES` slice.
3. Optionally add a typed domain module if you need pack-specific
   logic.
4. Bump version.

### Removing a field or table

**Don't.** SQLite makes column drops painful and per-customer data
loss is the kind of thing that ends customer relationships. If a
field is no longer used:

- Remove it from `default_in_list` and `list_view::columns` so it
  doesn't show in lists.
- Mark it deprecated in `FieldDef::help` so editors see a note.
- Leave the column in the DB. Cost is nothing.

If a field truly must go: write a migration that copies its data
elsewhere first, ship that, wait a release, then ship the column drop.

### Renaming a slug

`slug` is the canonical identifier. Renaming breaks foreign keys,
spine entity references, and pack metadata stored in `meta`. Don't.
Pick the right slug at creation time and live with it.

### Versioning the pack itself

Bump `version()` on every shipping change to the pack. Semver:

- **Major** — breaking schema change, removed fields, renamed kinds.
- **Minor** — new tables, new fields, new alerts.
- **Patch** — prompt-fragment tweaks, label changes, alert SQL fixes.

Pack version is independent of Travis core's version.

---

## Anti-patterns

- **"I'll write the whole UI from scratch."** Auto-CRUD covers 80% of
  pack UI needs out of the box. Reach for it first; ship custom UI
  only where the workflow demands it.
- **"I'll edit the journal extraction prompt to mention my domain."**
  Don't. Use `prompt_fragment()` — the journal extractor already
  appends every enabled pack's fragment. Editing core makes upgrades
  painful for everyone.
- **"My pack will write directly to core's `entity` table without
  using `entity_kind`."** Don't. The spine helpers exist precisely
  so you don't have to think about column conventions. Use
  `spine::entity::upsert` (auto-CRUD does this for you when
  `entity_kind` is set).
- **"I'll ship a hand-written `domain_cmd.rs` for everything."**
  Auto-CRUD covers it. Only typed commands when you have logic that
  doesn't fit metadata.
- **"I'll cram every operational concern into one big alert."** No —
  one alert per concern. Three alerts of "Money $X / Action / Info"
  is fine; one alert listing "$X uninvoiced + N unsigned + …"
  isn't actionable.
- **"My pack needs a workspace concept."** Workspaces are core
  (Phase 2 — see ROADMAP). Pack data lives in tables that workspaces
  organise; packs don't define workspaces themselves.

---

## Common patterns

### Person + place + transaction

L2E and tutoring follow the same shape: a "person" entity (coach,
tutor), a "place" entity (school, student), and a "transaction"
table that ties them together (coach_hours, session). This shape
covers a lot of verticals (HVAC tech + property + job; therapy
clinician + client + session). When in doubt, start here.

### Money-at-risk alert

Almost every vertical has one. The shape:

```sql
SELECT COUNT(*), NULL, NULL
FROM <transactional_table> t
WHERE NOT EXISTS (
    SELECT 1 FROM <billing_table> b
    WHERE b.<linked_field> = t.<linked_field>
      AND t.<date_field> BETWEEN b.period_start AND b.period_end
      AND b.status != 'void'
)
```

= "How many work units exist that aren't covered by a billing
record?" That's the universal "money on the table" question.

### Status-based filtering

When a table has a `status` enum that drives workflow (draft → sent →
paid → void; scheduled → completed → cancelled), the auto-CRUD list
view sorts on `created_at` descending by default. To let users filter
by status, that's the in-deferred filters work — for now, document
the workflow in `prompt_fragment` so the LLM understands the
transitions.

---

## When you need help from core

If your pack hits a wall that core can't accommodate without changes,
escalate. Open an issue (or a PR with a sketch) covering:

- The vertical you're trying to support.
- The specific pack-contract surface that's missing.
- Whether the missing surface is universal (every pack needs it) or
  vertical-specific (only yours does).

Universal additions go into the pack contract proper (extends
`PackHandle`). Vertical-specific needs are usually solvable with
typed Rust code in your pack.

---

## Reference packs

Read these alongside this guide:

- **`src-tauri/src/packs/lead_to_empower/`** — full-featured pack:
  schema metadata, typed domain modules, action handler, custom UI
  override (`InvoicesTab`), alerts, prompt fragment, PDF generation.
- **`src-tauri/src/packs/tutoring/`** — minimal pack: schema metadata
  only. Demonstrates that auto-CRUD is enough for a working pack.

When in doubt, copy from these.
