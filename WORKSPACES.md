# Workspaces — Phase 2 (spec)

A workspace is a **scoped namespace for operational data**. Every
task, reminder, journal entry, conversation, pack record, and spine
entity belongs to exactly one workspace. The user is always in one
*active* workspace; new captures land there. Travis's retrieval +
LLM context combine the active workspace with any other workspaces
the user has marked *cross-visible*, while sensitive categories
(health, therapy, legal, finance) default to isolated.

This unlocks the realistic shape of how non-trivial users actually
work — Taylor's day is L2E + her household + maybe a side project.
She wants Travis aware of all of it without her having to repeat
context, but she also wants therapy notes nowhere near a coach
follow-up. The workspace is the boundary that makes both true.

For the broader build plan see [ROADMAP.md](./ROADMAP.md). For the
data-model spine workspaces sit on top of, see
[PLUGIN_PLATFORM.md](./PLUGIN_PLATFORM.md).

---

## What a workspace is

```rust
struct Workspace {
    id: i64,
    slug: String,           // stable identifier — "personal", "l2e", "side-project"
    name: String,           // human label — "Personal", "Lead to Empower", "Tutoring side"
    category: Category,     // see below — drives defaults
    cross_visible: bool,    // does this workspace's data show up when other
                            // workspaces are active? Defaults from category.
    archived_at: Option<String>,  // soft delete; archived workspaces hide
                                  // from the switcher but data persists.
    created_at: String,
    updated_at: String,
}

enum Category {
    Work,        // default cross_visible = true
    Personal,    // default cross_visible = true
    Health,      // default cross_visible = false (sensitive)
    Therapy,     // default cross_visible = false
    Legal,       // default cross_visible = false
    Finance,     // default cross_visible = false
    Other,       // default cross_visible = true
}
```

Workspaces are flat — no nesting, no hierarchy. A user has 1–10 of
them, named by purpose. The category controls *defaults*, not
behaviour; users override `cross_visible` per workspace when they
need to.

---

## Active workspace + visible workspaces

At any moment Travis maintains:

- **Active workspace** — exactly one. New journal entries, tasks,
  reminders, and pack rows are stamped with this workspace's id.
  Stored in `meta.active_workspace_id`.
- **Visible workspaces** — the active workspace plus every
  *non-archived* workspace where `cross_visible = true`. Used for
  reads: list views show rows from any visible workspace; LLM
  retrieval (memory search, entity resolution, related-conversation
  injection) pulls from the visible set; the proactive nudge prompt
  considers the visible set when deciding what's worth flagging.

Both are cached on `AppState` at startup and refreshed when the user
toggles the switcher or changes a workspace's `cross_visible` flag.

---

## Cross-workspace visibility — what "visible" actually does

When the user is in workspace X, with X + Y + Z visible (all marked
`cross_visible = true`):

**Reads:**
- Task list, reminder list, conversation list — show rows from X, Y, Z
  with a small workspace badge per row.
- Pack auto-CRUD list views — show rows from any visible workspace.
- Memory search — searches journal entries from X, Y, Z.
- Entity resolution — when the LLM extracts a name like "Maria",
  it can match against entities in X, Y, Z.
- Splash alerts — packs run their alert SQL across the visible
  workspace set.

**Writes:**
- Always to the active workspace (X). New tasks, journal entries,
  pack rows get `workspace_id = X.id`.
- The user can move a row's workspace_id (rare; needs explicit
  action like a "Move to workspace…" menu).

**Sensitive categories — Health, Therapy, Legal, Finance:**
- Default `cross_visible = false` at create time.
- Their data NEVER appears when a non-sensitive workspace is active.
- When the user switches *into* the sensitive workspace, only that
  workspace's data is visible (even if some other workspace is
  marked `cross_visible = true`). This is the asymmetric isolation
  rule — sensitive workspaces don't *contribute* to others' views,
  AND they don't *receive* data from others' views.
- Override available but warned. Users can manually toggle
  `cross_visible = true` on a sensitive workspace; the toggle in
  Settings shows a confirmation like "Therapy notes will appear in
  cross-workspace queries — continue?".

---

## Data model: which tables get `workspace_id`

Two categories of tables. **Scoped** tables get a new
`workspace_id INTEGER NOT NULL` column with an index. **Unscoped**
tables stay as-is.

### Scoped (gets `workspace_id`)

Core:
- `task`
- `reminder`
- `journal_entry`
- `conversation`
- `entity` (spine)
- `relation` (spine)
- `event` (spine)
- `summary`
- `email_sent` (the workspace context the email was sent from)

