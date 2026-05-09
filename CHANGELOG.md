# Travis Changelog

## v0.5.0 — Knowledge graph foundation + Phase 3 token economy (2026-05-09)

Travis stops being a notes app with extras and starts forming
persistent memory. Every named person / place / organisation
mentioned in a journal entry now becomes a typed entity with a
mention timeline, confidence, and embedding. Co-occurrences become
graph edges. The LLM gets entity context injected silently on every
turn — Travis recognises Maria from prior captures without being
asked. Phase 3's token-economy work also rolls in: heuristic
fast-path, intent router, Haiku tier for capture-style turns. Design
in [`KNOWLEDGE_GRAPH.md`](./KNOWLEDGE_GRAPH.md).

### Highlights — Knowledge graph

- **Ambient entity capture.** Migration `0021_graph_extensions.sql`
  extends the existing spine `entity` table with embedding,
  confidence, tags, archive, and pack-table back-reference columns;
  `0022_entity_inference_state.sql` adds the cooldown timestamp for
  refinement prompts. Every journal turn passes through three
  layers: pack-declared kind extraction (coach, school, dept,
  tutor, student) at 0.7 confidence, generic
  `person/place/org:unknown` extraction at 0.5, and pack-typed CRUD
  at 1.0. Names recurring across kinds dedup against the highest-
  confidence existing entity in the workspace.
- **Mention timeline.** Every journal turn appends a `mentioned`
  spine event linking each touched entity to the source
  `journal_entry_id` with a 120-char snippet — a per-entity
  timeline ready to query without joining through journal text.
- **Co-mention edges.** Every unordered pair of entities mentioned
  in the same turn upserts a `mentioned_with` relation with a
  count tracked in `attributes_json`. Slice 5's canonical ordering
  prevents duplicate edges per pair.
- **Entity embedding pipeline.** Background sweeper
  (`graph_indexer::spawn`) keeps `entity.embedding_vector` current —
  every 5 minutes it picks up to 50 stale-or-never-indexed entities
  ordered by mentions desc, embeds via fastembed (already loaded
  for journal indexing), writes back. 7-day staleness threshold.
- **Graph-aware retrieval.** `memory::graph::retrieve` resolves the
  current turn's entity hints to known entities and surfaces a
  GRAPH MEMORY block alongside the existing text-similarity
  RELEVANT MEMORY: 5 most recent events, 3 most recent mention
  snippets, top-2 co-mentioned entities. Cheap by construction —
  indexed lookups with strict per-hit caps.
- **Inference helpers.**
  `recurring_mention_candidates`/`edge_proposals`/`name_conflicts`
  query for ambient `*:unknown` entities ripe for refinement, pairs
  co-mentioned often enough to deserve a labelled edge, and
  same-name conflicts across kinds. `apply_refinement` /
  `accept_edge_proposal` / `merge_entities` commit the user's
  answers — designed to be driven by conversation rather than a
  graph dashboard (per the minimal-surfaces directive).
- **Capture chip.** When extraction matches a pre-existing entity
  (mentions_count > 1), the overlay shows a faint *"→ Maria
  (coach)"* chip below the chat reply. Passive recognition; no
  interaction needed.

### Highlights — Phase 3 token economy

- **Cache hygiene.** Audit confirmed all four system-prompt
  builders (journal, summary, proactive, ask) keep dynamic content
  out of the cached prefix. Anthropic's `cache_control: ephemeral`
  on `system` covers tools transitively (canonical order is
  tools → system → messages). Docstring guard added to
  `build_system_prompt` so future contributors don't sneak in
  per-turn data.
- **Heuristic fast-path.** Pure greetings ("hi", "good morning"),
  acknowledgments ("thanks", "ok"), and direct task completions
  ("done 12", "mark 5 done") now skip the LLM entirely — synthetic
  Extraction flows through the existing persistence pipeline.
- **Intent router.** `classify_intent` runs cheap heuristics
  (question marks, leading question words, capture verbs, length)
  to bucket each turn into Query / Capture / Ambiguous. Captures
  skip the memory::retrieve fastembed call + full table scan;
  questions and ambiguous turns keep the full retrieval.
- **Haiku tier.** Capture-classified turns route to
  `claude-haiku-4-5` instead of `claude-sonnet-4-6` — extraction is
  structural and Haiku handles it well at ~3-4× lower cost. Honours
  the user's explicit `profile.model`; only swaps the implicit
  default. Non-Claude providers fall back to default since they
  don't have a comparable cheap tier wired up.

