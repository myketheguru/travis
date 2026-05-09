# Knowledge Graph — Phase 4 design

> **Status (2026-05-09):** design draft. No code yet beyond what
> already exists in `src-tauri/src/spine/`. Slices below assume the
> spine + workspace + intent-router work from Phases 1–3 has shipped.

## Why this phase

Right now Travis is a notes app with extras. Captures land in
`journal_entry` as text plus a few extracted scalars (task titles,
reminder strings, entity *names*). Entities are tracked as a flat
`(kind, normalized_name)` index with a mention counter — useful for
the "known coaches" prompt blurb, but not enough to *reason* about
the things the user keeps mentioning.

After Phase 4, the graph **is** the model. Every observed signal —
named people, places, organisations, departments, recurring topics —
becomes a typed node with a mention timeline, attribute payload,
embedding, and edges to related nodes. Pack tables (coach, school,
student, invoice…) project into the same graph so the typed-CRUD
world and the ambient-capture world share one identity layer.

The graph is also the moat. Twelve months of graph history is the
thing a user cannot get from a competitor without rebuilding their
context. Phase 6 (cloud sync) is what they'll pay to never lose; this
phase is what makes it worth paying for.

---

## Posture: track everything, ask only to refine

The default behaviour for every observable signal is to **record it
silently**. Travis never asks permission to track. Every mention
upserts the entity, every journal turn records mention events, every
recurring pattern accumulates without the user being prompted.

Clarifying questions exist — but only to **enrich** what's already
recorded. The classic "you've mentioned Maria 4 times" surface is a
notification of an interesting pattern Travis already saw, not a
gate on tracking. The follow-up question, when one is warranted, is
about *categorisation* — "is this Maria the L2E coach or a different
one?" — and only fires when the answer would meaningfully change
how Travis retrieves or routes.

This is the ambient-colleague posture: someone who's already paying
attention, not someone asking before noticing.

---

## What's already in place

The spine tables (added pre–Phase 4) cover the structural skeleton:

```
entity (id, kind, normalized_name, display_name,
        pack_slug, mentions_count, first_seen, last_seen,
        attributes_json, workspace_id)
relation (id, from_entity, to_entity, kind,
          pack_slug, attributes_json,
          [workspace_id from migration 0020])
event (id, entity_id, kind, pack_slug, occurred_at,
       attributes_json, workspace_id)
```

Existing helpers:

- `spine::entity::upsert` — idempotent on (kind, normalized_name);
  used by every primary pack table's CRUD path so coach/school/
  student rows project into the graph today.
- `spine::event::record` — append-only timeline; coach_hours,
  invoice transitions, sessions all write here.
- `spine::relation::link` — typed edges; not yet used in production.
- `identity::record_mention` — fast-path mention upsert called from
  the journal extraction path for every pack-declared entity kind
  (coach, school, dept, tutor, student).
- `identity::build_profile_blurb` — surfaces the top 5 mentioned
  entities per kind into the system prompt.

So mentions are already captured and counted. What's missing is
everything that turns this skeleton into a **memory**: ambient
entity discovery beyond pack-declared kinds, mention-as-event
recording, embeddings on entity nodes, graph-aware retrieval,
inference loops, and the UI to see + edit any of it.

---

## Data model — what changes

### entity table extensions

Add columns via migration:

- `embedding_vector BLOB` — fastembed embedding of
  `display_name + attributes_json + recent mentions context`. NULL
  until the indexer runs.
- `embedding_indexed_at TEXT` — debounce; re-embed only when
  attributes change or the entity has accumulated N new mentions.
- `confidence REAL DEFAULT 1.0` — how sure Travis is about the
  entity's `kind`. Pack-table projections write 1.0 (the row exists,
  the kind is certain). Ambient extractions start at 0.5 and rise
  with mentions. Drives the disambiguation UI.
- `tags TEXT` — free-form, comma-separated tags assigned by the user
  via clarifying-question answers ("priority", "billable",
  "mentor", "vendor", whatever the user volunteers). Searchable.
- `archived_at TEXT` — soft delete; archived entities don't appear
  in retrieval but mentions in past journal entries stay linked.

### Generic entity kinds

Today's `entity.kind` values are pack-declared (coach, school,
student, tutor, dept, invoice). Phase 4 adds three generic kinds for
ambient capture:

- `person:unknown` — a name Travis saw in a journal but couldn't
  attribute to a pack-declared role. Becomes `person:coach`,
  `person:friend`, etc. when the user clarifies or pack code
  upgrades them.