Every L2E pack table:
- `coach`, `school`, `coach_hours`, `signing_sheet`, `invoice`

Every Tutoring pack table:
- `tutor`, `student`, `session`, `progress_report`

Every future pack's primary tables — schema metadata's `TableDef`
gains a `workspace_scoped: bool` field (default `true`). Packs opt
out only when there's a compelling reason (a global lookup table
with no per-workspace meaning, e.g., a future "country codes" table).

### Unscoped (no `workspace_id`)

- `user_profile` — one identity per install.
- `meta` — per-install configuration.
- `event_log` (behavioral) — pattern detection runs across the
  user's whole behavioural history. Workspace context is included
  in the payload_json when relevant.
- `app_feedback` — capability-gap tracking; install-wide.
- `proposed_action` — sits within a conversation, which is scoped,
  so the action's effective workspace = the conversation's.
- `_sqlx_migrations` — sqlx tracking.

---

## Migration strategy

Single migration, `0020_workspaces.sql`:

1. Create `workspace` table.
2. Insert the default `Personal` workspace (id will be 1).
3. For each scoped table, `ALTER TABLE … ADD COLUMN workspace_id INTEGER NOT NULL DEFAULT 1;` plus an index.
4. Set `meta.active_workspace_id = '1'`.

The `DEFAULT 1` backfills every existing row into the Personal
workspace. Users see no behaviour change — their existing data is
all in one workspace; they can create more later.

> **Why no FK constraint on `workspace_id`?** SQLite's `ALTER TABLE
> ADD COLUMN` doesn't accept a `REFERENCES` clause. We could
> recreate every table to get the FK, but the cost (per-table
> recreate, manual data copy, index rebuild — for ~13 tables) isn't
> worth it. Travis writes are gated through Rust paths that always
> resolve to a real workspace id via `AppState.active_workspace_id`.
> SQLite's `foreign_keys = ON` setting still gives us referential
> integrity for the FKs that *are* there (entity → relation, etc.).

---

## Lifecycle

**Create.** User goes to Settings → Workspaces → "+ New workspace"
or hits a quick-add from the switcher. Picks name + category. Slug
auto-generates from name; can override. The workspace's
`cross_visible` defaults from the category. New workspaces are
empty.

**Switch.** Workspace switcher dropdown at the app header (next to
the cog / manage icons). Click → write to `meta.active_workspace_id`,
refresh `AppState.active_workspace_id` and `visible_workspace_ids`,
emit a `workspace-changed` event so all subscribed views (Manage,
Splash, Cmd+J overlay) reload data.

**Rename / category change.** Same Settings panel. Renaming changes
the display name; slug stays. Category change updates the default
`cross_visible` (with a toggle alongside so the user can pick).

**Archive.** Sets `archived_at` to now. The workspace disappears
from the switcher; its data stays on disk and queries that
explicitly target the workspace still work. Unarchive available.
Hard-delete is a separate destructive action that requires
typed confirmation ("delete and remove all data" — irreversible).

**On first launch (existing v0.3.0 user):** the migration creates
the Personal workspace. The user keeps using Travis as before.
Settings → Workspaces is where they go to create a second one.

**On first launch (fresh install):** onboarding (after pack picker)
adds a workspace step — *"How do you want to organise Travis?"*
with three pre-suggested workspace categories (Work, Personal, +
maybe a third based on the pack picker). User can name them
during this step. Or they can skip and Travis creates a single
"Personal" workspace.

---

## Universal conversation thread + auto-close

Each workspace has at most one *active* conversation at a time. The
Cmd+J overlay always opens the active conversation in the active
workspace. Adding to it appends turns; the conversation stays
"open" (status = `awaiting_user` or `idle`) until either:

- 7 days pass with no new activity → the conversation auto-closes
  (status = `resolved`). Next Cmd+J in this workspace starts a
  fresh conversation.
- The user explicitly closes / resolves it.
- The user starts a new thread on a different topic via Manage →
  Threads.

Why auto-close: the LLM's context budget grows with every turn.
A "rolling forever" thread bloats irrelevantly. The 7-day window
matches a typical operational rhythm — if Taylor hasn't returned
to a topic in a week, anything new is new context.

Existing thread structure already supports this — `conversation.status`
just needs a scheduled job to flip stale ones to `resolved`. New
work: workspace_id lookup, scheduled checker.

---

## Related-past-conversations injection

When the user starts a new turn in a conversation, Travis pulls in
the **2–3 most semantically similar past conversations from the
visible workspace set** (excluding the current one), summarised
into a short reference block injected into the system prompt:

```
RELATED PAST CONVERSATIONS:
- [2026-04-15, L2E] Maria's March hours — discussed signing-sheet
  routing through PS 142.
