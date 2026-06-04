# Travis Changelog

## v0.12.1 — Derive a sign-in sheet from the master Google-Sheet export (2026-06-04)

Taylor's workflow: a Google Form fills a master Google Sheet with every
coach-hours entry across every school LTE serves. To get a sign-in sheet
for one principal to sign, she manually filters down to one engagement
and reformats. That filter-and-reformat step is now Travis's job.

### What ships

- **CSV + XLSX ingestion.** Drop a `.csv`, `.xlsx`, `.xls`, `.xlsm`,
  `.xlsb`, or `.ods` file into the chat overlay; Travis stores it via
  the existing document substrate. New `calamine` and `csv` crates
  handle the read.
- **`coach_hours_master` extraction prompt.** The LLM reads the
  spreadsheet text, infers column mappings (`Coach Name` / `Site` /
  `Date` / `Hours` / `Notes` — variants welcome), normalises dates to
  ISO, returns every row as structured JSON. No filtering at extract
  time — the workflow does that.
- **New workflow recipe `lte_derive_sign_in_sheet`.** Slots: master
  spreadsheet (Document), engagement (Entity), period (DateRange).
- **New action handler `DeriveSignInSheetHandler`.** Loads the
  extracted rows, filters by school name (fuzzy match against the
  engagement's school) AND date in period AND has-coach/hours,
  upserts the matched rows into `coach_hours` (dedup by coach + school
  + date), renders the printable PDF via the existing
  `render_sign_in_sheet`, registers the result as a Travis-generated
  document for round-trip.
- **Skip report.** The confirmation message says how many rows
  matched, how many were dropped for wrong school / out of period /
  missing fields — so Taylor catches data-quality issues at the
  master-sheet level.

### Example flow

```
Taylor: derive a sign-in sheet for math at PS498 for January

Travis: [asks for the master sheet if not already attached]
Taylor: [drops Hours_Master.xlsx]
Travis: read 437 rows. Engagement = math team coaching at PS 498?
Taylor: yes

Travis: 18 matching rows for math at PS 498 between 2026-01-01 and
        2026-01-31 (3 new, 15 already on file, 419 skipped — wrong
        school or out of period). PDF saved to Downloads. Want to
        open it?
```

### What's next (Path B, not in this release)

Native Google Sheets integration — Drive `.readonly` OAuth scope, thin
Sheets client, configurable sheet-id/tab/column mapping per workspace.
Removes the manual-export step. Tracked alongside WORKFLOWS_BACKLOG.md.

---

## v0.12.0 — Docs-first workflows: ingest, extract, reconcile, preview (2026-06-04)

Travis now meets Taylor where she actually works — documents (POs, work
orders, signed sheets, contracts) as first-class inputs and outputs. She
states intent ("invoice PS498 for Jan-Feb"); Travis drives the workflow,
asks for the inputs it needs (drop the PO, drop the signed sheet, or
reuse what's linked), extracts structured data, reconciles across docs,
and proposes the draft. The same engine generalises to any pack's
workflow shape — [WORKFLOWS_BACKLOG.md](./WORKFLOWS_BACKLOG.md) enumerates
the capabilities core needs for full horizontal scale.

### Slice 1 — Workflow recipes + dialogue manager

- New `workflows` module: `WorkflowDef` / `Slot` / `SlotKind` types, per-
  conversation `workflow_state` table, dialogue manager that renders
  "what's filled · what's missing · what to ask next" into the LLM prompt.
- LLM drives transitions via a new `workflowOps` field on the journal
  extraction schema — `start` / `fillSlot` / `complete` / `abandon`.
- Migration `0028_workflows.sql`.
- `PackHandle::workflows()` lets each pack contribute recipes (mirrors
  `register_actions` / `register_tools`). Framework in core, recipes in
  packs.
- LTE pack ships its first recipe: `lte_generate_invoice` (slots: school,
  engagement, period, PO, signed sheet, optional WO).

### Slice 2 — Document substrate

- New `documents` module: `document` + `document_link` tables.
- Content-addressed file storage at
  `<app_data>/documents/<hash_prefix>/<hash><ext>` — duplicate drops
  dedup automatically.
- Tauri commands: `ingest_document`, `list_documents`, `get_document`,
  `get_document_path`, `link_document`, `set_document_kind`,
  `delete_document`.
- Drag-and-drop affordance in the chat overlay — Taylor drops a PDF, it
  hashes, copies, and surfaces as a chip above the input.
- Migration `0029_documents.sql`.

### Slice 3 — Read & digest

- PDF text-layer extraction via `pdf-extract` crate (pure Rust, no
  native deps).
- Kind-specific extraction prompts for PO / WO / signed sheet / invoice /
  contract — LLM in JSON mode produces structured fields.
- Fire-and-forget background extraction on ingest; `extract_document`
  Tauri command for manual / forced re-extraction.
- New LLM tools: `read_document`, `find_documents`.

### Vision fallback for scanned PDFs

- When `pdf-extract` returns no text layer (paper sheets faxed/scanned
  back), Travis sends the PDF bytes directly to Claude via the native
  `document` content block — no PDFium / Tesseract / OS-side OCR
  needed. Claude OCRs and returns the same JSON shape as text-path
  extraction.
- New `LlmProvider::extract_pdf(bytes, prompt, max_tokens)` trait
  method. Claude implements; OpenAI and Ollama return a clear "switch
  to Claude in Settings for scanned PDFs" error.
- 30MB cap per file; bigger PDFs need page-splitting (future).

### Slice 4 — Doc-entity round-trip wiring

- Every Travis-generated PDF (invoice, work order, sign-in sheet) now
  registers as a `document` row with `source = generated_by_travis`.
- Round-trip: the PDF Travis emits is the same shape it can ingest later.
- `register_generated_document` helper in `documents::cmd` — packs call
  it after writing their PDFs.

### Slice 5 — Multi-doc reconciliation

- New `reconcile_documents` LLM tool: walks N documents' extracted JSON,
  flags PO-number mismatches, school-name mismatches, period-window
  inconsistencies, PO-vs-invoice total mismatches.
- Travis uses this when multiple document slots are filled on the active
  workflow, *before* proposing the finalize action — so inconsistencies
  surface in chat rather than in the rendered invoice.

### Slice 6 — Modify / regenerate

- `update_document_extraction` Tauri command for full-overwrite
  corrections to extracted JSON.
- `update_document_field` LLM tool for surgical edits ("change line 2
  unit price to $5031.30") via dot-path. Source PDF never modified —
  only the structured layer Travis reasons over.
- Generated PDFs round-trip via Slice 4: re-emitting after a data
  correction re-registers the new PDF automatically.

### Preview

- `preview_document` Tauri command + LLM tool open any stored document
  with the OS default viewer (Preview / Acrobat / browser / Excel) via
  the existing `tauri-plugin-opener`. Taylor says "show me that invoice",
  Travis opens the PDF.

### Extraction confirmation cards

- New `DocumentExtractCard` React component. Each attached-doc chip in
  the overlay is now a toggle — tap to expand into a card showing every
  extracted field, nested arrays (line items) rendered as sub-groups.
- Inline editing: tap any field, type, hit save. Edits dispatch
  `update_document_extraction` (full overwrite) — the source PDF is
  untouched. Re-extract button forces a fresh extractor run.
- "View source" button opens the original PDF via `preview_document`.
- Card refreshes automatically when the backend emits the
  `document-extracted` event after the background extractor finishes.
- Type coercion on save: numeric strings become numbers, "true"/"false"
  booleans, empty strings null. Conservative; preserves shape.

### Backlog

- [WORKFLOWS_BACKLOG.md](./WORKFLOWS_BACKLOG.md) — exhaustive list of
  workflow framework capabilities core needs to scale horizontally
  beyond LTE-shape (slot kinds, branching, loops, sub-workflows,
  external-action finalisers, multi-actor approval, audit trails).

### Dependencies

- `sha2 = "0.10"` — file-content hashing for documents.
- `pdf-extract = "0.7"` — text-layer extraction.

---

## v0.11.0 — BRAIN.md capabilities #2-#7 complete (2026-05-21)

Travis goes from "graph-aware operations assistant" to "partner
that thinks alongside you" — the seven BRAIN.md capabilities are
now substrate-complete. Plus a macOS keychain fix that turns
N-prompts-per-session into one.

### Capabilities shipped

- **#2 Personality.** Single source persona module
  (`src/persona/mod.rs`) — values + voice + hard-line constraints
  (Travis v1). Per-user voice corrections accumulate via the
  `update_profile_context` action (append, dedup, bound at 10).
- **#3 Learning others' personalities.** User-model background
  task derives activity patterns (active hours, capture cadence,
  question ratio) into `user_profile.derived_model_json`. Per-
  entity personality slots (contact window, style hint, top
  topics) for person entities with ≥5 mentions, persisted under
  `entity.attributes_json.personality`.
- **#4 Collaboration.** New `initiative` table. Tasks and
  conversations can tag one. `create_initiative` and
  `close_initiative` actions; journal prompt now includes an
  ACTIVE INITIATIVES block so multi-session pushes resume
  without restating context.
- **#5 Proactivity 2.0.** Observer scans the graph every
  proactive tick: mention spikes, signed sheets ready to invoice,
  stale invoice drafts. Findings append as candidate reasons in
  the proactive LLM prompt. Rhythm-aware timing reads the user
  model's peak window and biases toward silence outside it.
- **#6 Self-advocacy.** Recurring unaddressed capability gaps
  (≥3 hits in 14 days) surface as ONE Travis-voice ask through
  the clarifying-questions pipe, with a 7-day cooldown after
  surface. No pestering; soft anti-pestering thresholds.
- **#7 Wellbeing.** Affect-signal extraction (tone + themes)
  per journal capture. Recurring-theme observer detects topics
  the user keeps returning to with concerned/drained tone.
  Persona gains wellbeing constraints (never therapeutic, never
  wellness performance, push back once on self-harming asks).
  Affect data **never** appears in exports.

### Fixes

- **macOS keychain prompt per LLM call.** `secrets.rs` now
  caches API keys in a process-wide OnceLock map. First call
  hits the keychain; every subsequent call reads from memory.
  Same threat model (secret already in process memory when
  used); meaningful UX win on macOS where keychain access
  triggered a password modal per request.

### Privacy posture

Wellbeing affect signals are the most sensitive bytes Travis
generates. The export logic excludes the `affect_signal` table
explicitly. They're not in any pack-queryable surface. Per
BRAIN.md's surveillance-creep failure mode: descriptive
observations only, no aggregation, no transmission, no
prescriptive labels.

### Migrations

`0024_user_model.sql`, `0025_advocacy_cooldown.sql`,
`0026_initiatives.sql`, `0027_affect_signals.sql`. All
additive; existing data unchanged.

## v0.10.0 — Phase 4.5 cognition complete (2026-05-21)

The full BRAIN.md Phase 4.5 build list lands. Travis now thinks
alongside the user with composed graph queries, persisted
reasoning conclusions, multi-turn working memory, recency-aware
ranking, and graded confidence — instead of recomposing intent
from scratch every turn. Substrate work that unlocks the rest of
the cognition roadmap (personality, learning others, proactivity,
self-advocacy, wellbeing).

### Items shipped (BRAIN.md ranking order)

1. **Embedding-based entity retrieval** — `retrieve_semantic`
   cosines against the existing entity index for fuzzy/pronoun-
   shaped queries the exact-name path missed.
2. **Structured fact extraction** — `entityFacts` bucket on the
   journal extractor; each fact persists as a typed claim.
3. **Memory consolidation tick** — background pass every 30 min
   summarises stale entities into stable claim rows so retrieval
   doesn't get noisier over time.
4. **Multi-hop traversal** — `graph_neighbors` LLM tool walks
   `mentioned_with` edges up to 3 hops out with strength ranking.
5. **Confidence in answers** — `ConfidenceBand` (high/medium/low)
   annotated on every GraphHit so Travis can quote certainty
   rather than asserting flat.
6. **Working memory cache** — in-process per-conversation
   hypothesis store with 30-min TTL; multi-turn reasoning
   compounds rather than restarting.
7. **Persisted claims layer** — new `claim` table with
   confidence + source attribution; contradicting claims kept
   side-by-side flagged `contested` rather than silently
   overwritten.
8. **Active forgetting / decay** — 30-day half-life multiplier
   on semantic ranking; ancient strong matches no longer outrank
   recent weak ones.
9. **Per-entity recall tooltip** — capture chips hover-expand
   into a popover showing what Travis remembers about that
   entity (mentions, claims, recent snippets, related entities).
10. **Inference helpers driving conversation** — refinement
    candidates piped into the in-thread clarifying-question
    surface; `*:unknown` entities with 5+ mentions trigger one
    focused question with role suggestions inline.

### Migration

Core migration `0023_claims_and_facts.sql` creates the `claim`
table and adds `entity.last_consolidated_at`. Additive + safe;
existing data unchanged.

## v0.9.0 — Chat-first operations + generic pack bridge (2026-05-20)

The COO can drive the entire LTE billing chain through conversation
without opening a Manage tab. Travis decides per-call whether to
silent-create or confirm-card, asks one focused question per gap
with clickable options rather than typed input, ranks ambiguous
matches by recency + activity, and resumes mid-flow on the next
turn. Pack v0.6.0.

### Highlights

- **Six new LLM-callable handlers** for the LTE chain. Schools and
  coaches are observational (silent creates via tools); contracts,
  engagements, work orders, purchase orders, and coach hours go
  through action confirmation cards (commit to relationships /
  billable artifacts).
- **Four read-only search tools** with ranking + rationale:
  `lte_find_or_create_school`, `lte_find_contract`,
  `lte_find_engagement`, `lte_summarize_context`. Each returns
  ranked candidates so the LLM presents the top match or asks
  between 2-3.
- **Generic pack bridge.** `pack_introspect` lists every enabled
  pack's tables + field schemas; `pack_query` reads rows from any
  table with safe filters (`eq`/`ne`/`lt`/`lte`/`gt`/`gte`/`like`/
  `ilike`/`in`/`isNull`/`isNotNull`), workspace-clamped
  automatically. Field names validated; no SQL injection. Unblocks
  every "Travis, look up …" question across any current or future
  pack.
- **Selection chip UX.** Chat reply parser detects `⊙ ⊕ ⊡ 📅`
  markers. Single-select chips submit on click; add-new chips
  styled subtly differently; multi-select accumulates with a "Send
  selection (N)" button; date chips open the native OS picker and
  submit the chosen ISO date. Pure markdown convention — zero
  schema changes; Travis just emits markers in its reply text.
- **Prompt fragment teaches the loop.** Confirmation policy,
  ambiguity handling, selection markers, resumption cues, and
  bias-toward-action are all spelled out so the LLM doesn't need
  to re-derive intent each turn.

### What this unlocks

> **Taylor:** Create an invoice for PS95.
>
> **Travis** (silently creates PS95, finds three active contracts):
> Saved PS95 as a new school. No engagement yet, and three contracts
> could fit:
> - ⊙ QR179CF — Systemwide Services (38% burn)
> - ⊙ NYCPS HS Math — Supt. White pursuit
> - ⊙ NYCPS Tutoring
> - ⊕ New contract
>
> **Taylor** *(clicks QR179CF)*
>
> **Travis:** Proposing engagement "PS95 — 26-27" under QR179CF.
> Stage assessment. *(Confirm card.)*
>
> *(After confirm…)*
>
> What scope items? You can paste from the WO or pick from the
> catalog: ⊡ Data Coaching, ⊡ Leadership Coaching, ⊡ Instructional
> Coaching, ⊡ School Assessment …

End-to-end without a click into Manage.

## v0.8.0 — LTE contracts: first-class master agreements (2026-05-20)

Promotes contract tracking from a free-text field to a typed table.
The "don't abstract on n=1" guardrail no longer applies — the COO
runs multiple master agreements in parallel, and the spec's deferred
follow-up (`LTE_INVOICING_SPEC.md` §11) ships here. Pack v0.5.0.

### Highlights

- **`contract` table** — ref (unique per workspace), name,
  counterparty, parent_solicitation, term_start/end, ceiling_cents,
  signed_at, status (`draft`/`active`/`expired`/`terminated`/
  `archived`), notes, pdf_path. Primary tab in Manage.
- **Soft FK on the chain.** `engagement.contract_id`,
  `work_order.contract_id`, and `purchase_order.contract_id`
  added. `ON DELETE SET NULL` — deleting a contract leaves its
  history visible rather than cascading away invoices.
- **Backfill, no data loss.** Migration 0004 scans existing
  `engagement.contract_ref` strings, inserts one contract row per
  distinct ref (workspace-scoped), then sets the FKs by string
  match. `contract_ref` stays as a display field for legacy.
- **Two new alerts.** `contract_near_ceiling` (Money): active
  contracts where invoiced ≥ 90% of `ceiling_cents` (skips
  ceiling=0). `contract_expiring_soon` (Action): active contracts
  with `term_end` ≤ 60 days out. Surfaces in Splash like every
  other LTE alert.

### What does *not* break

- Existing `engagement.contract_ref` strings continue to work and
  render. The new FK is additive — set the contract on the
  engagement and downstream WO/PO inherit through the chain.
- `propose_program_invoice_draft` and all PDF generators are
  unchanged. They didn't reference contracts directly; the FK
  routes through the engagement they already use.
- Spec §11 in `LTE_INVOICING_SPEC.md` is now superseded — leaving
  the line as a historical note since the rationale shaped the
  v0.4.0 schema.

## v0.7.0 — LTE invoicing: document layer + validators + PDFs (2026-05-20)

Closes the post-sale half of the Lead to Empower pack. v0.6.0
modeled what LTE sells (catalog) and how it delivers (the 3 A's);
this release handles **turning delivered work into a paid invoice**
through the NYC DOE four-document chain — Work Order → Purchase
Order → Sign-in Sheet → Invoice → Polaris submission.

Driven by the COO's recorded walkthrough and the PS/MS 498 sample
documents (PO `WR260363316`, invoice `LTE2064981`). Spec:
`LTE_INVOICING_SPEC.md`. Pack version `0.4.0`.

### Highlights

- **The document layer.** Two new typed tables — `work_order`
  (vendor-issued, school-countersigned) and `purchase_order`
  (DOE-issued, received) — both linked to engagements and pulling
  line items from `engagement_module` (no schema duplication).
  `invoice_line` table for multi-module invoices with snapshot
  qty + unit_price so post-send scope edits don't rewrite history.
  `engagement_module.qty` (NEW) captures billable units per module.
- **Three deterministic validators at draft→sent.** Catalog/agreed
  unit-price match (catches the PS 498 Leadership-billed-at-
  Instructional-rate error); per-line arithmetic (catches the
  qty × price ≠ subtotal mismatch); period-within-PO-window. Refuses
  the transition with a *fix-shaped* message, not a generic 400.
- **Two new alerts.** `overlapping_invoice_period` — engagement-
  scoped (so multi-engagement schools don't false-positive), covers
  same-date double-billing, period overlap, and outside-PO-window
  in one cast. Solves Jacob-goes-from-memory. `wo_date_outside_
  school_year` catches the 02/15/2025-vs-2026 typo.
- **Three PDF generators.** Work Order in NYC DOE format,
  Sign-in Sheet in LTE table layout (replaces Taylor's Excel
  cleanup dance entirely), Invoice in LTE letterhead (replaces
  Canva). All write to Downloads. All branding parameterised from
  `company_profile` — a sibling consulting firm swaps the row and
  reuses every template.
- **Settings → Company.** Single-row edit form for company_profile.
  Edit once; every WO / sign-in sheet / invoice picks up the new
  values automatically.
- **`propose_program_invoice_draft` action.** Builds multi-line
  invoices from an engagement + period: resolves engagement,
  picks the covering PO, computes remaining billable qty per
  scope item (planned − already billed), formats the date list
  per module from coach_hours, inserts the invoice + invoice_line
  rows. The "draft this month's invoices" handler.
- **`lte_validate_invoice` LLM tool.** Read-only — runs the same
  draft→sent validators against a draft and reports the verdict
  conversationally. Travis can use it before suggesting send.

### Migration

Pack-owned migration `0003_invoicing.sql`. Creates 4 tables,
ALTERs 3 existing (engagement_module, invoice, coach_hours), all
additive with safe defaults. Pre-existing data stays intact.
First-install seeds the `company_profile` row with LTE defaults
(verbatim from the MTAC #R1179 application package); upgrades
keep any existing row via `INSERT OR IGNORE`.

## v0.6.0 — LTE program delivery: the 3 A's, catalog & quotes (2026-05-19)

The Lead to Empower pack modeled only the billing spine (coaches,
hours, signing sheets, invoices) — *money out the door*, with no
representation of what LTE sells or how it delivers it. Digesting the
full NYC DOE MTAC #R1179 application supplied the missing half. This
release encodes it.

### Highlights

- **The "3 A's" state machine.** New `engagement` table — one run per
  school — moving Assessment → Action Planning → Accountable →
  closed, with the signed metrics agreement as the gate into
  delivery. Stage advances from conversation (track-everything;
  Travis proposes the transition, doesn't make you fill a form).
- **The 21-line catalog.** New `catalog_module` table seeded verbatim
  from Appendix F — both pillars (Leadership Development; Data-Driven
  Decision-Making & Teacher Effectiveness), every line with its
  price, session shape, and participant envelope. Plus `assessment`
  (the diagnostic), `engagement_module` (scope of work), and
  `accountability_review` (the ~3/year metrics checkpoints).
- **Quote / margin tool.** `lte_quote_margin` — a read-only LLM tool
  that computes the Appendix G cost model (labor = sessions × hours ×
  instructors × $100/hr, + G&A + materials + rental; margin = list −
  cost) for any module with staffing/price overrides. Answers
  "what's our margin if we run Developing Data-Driven Practices for
  40 kids with one facilitator?" in conversation. Pinned to the
  source numbers by unit tests (Authentic Leadership → $231 / 9.0%).
  New `quote` table persists pre-sale scenarios for bid comparison.
- **Operational alerts for the program side.** Three additions to
  Splash: engagements delivering without a signed metrics agreement,
  active engagements with no accountability review on record (money —
  unreviewed metrics loses renewals), and engagements stuck in
  Assessment with no diagnostic recorded.
- **Billing bridge.** `coach_hours.engagement_id` ties delivered
  hours back to the engagement they served (forward column; typed UI
  wiring in a later slice).

### Notes

- First pack-owned migrations for `lead_to_empower` (the billing
  spine stays in core's `0003_domain.sql` for history continuity).
  Pack version → 0.3.0.
- Specs: `LTE_PACK_SPEC.md`, `LTE_QUOTE_SPEC.md`. Persisted-quote
  stored-computed columns are deferred to a custom quote UI slice
  (documented in the quote spec); the tool is the compute engine
  meanwhile.

## v0.5.1 — Export your data (2026-05-09)

Adds a transparency hatch: a Settings → **Export** section that
dumps every user-table row in the current instance to a JSON file
in the user's Downloads folder. Built for the consented
pre-commercialization observation arrangement — Travis is still a
black box; the export is how an operator inspects what's been
captured.

### Highlights

- **Settings → Export.** Single button writes
  `travis-export-<timestamp>.json` to Downloads (or
  `<timestamp>-full.json` when sensitive workspaces are included).
  Reveal-in-folder affordance for one-click attach-to-email.
- **Privacy posture.** Sensitive workspaces (health/therapy/legal/
  finance) are excluded by default; explicit checkbox to opt in.
  OAuth tokens (`access_token`, `refresh_token`,
  `credentials_json`) always redacted regardless. Embedding blobs
  replaced with byte-length sentinels — file stays inspectable
  without leaking 3 KB vectors per row.
- **Transparency surface.** The result panel shows the file path,
  size, total row count, per-table breakdown (collapsible), and
  any redactions applied — the user sees exactly what's in the
  file before sharing.
- **Backend dynamic.** Walks every user table via `PRAGMA
  table_info`, encodes columns by declared type (int/real/text/
  blob/bool). Filters by workspace when the table has a
  `workspace_id` column. Adds new tables automatically as packs
  ship migrations; no per-table maintenance.

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