- `place:unknown` — proper-noun locations (PS 142 before any pack
  knows it's a school).
- `org:unknown` — agencies, companies, departments before
  classification.

Generic kinds let ambient capture run with **zero** pack
configuration. Packs can later "claim" an unknown by upgrading its
kind.

### mention event kind

Every journal turn that names entities writes `event` rows with
`kind = "mentioned"`, `entity_id = <id>`, `attributes_json =
{ "journal_entry_id": <id>, "snippet": "..." }`. This gives every
entity a complete mention timeline drawn from existing data —
useful for "show me everything you remember about Maria", which is
exactly the cross-pack question the spine was built for but doesn't
yet answer in production.

### relation usage

Currently the `relation` table has helpers but no production caller.
Phase 4 starts populating it from journal extraction:

- `mentioned_with` — when two entities appear in the same journal
  entry, an edge between them. Carries `attributes_json =
  { "journal_entry_id": id, "co_mention_count": n }` updated on
  repeat co-mentions.
- `works_at`, `parent_of`, `bills`, `reports_to`, etc. — typed
  edges proposed by the LLM during extraction. Pack code may also
  emit these from typed CRUD paths (e.g. invoice `bills` school).

Relations stay best-effort: if extraction is wrong, the user can
delete the edge from the entity detail UI.

---

## How ambient capture works

### Per-turn flow (extends existing journal_ingest)

1. **Heuristic + intent classifier** (already shipped — Phase 3):
   captures vs queries.
2. **LLM extraction** (already shipped, but the schema grows):
   - Existing buckets: `tasks`, `entities` (pack-kinded),
     `reminders`, `completedTaskIds`, `clarifyingQuestions`,
     `capabilityGaps`, `proposedActions`, `workspaceRouting`.
   - **New buckets:**
     - `genericEntities` — `[ { name, kind: "person"|"place"|"org",
       contextSnippet?, confidence: "high"|"medium"|"low" } ]`.
       Catches names that don't match any pack-declared kind.
     - `relations` — `[ { fromName, toName, kind } ]`. Co-mentions
       and typed edges.
3. **Persist:**
   - Pack-kinded entities → existing `record_mention`.
   - Generic entities → `record_mention` against the matching
     `*:unknown` kind, with confidence reflected in attributes.
   - Every entity touched in step 2: append a `mentioned` event
     linked to `journal_entry_id`.
   - Co-mentions: upsert `mentioned_with` edges, increment
     co-mention counts.
4. **Embedding refresh** (debounced, async):
   - Entities whose attributes/mentions changed since
     `embedding_indexed_at > 7 days ago` OR mention count crossed a
     threshold (10, 50, 200) get queued for re-embedding.
   - The fastembed loader is already warm from the journal-entry
     indexing path; piggy-back.

### What never blocks the user

Nothing in step 3 or 4 sits between the user's Cmd-J and the LLM
response. All entity persistence runs after the response is
returned, in the same transaction shape as today's
`identity::record_mention`. Embedding refresh is fully async —
worst case, a few seconds of staleness on first use of a freshly
mentioned entity.

---

## Graph-aware retrieval

Currently `memory::retrieve` is a pure embedding scan over journal
text. Phase 4 adds a parallel **entity-mention retrieval** path
that the system prompt prefers when the LLM looks like it's
answering a question about a known entity.

### When the entity path fires

Trigger conditions in `memory::retrieve`:

- The user's input mentions a name that resolves to an existing
  entity (with `confidence >= 0.5`), OR
- Intent classifier classified the input as `Query` AND the input
  contains a proper noun.

### What it returns

For each matched entity:
- The 5 most recent `event` rows (across all kinds).
- The 3 most recent `journal_entry` snippets that mentioned it
  (from the `mentioned` events).
- 1-2 strongly-related entities by relation count (so "Maria"
  retrieval surfaces "PS 142" if they co-mention often).

This gets formatted into the user message as a `RELEVANT MEMORY`
block alongside today's text-similarity hits — the LLM picks what
to use.

### Workspace scoping

Same rule as today: graph queries respect `visible_ids`. Sensitive
workspaces' entities never bleed into non-sensitive contexts. The
`workspace_id` on every spine table makes this enforceable at SQL
level.

### Cost

Entity retrieval is a few indexed lookups on `entity.id` /
`event.entity_id` — cheap compared to a full-table embedding scan.
Net effect: faster *and* more accurate answers for entity-anchored
questions.

---

## Inference loops

These run on a quiet background tick (every 30 minutes) and write
their findings as proactive nudges into the existing nudge thread —
they never block capture.

### 1. Recurring-mention surface

**Signal:** an entity with `kind = "person:unknown"` (or any
`*:unknown`) crosses 4 mentions in a 14-day rolling window.

**Action:** propose a clarifying question via the existing nudge
thread: *"You've been talking about Maria — is she a coach, a
parent, a friend? (Skip if not relevant.)"* User's answer upgrades
the entity's `kind` and `confidence`. Skip = no follow-up for 30
days.

This is a **categorisation refinement**, not a tracking gate. The
entity has been tracked since mention #1.

### 2. Co-mention pattern

**Signal:** two entities appear in 3+ journal entries together
within a month, with no existing relation edge.

**Action:** propose a labelled edge: *"You always mention Maria and
PS 142 together — should I note 'Maria works at PS 142'?"* (one tap
to confirm).

