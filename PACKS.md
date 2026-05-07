# Pack Format — v0.1 (draft)

A **pack** is the unit of vertical extension in Travis. One pack = one
operational shape (after-school programs, tutoring, home care, therapy,
field service, etc.). Travis core is the kernel; packs are the standard
library.

This document specifies what a pack *is* on disk and how Travis core
loads one. The point of the format is that **building the second pack
should feel like writing a known thing, not inventing a new one** —
that's the validation we're aiming for in week 5–6 of the Phase 1
sequence (see [ROADMAP.md](./ROADMAP.md)).

For the codebase-audit findings that drove this format, see
[PACKS_AUDIT.md](./PACKS_AUDIT.md).

---

## The data model: spine vs typed tables

Travis core does **not** assume the shape of any vertical's domain
data. Core ships a small, universal **spine** — primitives every
vertical actually has in common — and each pack ships its own
**typed tables** for its domain entities. Pack code keeps the spine
in sync with its typed tables (explicit writes, not triggers — easy
to reason about).

This avoids the trap of locking core onto a specific shape (e.g.,
"every vertical has tasks") that breaks the moment a vertical needs
a different shape (HVAC has *jobs*; therapy has *cases*; legal has
*matters*).

### The spine (core tables)

| Table | Purpose | Status |
|---|---|---|
| `entity` | Generic rendezvous for every domain object — coach, client, case, job, property, invoice. (id, kind, name, normalized_name, pack_slug, attributes_json) | Generalize existing `entity_index` |
| `relation` | Typed edges between entities — "coach works at school", "case belongs to client", "job at property". (from_entity, to_entity, type, attributes_json) | **New** |
| `event` | Anything that happened to an entity — "hours logged", "session completed", "job dispatched", "invoice issued". (entity_id, kind, occurred_at, attributes_json) | **New** |
| `task` | Thin generic work-item: title, status, due, entity_id, attributes_json. **Opt-in convenience** — packs that need a richer work-item (HVAC `job`, therapy `case`) ship their own and ignore `task`. | Move out of L2E pack into core, drop L2E-specific CHECK constraints |
| `journal_entry`, `conversation`, `message`, `reminder`, `audit_log`, `embedding`, `user_profile`, `proposed_action` | Already core | No change |

### Pack typed tables

Each pack ships strongly-typed tables for the shapes it actually
cares about. L2E ships `coach`, `school`, `coach_hours`,
`signing_sheet`, `invoice`. HVAC would ship `job`, `technician`,
`property`. Therapy would ship `client`, `case`, `session`, `note`.

Pack code is responsible for keeping the spine in sync:

- When a pack creates a row in its typed table, it also INSERTs a
  matching row into `entity` (with `kind = "<pack_kind>"`,
  `pack_slug = "<this_pack>"`, `attributes_json` holding any extra fields
  the spine should know about).
- When relevant state changes, it INSERTs into `event`.
- Cross-entity references go into `relation`.

This is explicit — no clever schema or triggers. Reading the pack
code, you can trace exactly when the spine is updated.

### Why the spine matters

- **Cross-pack retrieval works from day 1.** A user with both an
  L2E pack and a tutoring pack can ask "what's outstanding?" and
  Travis pulls from both — the spine is the unified view.
- **Phase 4 graduates cleanly.** When the typed knowledge graph
  lands (ROADMAP Phase 4), it adds embeddings on top of `entity`,
  `relation`, `event`. The schema doesn't need to change.
- **The pack-installable surface stays small.** A user-installable
  pack (Phase 2) ships migrations + prompt + entity-kind
  declarations + remote-tool definitions. It does NOT need to
  re-design the data model — it plugs into the spine.

### What core does NOT own

- Domain entities (coach, client, patient, case, job, property).
- Domain workflows (signing sheets, intake forms, billing cycles).
- Domain-specific work-items when the generic `task` doesn't fit
  (HVAC `job`, therapy `case`, legal `matter`).
- Vertical-specific tone, examples, capability boasts in the system
  prompt — that's `prompt.md` per pack.