### Manage redesign

The horizontal tab strip is replaced by a sidebar with grouped
navigation: **Capture** (Ask, Tasks, Threads, Reminders) at the
top, then one group per enabled pack with the pack's display name
as the group header (Lead to Empower → Coaches/Schools/Hours/
Sheets/Invoices; Tutoring → Tutors/Students/Sessions/Reports), with
**Diagnostics** as a trailing collapsible group only visible when
the dev toggle is on. People/Places/Orgs tabs explicitly **not**
shipped per the minimal-surfaces directive — the graph is internal
magic, not a CRUD surface.

### Bug fixes

- **Reminders scheduler** was logging *"no column found for name:
  workspace_id"* every 30 seconds because Phase 2's reminder
  scoping pass missed `due_now()`'s SELECT. Now selects the column
  the `Reminder` struct expects.
- **Knowledge graph tabs reverted** in the same release — they were
  briefly added during slice 12 before the user's *"keep Manage
  minimal"* directive landed. Frontend KnowledgeTab.tsx,
  lib/knowledge.ts, and the `list_entities_by_family` Tauri command
  removed; backend graph helpers stay since they drive the chip
  and prompt-injection surfaces.

### Migrations

- `0021_graph_extensions.sql` — schema_version 20 → 21. Adds
  `embedding_vector`, `embedding_indexed_at`, `confidence`, `tags`,
  `archived_at`, `pack_table_id` columns to `entity`; new indexes on
  archived filter / pack-projection lookup / kind-by-workspace
  listing / mention-timeline ordering / relation traversal.
- `0022_entity_inference_state.sql` — schema_version 21 → 22. Adds
  `last_clarification_at` to `entity` plus an index for the
  refinement-candidate query.

### What this enables

Travis recognises names across captures without being told. Cmd-J
"hours with Maria today" auto-resolves to the existing coach Maria
and pulls her recent mentions, related entities, and last-seen into
the LLM context. Inference helpers are ready to drive
through-conversation refinement — Travis can ask "is Maria the L2E
coach or the personal contact?" and apply the answer when the user
replies in chat. The capture chip is the only visible UI surface;
the rest is silent.

## v0.4.0 — Workspaces (2026-05-08)

Travis now keeps separate worlds separate. A workspace scopes every
operational record — tasks, reminders, journal entries, conversations,
embeddings, and every typed pack table — to the world it belongs in.
Switch workspaces from the header chip; the Manage tabs, splash, and
proactive nudges all re-scope. Sensitive categories (Health, Therapy,
Legal, Finance) stay isolated by default per the asymmetric rule —
they don't bleed into other workspaces' reads, and Travis won't auto-
route captures into them. Design is in
[`WORKSPACES.md`](./WORKSPACES.md).

### Highlights

- **Per-row `workspace_id` scoping.** Migration `0020_workspaces.sql`
  adds the column to every scoped core table (task, reminder,
  journal_entry, conversation, embedding, entity, relation, event,
  summary, email_sent) and the L2E pack's typed tables; the tutoring
  pack adds it via its own per-pack migration. Existing rows backfill
  into the default `Personal` workspace.
- **Active + visible workspace state.** `AppState.workspace` holds
  `{active_id, visible_ids}`, refreshed on switch. Reads expand
  across `visible_ids` (active + cross-visible non-sensitive peers);
  writes stamp `active_id`. Asymmetric isolation: sensitive
  workspaces collapse `visible_ids` to themselves.
- **Workspace switcher.** Header chip shows the active workspace's
  name with a warn-yellow tint + lock icon for sensitive ones. Click
  to switch. The `workspace-changed` event refreshes every subscribed
  view.
- **Settings → Workspaces.** Full CRUD: create, rename, recategorise,
  toggle cross-visibility, archive, unarchive. Sensitive cross-
  visibility toggle includes a warning copy.
- **Auto-close idle conversations.** Daily background tick closes any
  `awaiting_user` conversation whose `updated_at` is 7+ days old, so
  the resume-where-you-left-off surface stays clean.
- **Workspace-aware system prompts.** Journal, summary, ask, and
  proactive nudge prompts include the active workspace's name +
  category. Sensitive workspaces get an extra do-not-bleed line.