- [2026-03-12, L2E] Coach onboarding for Carmen — agreed on a
  $48/hr starting rate.
```

Mechanism:
- Embeddings on conversation summaries (one summary per resolved
  conversation, generated at close time).
- Cosine similarity against the current turn's content embedding.
- Top 2–3 above a similarity threshold; truncate to ~200 chars
  each.
- Cross-workspace inclusion only respects the visible set.

The user sees this as Travis appearing to "remember earlier
discussions" — without us actually re-feeding raw turns. Phase 4's
knowledge graph eventually replaces this with structured recall;
for now, embeddings + summaries are the cheap fix.

---

## Intelligent workspace routing

Manual workspace management is friction the user shouldn't have to
think about. Work and personal lives intersect — Taylor's "follow up
with Maria" might be a coach (L2E) or a friend (Personal); her
"meeting tomorrow at 3" might be either. Forcing her to switch
workspaces before each capture would feel like the Slack workspace
switcher all over again — defensible UX, but heavy.

**The default: Travis infers the right workspace from the capture
itself and only asks when genuinely uncertain.**

The mechanism plugs into the journal extraction LLM that already
runs on every Cmd+J capture. The extraction tool's JSON schema
gains a routing field:

```json
{
  …
  "workspaceRouting": {
    "targetSlug": "lead-to-empower",
    "confidence": "high" | "medium" | "low",
    "rationale": "Mentioned PS 142 — that's an L2E school."
  }
}
```

The LLM has access to:
- Every visible workspace's name, category, and pack vocabulary.
- Existing entities by workspace (from the spine).
- The recent capture history (which workspace each of the last 50
  captures landed in).
- The current active workspace.

### Routing signals + behaviour

| Signal | Strength | Example |
|---|---|---|
| Existing-entity match | **High** | "Maria" + L2E has a coach named Maria |
| Pack-vocabulary hit | **Medium** | "session homework" → tutoring vocabulary |
| Recent-capture pattern | **Low** | Last 3 captures were L2E — mild bias |
| Active workspace fallback | **Tiebreaker** | All else equal, stay where you are |

Confidence drives the user-facing behaviour:

- **High** — silent route. The capture's `workspace_id`, the journal
  entry, any extracted tasks / entities / events all land in the
  routed target. UI shows a small *"Captured to L2E"* chip in the
  response.
- **Medium** — silent route, same UI chip. The distinction from High
  shows up in proactive nudges (Travis is more cautious about
  acting on Medium-routed records autonomously).
- **Low** — clarifying question in the response: *"Should this go
  in L2E or Personal? Both have a Maria."* User taps; Travis
  remembers the entity-name disambiguation going forward.

### Sensitive-workspace routing rule

Sensitive workspaces (health, therapy, legal, finance) are **never
auto-routed *into***, regardless of LLM confidence. The user must
either:

- Switch into the sensitive workspace explicitly before capturing,
  OR
- Confirm a clarifying-question prompt: *"This looks like therapy
  notes — save to Therapy, or stay in Personal?"*

This protects against an LLM hallucination dropping casual content
into a privacy-sensitive zone. Sensitive workspaces stay opt-in
for writes the same way they stay opt-in for reads (asymmetric
isolation rule).

### Auto-switching the active workspace

Routing is per-capture; active stays whatever the user last
explicitly set. But once the user captures into the same
non-active workspace **3+ times in a session**, Travis offers a
one-click *"Switch active workspace to L2E?"* in the response.
Travis never auto-switches without confirmation — the active
workspace also drives which UI tabs are foregrounded, and silently
flipping that would be disorienting.

### Cross-workspace retrieval, not just routing

The same LLM-driven inference also drives **read-side context
pulling**. When the user mentions "the meeting tomorrow", Travis
searches every visible workspace's events / calendar / notes —
not only the active one. The semantic-memory retrieval is
workspace-spanning by default; the routing decision applies the
filter only at write time. Sensitive workspaces remain isolated
per the asymmetric rule.

### Implementation note

This kicks back to the journal extraction prompt + schema. The
extraction tool's input now includes a workspace summary block
(name, category, entity_kinds, recent capture count) and the
`proposedActions` enum stays unchanged. The clarifying-questions
channel that's already in the schema becomes the surface for
low-confidence routing; the cross-workspace ambiguity detection is
a refinement on top.

---

## LLM prompts: what changes

The system prompt assembly gains two more workspace-aware bits:

**Workspace context block** (always present):

```
ACTIVE WORKSPACE: <name> (<category>)
VISIBLE WORKSPACES: <name1>, <name2>, …
```

**Cross-workspace cautions** (when relevant):

```
SENSITIVE WORKSPACE active: <name>. Do not reference data from
other workspaces in this turn unless the user explicitly mentions
it.
```

Pack prompt fragments stay the same — they're pack-shaped, not
workspace-shaped. The workspace context tells the LLM *which* of
the available pack vocabularies are likely to apply right now.

---

## Implementation slices

In dependency order. Each slice is independently shippable.

| Slice | Description | Estimate |
|---|---|---|
| 1 | Migration: `workspace` table, `workspace_id` columns + indexes, default Personal workspace | 0.5 day |
| 2 | Active + visible workspace state on AppState; load + cache; emit `workspace-changed` | 0.5 day |
| 3 | Workspace CRUD Tauri commands: `list_workspaces`, `set_active_workspace`, `create_workspace`, `update_workspace`, `archive_workspace` | 0.5 day |
| 4 | Frontend: workspace switcher in app header + Settings → Workspaces panel | 1 day |
| 5 | Pack auto-CRUD scoping: list/get filter by visible; upsert stamps active | 1 day |
| 6 | Core CRUD scoping: task, reminder, journal_entry, conversation | 1 day |
| 7 | Spine scoping: entity, relation, event reads + writes | 0.5 day |
| 8 | Conversation lifecycle: 7-day auto-close scheduled job | 0.5 day |
| 9 | Workspace context block in system prompts (journal, summary, ask, proactive) | 0.5 day |
| 10 | Related-past-conversations: summaries + embeddings + injection | 1.5 days |
| 11 | Intelligent workspace routing — LLM infers target workspace per capture; clarifying questions only on low confidence; sensitive-workspace exception; cross-workspace retrieval | 2 days |
| 12 | Sensitive-category UX: warnings, default-off cross_visible, isolation tests | 0.5 day |
| 13 | Onboarding: workspace step (after pack picker) | 0.5 day |
| 14 | Verification + bug fixes | 1–2 days |

Total: ~11–13 focused days. Still inside ROADMAP's 3–6 week
estimate (which includes discovery + interruptions).

After Phase 2 ships:
- Phase 3 (token economy / intent routing) — slips into Phase 2's
  tail per ROADMAP.
- Phase 4 (knowledge graph) — the brain. Pulled-forward question
  re-evaluates after 2 ships.

---

## Open questions for sign-off before slice 1 starts

These are calls baked into the spec above. Flag any that should be
different.

1. **Sensitive categories list.** Health, Therapy, Legal, Finance —
   default to `cross_visible = false`. Any others? (Considered:
   journal-as-diary, "personal" itself; decided against because
   the asymmetry would be confusing.)
2. **Asymmetric isolation rule.** When a sensitive workspace is
   *active*, only that workspace's data is visible — even if other
   workspaces have `cross_visible = true`. The user has to opt
   *into* contamination both ways. Is this the right default?
3. **Migration choice.** `ADD COLUMN workspace_id NOT NULL DEFAULT 1`
   without a foreign key constraint, vs full table recreation with
   FK. Going with the cheaper option. Rust paths gate writes;
   referential integrity at the SQL layer is nice-to-have.
4. **Auto-close window.** 7 days inactive → auto-close conversation.
   Configurable per user later? For v1, a hardcoded constant.
5. **Workspace name change vs slug change.** Slug is stable (used
   in cross-references, telemetry); name is renameable. Slug only
   set at create. Right?
6. **Cmd+J target workspace.** Initially targets active; the
   journal extractor's intelligent routing (see *Intelligent
   workspace routing* above) can land the capture in a different
   workspace when confident, with a UI chip showing the destination.
   Sensitive workspaces never auto-receive — they require explicit
   confirmation. After 3+ captures into a non-active workspace in a
   session, Travis offers to switch active.
7. **Sub-workspaces / projects.** Flat workspaces only in v1. If
   users want "L2E → Q1 reporting" granularity, they'd add a tag
   field on rows (Phase 4 or later). Not now.
8. **Auto-switch threshold.** 3+ consecutive captures into a
   non-active workspace trigger a one-click switch offer.
   Configurable as a constant; revisit once we see real usage.

If those calls are right, I'll start slice 1 (migration + Personal
workspace). If any need rethinking, this spec gets updated first.