If a future pack needs a shape that the spine can't represent in
`attributes_json`, that's a signal to extend the spine — not to graduate
the pack's table into core. The bar for adding to core: at least
3 verticals already need it.

---

## What a pack contains

A pack is a directory (or a zip — same shape, packed). Layout:

```
my-pack/
├── pack.toml                  # manifest — required
├── migrations/                # SQLite schema — optional
│   ├── 0001_init.sql
│   └── 0002_add_signatures.sql
├── tools/                     # tool definitions — optional
│   ├── list_coaches.rs        # native (compiled into Travis)
│   └── send_to_payer.json     # remote (REST wrapper, Phase 3)
├── actions/                   # proposed-action handlers — optional
│   └── propose_invoice_draft.rs
├── prompt.md                  # system prompt fragment — optional
├── entities.toml              # entity kinds for the index — optional
├── ui/                        # UI hints — Phase 2 only, ignore for now
│   └── tabs.toml
├── templates/                 # arbitrary pack-internal assets
│   └── invoice_template.hbs
└── README.md                  # human-readable description
```

For **v0.2 (the first pack ship)**, only `pack.toml`, `migrations/`,
`prompt.md`, and `entities.toml` are dynamically loaded. `tools/` and
`actions/` written in Rust are compiled into the binary and gated by
their parent pack being installed (a Cargo feature flag). True
runtime-loaded tools are Phase 3 work (see ROADMAP).

## The manifest — `pack.toml`

```toml
[pack]
slug          = "lead-to-empower"        # globally unique, lowercase, hyphens
name          = "Lead to Empower"        # human label
version       = "0.1.0"                  # semver
travis_min    = "0.2.0"                  # minimum Travis core version
description   = "After-school enrichment program ops — coaches, schools, signing sheets, NYC DoF invoices."
author        = "Travis core"
license       = "MIT"

[pack.depends]
# Other pack slugs this pack expects to be present. Empty for self-contained packs.

[pack.system_prompt]
# How the pack's prompt fragment is merged into Travis's system prompt.
mode = "append"          # append | prepend | replace_section
section = "capabilities" # only when mode = "replace_section"

[pack.entity_kinds]
# What this pack adds to the entity index. The schema rejects unknown kinds
# unless they're declared by an installed pack (see PACKS_AUDIT.md decision 5).
kinds = ["coach", "school", "dept"]

[pack.action_kinds]
# Action kinds this pack registers handlers for. Travis core's action
# dispatcher routes by kind; pack-supplied handlers must match these names.
kinds = ["propose_invoice_draft"]

[pack.work_item]
# Names the pack's primary "work-item" table for system-prompt and UI
# language. If the pack uses core's generic `task` table, omit this
# section entirely.
table   = "signing_sheet"   # the pack-typed table that holds the work
plural  = "signing sheets"  # how the LLM should refer to them
singular = "signing sheet"
```

Fields not yet specified (left for after v0.2): `pack.permissions`
(what kinds of writes the pack is allowed), `pack.signing` (signature
for marketplace distribution), `pack.config` (per-install settings the
user fills in via UI).

## Migrations

Every pack's migrations run **after** core migrations and **after**
already-applied migrations from packs already installed. Each pack
keeps its own migration tracking row in `meta`:

```
meta.key = "pack.lead-to-empower.schema_version"
meta.value = "2"                                    # last-applied number
```

Migration files are numbered `0001_*.sql`, `0002_*.sql`, etc., per
pack — pack numbering is independent of core's `_sqlx_migrations`.
Travis core won't apply two packs whose tables collide; the second
install fails fast with a readable error. (For v0.2 we'll accept that
constraint; namespaced table prefixes can graduate to a real feature
once we've got 3+ packs.)

**Pin migrations to LF** — `.gitattributes` already enforces this.

## Tools

A pack contributes tools into the LLM's `read_only_registry` and (for
write tools) into the action dispatcher.

For **v0.2**, all pack tools are **native** (Rust, compiled into the
binary). They live under `src-tauri/src/packs/<slug>/tools/`. At
startup, if the pack is enabled, the pack's `register_tools()` function
is called — it adds tool implementations to the existing
`ToolRegistry`. This is exactly how `read_only_registry` already works
today; the only change is that pack tools register themselves
conditionally rather than the core `read_only_registry()` listing them.