### 3. Stale-entity flag

**Signal:** an entity with mentions_count > 10 has zero mentions in
the last 60 days.

**Action:** quiet — no nudge. The entity stays in the graph but
drops out of the top-5 prompt blurb. User can archive from the
detail view if it's truly gone.

### 4. Conflict detection

**Signal:** two entities with the same `normalized_name` but
different `kind` (e.g. `person:friend` and `person:coach`) get
mentioned in the same week.

**Action:** propose merge OR confirm distinction. *"Maria appears as
both a coach and a personal contact — same person, or different
Marias?"*

These four loops are the v1 inference layer. They're all "observe
silently, surface only when the answer would change retrieval".

---

## Pack integration

Existing pack-table → spine projection (one-way: pack rows write to
`entity`) keeps working. Phase 4 adds a **back-reference**: the
`entity` row carries a `pack_slug` already; we add a new column
`pack_table_id` so a `coach` entity row knows which `coach.id`
created it.

Why: when ambient extraction sees "Maria" and an existing pack
entity matches by normalized name, the mention attaches to the
pack-kinded entity (not creating a duplicate `person:unknown`).
This is the dedup that turns ambient + typed into one identity
layer.

Pack manifests grow an optional `entity_taxonomy` field declaring
what subkinds the pack contributes:

```rust
fn entity_taxonomy(&self) -> &'static [(&'static str, &'static str)] {
    &[("coach", "person:coach"), ("school", "place:school")]
}
```

Generic-kind ambient extractions get re-classified into pack-kind
when the user confirms (Loop 1) or when an exact pack-table row
match exists.

---

## UI surfaces

### People / Places tabs

A new top-level Manage area aggregates entities across packs by
high-level kind:

- **People** — every `person:*` entity. List view sortable by
  mention count, last-seen, name. Click → entity detail.
- **Places** — every `place:*` entity. Same shape.
- **Organisations** — every `org:*`.

These coexist with the existing per-pack tabs (Coaches, Schools,
…) — those stay typed and pack-specific. The Knowledge tabs are
the cross-pack ambient view.

### Entity detail

For any entity:
- Header: display name, kind, confidence, tags.
- Mention timeline: `event` rows with `kind="mentioned"` rendered
  as a calendar/list with one-click jump to the source journal
  entry.
- Pack-projected fields: when pack_table_id is set, the typed
  fields show up (rate_cents for a coach, etc.), editable inline.
- Related entities: outgoing/incoming `relation` rows.
- Merge / split / archive controls.

### Subtle indicators

- The Splash screen's "metric to capitalise on" gains an entity
  surface: *"Maria — 6 mentions this week, hours not yet logged"*.
- The journal capture overlay shows a small chip when extraction
  resolves a mention to a known entity: *"→ Maria (coach)"*. One
  click opens the detail.

---

## Migrations

### `0021_graph_extensions.sql`

```sql
-- Embedding payload + freshness tracking
ALTER TABLE entity ADD COLUMN embedding_vector BLOB;
ALTER TABLE entity ADD COLUMN embedding_indexed_at TEXT;
ALTER TABLE entity ADD COLUMN confidence REAL NOT NULL DEFAULT 1.0;
ALTER TABLE entity ADD COLUMN tags TEXT;
ALTER TABLE entity ADD COLUMN archived_at TEXT;
ALTER TABLE entity ADD COLUMN pack_table_id INTEGER;

CREATE INDEX idx_entity_archived ON entity(archived_at);
CREATE INDEX idx_entity_pack_table ON entity(pack_slug, pack_table_id);
CREATE INDEX idx_entity_kind_workspace ON entity(kind, workspace_id);

-- Mention events get an index since the entity-detail timeline
-- pulls them constantly.
CREATE INDEX IF NOT EXISTS idx_event_entity ON event(entity_id, occurred_at DESC);

-- Co-mention edge tracking; relation already has workspace_id.
CREATE INDEX IF NOT EXISTS idx_relation_from ON relation(from_entity, kind);
CREATE INDEX IF NOT EXISTS idx_relation_to ON relation(to_entity, kind);
```