- **Workspace-scoped semantic memory.** Embeddings denormalise
  `workspace_id` at insert time; retrieval scans only rows in the
  visible set. Cross-workspace recall happens silently when active
  is non-sensitive; sensitive contexts only see themselves.
- **Intelligent LLM routing.** The journal extraction tool gains a
  `workspaceRouting` field. High/medium-confidence picks for non-
  sensitive targets restamp the journal entry, conversation,
  embeddings, tasks, and reminders into the routed workspace. Low-
  confidence and sensitive targets demote to a clarifying question.
  The overlay shows a "Captured to <name>" chip when routing
  diverges from the active workspace.
- **Onboarding workspace step.** New step between the pack picker
  and the done screen lets the user add a Work / Personal / Other
  workspace inline. Sensitive categories deferred to Settings — they
  deserve a deliberate add.

### Migrations

- `0020_workspaces.sql` — schema_version 19 → 20. Creates the
  `workspace` table, the default `Personal` row, the
  `meta.active_workspace_id` pointer, and `ALTER TABLE ADD COLUMN
  workspace_id INTEGER NOT NULL DEFAULT 1` on every scoped core
  table + the L2E pack's typed tables. Indexed for filter speed.
- Tutoring pack `0002_workspace_id.sql` — adds `workspace_id` to
  tutor / student / session / progress_report.

### Known v1 cuts

- 3-capture suggestion to switch the active workspace (deferred —
  routing works per-capture, switching stays manual).