**Phase 3** introduces remote tools (JSON-defined REST wrappers,
fetched and registered at runtime). Format and security model TBD; not
in scope for v0.2.

### Tool definition shape (Rust, native)

```rust
// src-tauri/src/packs/lead_to_empower/tools/list_coaches.rs
pub struct ListCoachesTool;

#[async_trait]
impl Tool for ListCoachesTool {
    fn definition(&self) -> ToolDef {
        ToolDef {
            name: "list_coaches".into(),
            description: "List all coaches and their billable rates.".into(),
            input_schema: json!({ "type": "object", "properties": {} }),
        }
    }
    async fn execute(&self, ctx: &ToolContext, _input: Value) -> Result<String> {
        // ... query the pack's tables via ctx.db.pool ...
    }
}
```

This is identical to how `web_fetch::WebFetchTool` is shaped today.

## Action handlers

Travis core's `actions.rs::dispatch` matches on `action.kind` and
routes to a hard-coded handler. To make this pack-extensible, core
gains a registry:

```rust
type ActionHandler = fn(&SqlitePool, &AppHandle, &str) -> BoxFuture<Result<Applied>>;
struct ActionRegistry { handlers: HashMap<String, ActionHandler> }
```

Pack code at startup calls `registry.register("propose_invoice_draft",
apply_propose_invoice_draft)`. The dispatcher becomes:

```rust
match registry.lookup(action.kind) {
    Some(handler) => handler(pool, app, &action.params_json).await,
    None => bail!("unsupported action kind: {}", action.kind),
}
```

Built-in actions (`defer_task`, `set_reminder`, `write_clipboard`,
`run_shell_command`, `send_email`, `update_profile_context`) stay in
core and pre-register themselves. Pack-defined kinds register
alongside.

## Prompt fragments — `prompt.md`

A markdown file. Content is concatenated into Travis's system prompt
according to `pack.system_prompt.mode`:

- `append` (default) — text appears after core prompt, before any
  user-context block.
- `prepend` — text appears at the very top.
- `replace_section` — replaces a named section in the core prompt
  (sections are marked with HTML comments like
  `<!-- pack-section: capabilities -->` in the core prompt template).
  Used sparingly; mostly for vertical-specific tone tweaks.

Example fragment for the Lead-to-Empower pack:

```markdown
You also help with after-school enrichment program ops:
- Track coaches placed at schools, their hourly rates, and hours worked.
- Maintain signed timesheets (signing_sheets) — these are how the
  Department of Finance authorizes payment.
- Draft NYC DoF-shaped invoices when hours have been signed off.

When the user mentions a coach by name, prefer recording the mention
even if no specific action is requested.
```

## Entity kinds — `entities.toml`

```toml
[[entity_kind]]
slug = "coach"
display = "Coach"
description = "A contractor placed at one or more schools."

[[entity_kind]]
slug = "school"
display = "School"
description = "A site where a coach is placed."

[[entity_kind]]
slug = "dept"
display = "Department"
description = "An agency or department that pays invoices (e.g., NYC DoF)."
```

These declarations let core's `identity::record_mention` accept these
kinds without rejection, and let the journal-extraction schema generate
the right entity buckets for the LLM. (For v0.2, the journal extractor
keeps fixed bucket names — `people / orgs / agencies` — and the pack's
entity-kind declarations map to them; full pack-driven schema
generation is Phase 2.)

## UI hints (Phase 2 only — ignore for v0.2)

For v0.2, the L2E `InvoicesTab` stays compiled into the binary and is
shown only when the L2E pack is installed (gated by a runtime check
against the pack registry). The pack itself owns the migrations and
prompt fragment; the tab is "L2E-shaped UI living in core" until we
have enough packs to justify a real plugin UI system.

When that happens, `ui/tabs.toml` will declare:

```toml
[[tab]]
slug = "invoices"
label = "Invoices"
component = "...some declarative component spec..."
```