### `0022_relation_workspace_helper.sql` (if needed)

Audit shows `relation.link` doesn't yet bind workspace_id (relies
on the migration default of 1). Slice 2 of Phase 4 fixes that
helper signature; no migration needed since the column already
exists from `0020_workspaces.sql`.

---

## Slicing — proposed cut into shippable chunks

| # | Slice | Estimate |
|---|---|---|
| 1 | Graph extension migration + struct fields | 0.5 day |
| 2 | Workspace-aware `relation::link` + audit existing callers | 0.5 day |
| 3 | `mentioned` event recording from journal flow | 1 day |
| 4 | Generic `person/place/org:unknown` extraction in journal schema | 1 day |
| 5 | Co-mention edge writes + dedup | 1 day |
| 6 | `pack_table_id` back-reference + dedup against pack rows | 1 day |
| 7 | Entity embedding pipeline (debounced, async) | 1.5 days |
| 8 | Graph-aware retrieval in `memory::retrieve` | 2 days |
| 9 | Inference Loop 1: recurring-mention categorisation nudge | 1 day |
| 10 | Inference Loop 2: co-mention edge proposal | 0.5 day |
| 11 | Inference Loop 4: conflict detection + merge prompt | 1 day |
| 12 | UI: People / Places / Orgs Manage tabs (auto-CRUD reuse) | 1.5 days |
| 13 | UI: Entity detail page with timeline + relations + merge | 2 days |
| 14 | Splash entity surface + capture chip | 1 day |
| 15 | Verification + bug fixes | 1–2 days |

Total: ~16–17 focused days. Roadmap says "2–3 months" for Phase 4 —
the gap is iteration on the inference loops once real graph data
exists; thresholds and prompts will need tuning that's hard to
predict from a design doc.

---

## Open questions

1. **Embedding model for entities.** Reuse fastembed bge-small? Same
   model as journal embeddings keeps the cosine space coherent for
   cross-source retrieval; fine.

2. **Generic-kind upgrade trigger.** Today's plan: user answer to
   the categorisation nudge, OR exact match on a pack table row.
   Should we also auto-upgrade when LLM extraction confidently
   classifies during a later journal turn? Probably yes — start
   with `confidence >= 0.8` from the LLM as the auto-upgrade gate.

3. **Privacy on the merge UI.** When two entities with the same
   normalized name exist in different workspaces, never offer merge
   across workspaces — that would break isolation. The conflict UI
   only fires within a single workspace.

4. **Archive vs delete.** Soft-archive is clearly right for the v1.
   Hard-delete (with cascade to events / relations) would be a
   destructive button-mash that needs a typed-confirmation flow —
   defer to a follow-up.

5. **Pack-projection of relations.** Should pack typed CRUD start
   emitting relations (e.g. invoice → school as a `bills` edge) or
   stay extraction-driven for the v1? Lean: extraction-driven for
   v1, pack-driven as a Phase 4.5 polish if pack authors want
   structured edges.

---

## Out of scope for Phase 4

These are deliberately deferred:

- **Time-aware decay.** Old mentions don't lose weight in retrieval
  ranking yet. The recency signal in memory::retrieve already
  exists; an entity-level decay could be Phase 4.5.
- **Cross-workspace inference.** Patterns that span workspaces
  (e.g. "this person appears in both Personal and L2E") are
  detectable but won't surface — sensitive isolation rule wins.
- **Entity attributes editor.** The detail view edits pack-projected
  fields; ambient-discovered attributes (free text in
  `attributes_json`) stay read-only until users ask for it.
- **Public/shared graph.** All graph data stays workspace-scoped
  per device. Cross-device sync arrives with Phase 6.
- **Automatic relation pruning.** False positives in co-mention
  edges accumulate; pruning is a Phase 4.5 problem.

---

## Why now (not later)

Phase 4 unlocks the queries Travis already implies it can answer
("when did I last meet Maria") but doesn't actually answer well
today. Every phase after this — cloud sync, mobile, voice — sits on
top of a graph or has to invent one. Building it now means Phase 6
syncs a known-good model, mobile retrieval queries one well-typed
endpoint, voice answers with a coherent memory, and pack authors
build against one identity layer instead of bolting names into
text.

It's also the moat. The thing a year-deep Travis user has that no
one else can replicate.