- Per-entity remembered disambiguation (e.g. "Maria → always
  Personal") deferred — routing decides fresh each turn.
- Persistent sensitive-workspace banner across the whole app —
  switcher chip is the only indicator for now.

## v0.3.0 — Plugin platform + runtime pack selection (2026-05-08)

Packs become a real plugin format. Every primary table from every
enabled pack now renders as a Manage tab — list, detail, edit, delete
— with **zero pack-side UI code**. Pack authors ship schema metadata;
core materialises the UI dynamically. Custom React components are
optional, ship inside the pack, and override the auto-CRUD when the
UX warrants it. The pack-authoring guide is at
[`AUTHORING_PACKS.md`](./AUTHORING_PACKS.md).

### Highlights

- **Schema-driven auto-CRUD.** New `PackHandle::tables()` declares
  every typed table with rich field metadata (`FieldType` covers
  Text, LongText, Email, Phone, Integer, Number, Currency, Date,
  DateTime, Bool, Enum, Ref, Json, Timestamp). Generic Tauri commands
  (`pack_table_list / _get / _upsert / _delete`) build SQL from the
  metadata; SQL-injection-safe by construction.
- **Auto list / detail / form views.** Frontend `src/lib/autoCRUD/`
  contains type-aware components (`ListView`, `DetailView`,
  `FormView`, `FieldCell`, `FieldInput`) that render any pack's
  table. Sortable columns, click-to-detail, edit forms, two-click
  delete.
- **Custom UI overrides.** Pack-shipped React components live at
  `src/packs/<slug>/ui/` and register in `src/lib/packRegistry.ts`.
  The L2E `InvoicesTab` moves into the L2E pack and demonstrates
  the override path.
- **Operational alerts.** New `PackHandle::alerts()` returns
  `AlertDef` entries with severity (money / action / info) and SQL
  for the headline metric. The Splash screen renders these
  prominently above the entity stats. L2E ships *Hours not yet
  invoiced* + *Signing sheets awaiting signature*; tutoring ships
  *Progress reports drafted but not sent* + *No-show sessions to
  follow up*.
- **Runtime pack selection.** `meta.pack.<slug>.enabled` per DB
  decides which compiled-in packs participate. Onboarding step 8
  asks "What should Travis help with?"; Settings → Packs lets users
  toggle anytime. Cargo features stay as a build-time lever for
  distros (`--no-default-features --features pack-tutoring`).
- **Tutoring pack.** The second vertical pack ships in the default
  build, runtime-disabled by default. Validates that the abstraction
  isn't accidentally L2E-shaped — writing the second pack felt
  mechanical: declare schema, ship migrations, register entity
  kinds. No UI code, no Tauri commands.

### What this enables

A new vertical pack now needs only: `tables.rs` schema declarations,
a SQL migration, an entity-kinds list, a prompt fragment, an alert
or two. Roughly half a day from the right MARKET.md vertical to a
working pack with full UI. Custom UI is opt-in for places the
auto-CRUD shape doesn't suit.

### Migrations

No new core migrations in v0.3.0; pack metadata lives in compile-
time `&'static` data. The tutoring pack's `0001_init.sql` runs as
a per-pack migration, tracked in `meta.pack.tutoring.schema_version`.

### Breaking changes

None for end users on the default build. The L2E pack's invoice tab
is now sourced from `src/packs/lead_to_empower/ui/InvoicesTab.tsx`
instead of `src/manage/tabs/InvoicesTab.tsx` (path change only;
identical behaviour).

### Internal docs

- `PLUGIN_PLATFORM.md` — design spec; slices 1–7 shipped, slice 8
  (onboarding hooks) deferred per `DEFERRED.md`.
- `AUTHORING_PACKS.md` — comprehensive guide to building and
  evolving packs.

---

## v0.2.0 — Pack architecture (2026-05-07)

Travis is now generic at the data-model level. The vertical-specific
code that used to be baked into core (after-school enrichment program
ops — coaches, schools, signing sheets, NYC DoF invoicing) lives
entirely as an installable pack under
`src-tauri/src/packs/lead_to_empower/`. Future verticals — tutoring,
home care, therapy, field service — ship as their own packs and plug
into the same extension points.

### Highlights

- **Pack architecture.** New `PackHandle` trait declares a pack's
  slug, version, migrations, prompt fragment, entity kinds, action
  kinds, and registration hooks for tools and action handlers. Packs
  gate compilation behind a Cargo feature flag
  (`pack-lead-to-empower` is default-on).
- **Universal spine.** Three new core tables — `entity`, `relation`,
  `event` — give every domain object a place in a cross-pack
  rendezvous index. Packs sync their typed data into the spine via
  explicit writes, so retrieval and the future knowledge graph see
  one unified view.
- **Action + tool registries.** Action dispatch and tool registration
  are runtime registries that packs extend at startup. The static
  `actions::dispatch` match and the hardcoded
  `tools::read_only_registry` are gone.
- **Dynamic journal extraction.** The schema for the journal
  extraction LLM tool derives entity buckets and the
  `proposedActions` enum from the live pack registry. Adding a pack
  with new entity kinds doesn't require touching `journal.rs`.
- **Pack-supplied prompt fragments.** Each pack contributes a
  system-prompt fragment that's appended to journal, proactive,
  summary, and ask-Travis prompts.
- **Frontend pack gating.** The Manage tab list hides pack-supplied
  UI (the L2E Invoices tab) when the corresponding pack is disabled.
  `appStatus.enabledPacks` exposes the pack list.
- **`task` graduates to core.** L2E-specific `link_kind` CHECK
  constraint dropped; new `entity_id` column links to the spine.

### Migrations

- `0018_pack_spine.sql` — generalises `entity_index` → `entity`,
  adds `relation` and `event`.
- `0019_task_to_core.sql` — recreates `task` without the L2E CHECK
  constraint, adds `entity_id`.

Both run cleanly on a fresh DB. Existing dev installs that predate
the LF-pinning of migration files in `.gitattributes` may hit a
sqlx checksum drift on first launch (delete the data dir to reset).

### What this enables

Building the next vertical pack — tutoring, home care, therapy, MSP,
legal — is now scoped to creating a `src-tauri/src/packs/<slug>/`
directory with a `PackHandle` impl plus typed tables and Tauri
commands. No core changes needed. `MARKET.md` lists the 20 target
verticals; `PACKS.md` is the format spec.

### Breaking changes

None for end users on the default build. The L2E pack ships enabled
by default and behaves identically to v0.1.3.

### Internal docs

- `PACKS.md` — pack format spec.
- `PACKS_AUDIT.md` — full refactor record (12 steps).
- `ROADMAP.md` — Phase 1 marked shipped; remaining bullets cover the
  second-pack validation milestone.

---

## v0.1.3 — Dark window chrome + granular proactive schedule (2026-04-28)

See [GitHub release](https://github.com/myketheguru/travis-releases/releases/tag/v0.1.3).

## v0.1.2 — macOS visual fixes

## v0.1.1 — Startup hardening, voice dropdown, onboarding overflow fix

## v0.1.0 — Initial release