Detail format TBD.

## Pack discovery and lifecycle

For v0.2, packs are bundled at build time:

1. Each pack is a directory under `src-tauri/packs/<slug>/`.
2. A Cargo feature flag (`pack-lead-to-empower`) gates compilation.
3. At startup, Travis iterates **enabled** packs (compile-time list),
   runs each one's migrations against the SQLite DB (recording
   `meta.pack.<slug>.schema_version`), and calls
   `register_tools(&mut registry)` and
   `register_actions(&mut action_registry)`.
4. Disabling a pack (next build with the feature off) is supported.
   Migrations stay applied — uninstall doesn't drop tables (data
   preservation > schema cleanliness). A future `travis pack uninstall`
   command can offer destructive cleanup.

User-installable packs (drag-and-drop a `.zip` onto Travis →
auto-load) is the **week 4** milestone in the Phase 1 sequence.
Implementation: pack zips extract to `%APPDATA%/Travis/packs/<slug>/`,
get loaded at startup. Native (Rust) tools won't work for
user-installed packs until Phase 3 (remote tools); until then,
user-installable packs are limited to migrations + prompt + entity
kinds + remote tool definitions.

## Versioning

- Pack version is semver. Travis core enforces `pack.travis_min` at
  install time.
- Migration count per pack is monotonic; later versions add migrations
  but never edit applied ones (same rules as `sqlx::migrate!`).
- Breaking schema changes within a pack require a version bump and a
  forward-only migration.
- Pre-1.0 packs may break compatibility freely between minor versions
  (matches Travis core's pre-1.0 status).

## Security model (sketch)

- A pack's tools have full DB access via `ToolContext.db` — no
  isolation. Trust model: native packs are part of the binary, written
  by you (or someone you trust enough to compile in).
- Remote tools (Phase 3) will get a permission gate — read vs write,
  external-call vs local-only, irreversible vs reversible. The
  `proposed_action` confirmation card already covers the irreversible
  case.
- User-installed packs (Phase 2) will be sandboxed: migrations only,
  no compiled code. They contribute schema + prompt + remote-tool
  definitions; they cannot define native Rust tools.

## Open questions for sanity-check

These are the design calls I made above; flag anything that should be
different:

1. **Cargo feature flags as the v0.2 enable mechanism.** Cheap, works
   today, but means pack management is a build-time concern, not a
   user-runtime concern. OK because v0.2 ships a single pack (L2E)
   bundled by default. Drag-and-drop install lands week 4.
2. **Pack migrations in their own `meta`-tracked counter, not
   `_sqlx_migrations`.** Means we don't reuse `sqlx::migrate!`'s
   checksum guarantees for pack migrations. We'll write our own runner
   (~50 lines). Acceptable trade-off.
3. **Action handlers as a runtime registry rather than compile-time
   match.** Modest refactor of `actions.rs::dispatch`. Worth it.
4. **Entity kinds: `record_mention` accepts pack-declared kinds; the
   journal extraction schema keeps fixed `people / orgs / agencies`
   buckets** for v0.2 (renamed from `coaches / schools / depts`).
   Pack-driven dynamic schema generation defers to Phase 2.
5. **Universal spine, not "task-everywhere".** Core ships
   `entity`, `relation`, `event` (the spine) plus a thin
   opt-in `task`. Packs that need a richer primary work-item
   (HVAC `job`, therapy `case`) ship their own typed table and
   ignore `task`. The pack manifest declares its primary work-item;
   the system prompt names it accordingly. The current
   `link_kind IN ('coach','school','...')` CHECK gets dropped;
   `task.entity_id` becomes a soft FK into `entity`.
6. **The L2E `InvoicesTab` and Tauri commands stay in the binary,
   gated on pack-installed.** Real pack UI is Phase 2. Cheap path: in
   v0.2, the binary contains both the L2E pack (under `packs/`) and
   the L2E UI (under `src/manage/tabs/`); the latter is shown only when
   the former is loaded. Not pretty but unblocks shipping.

If any of these answers should be different, the audit doc has the
reasoning — change them there first, then re-spec here.
