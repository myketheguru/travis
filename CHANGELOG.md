# Travis Changelog

## v0.16.1 — v0.16.0 CI fix (2026-06-08)

The v0.16.0 commit had a TypeScript discriminated-union access bug
in `AskTab.tsx`'s step subscription handler (`event.conversationId`
is only present on the `started` variant of `StepEvent`). CI caught
it; this patch fixes it and re-ships the full v0.16.0 content
under v0.16.1.

All v0.16.0 content (case substrate going live, live step events
fix, Pyodide warmup bump) ships here.

## v0.16.0 — Case substrate goes live (2026-06-08)

v0.14 added the `travis_case` and `case_artifact` tables but they
were never read. v0.16.0 turns them on. Multi-day workflows now
auto-open a case; the LLM sees a continuity context block every
turn for the case's lifetime; the chat surfaces a header strip with
a switcher and close button.

### Auto-detection

When a chat doesn't already have a case linked, `journal_ingest`
evaluates three triggers before the agent loop:
- An active workflow on this conversation
- Multi-doc upload (≥2 `doc#N` markers in the user turn)
- Conversation depth ≥3

When **any two** fire, Travis auto-opens a case (named after the
active workflow's recipe, or the user's first-turn note) and links
the conversation to it. Cases hold across tab switches, restarts,
and multi-day gaps.

### `== ACTIVE CASE ==` context block

The user message every turn now carries a tight case-context block:
case name + id, started/last-activity timestamps, summary if any,
plus a directive: "this conversation is part of a multi-session
case; reference prior decisions, build on past artifacts, don't
restart from scratch." The LLM sees this every turn for the case's
lifetime — the continuity surface that lets Travis resume coherently.

### UI: case header strip + switcher

Above the chat transcript, a slim purple-tinted strip renders when
the conversation has a linked case:

```
case  PS 89 invoice close-out · 3 conversations · started 06-05    [switch] [close case]
```

Click `switch` to see a popover of other open cases — picking one
routes you to that case's most recent conversation. Click
`close case` to mark it closed (still reachable from the switcher,
just no longer "active").

### New backend helpers (cases::db)

- `find_by_conversation(conv_id) -> Option<Case>` — scans
  `conversation_ids_json` to find the case holding a given turn.
- `link_conversation(case_id, conv_id)` — idempotent append.
- `set_summary(case_id, summary)` — background-writer hook.
- `touch(case_id)` — bump `last_activity_at` cheaply each turn.

### New Tauri commands

- `case_for_conversation(conversation_id) -> Option<Case>` —
  what the frontend strip consumes.

### Bundled bug fixes

User-reported during v0.16.0 development — small enough to land in
the same slice.

**Live step events.** The frontend's step-event subscription was
gated on `activeConversationId` being non-null. When the user sent
their first message in a fresh chat, the id was null (backend
assigns it mid-call), so every step event the backend emitted
during that turn was filtered out. Steps only appeared after the
chat reloaded from the DB. Fix: subscribe persistently on mount;
use a ref to filter against the current id without re-subscribing.

**Pyodide warmup timeout.** Cold-start Pyodide load was exceeding
the 30-second warmup wait, causing repeated "interpreter not ready"
errors during the first `run_python` call. Each failure burned a
manager-pass iteration on the retry loop. Bumped to 90 seconds
which leaves comfortable headroom for cold installs.

### Sequencing (v0.16.1 → v0.16.3)

Queued for follow-up slices, independent + non-blocking:

- **v0.16.1** — Typed-edge memory graph (#174) + decay policy (#178).
  New `memory_edge` table with AutoMem's 11 typed edges
  (`LEADS_TO`, `EVOLVED_INTO`, `DERIVED_FROM`, `CONTRADICTS`, etc.).
  Cross-document reconciliation gets a real primitive.
- **v0.16.2** — Event log substrate (#172). `conversation_message`
  becomes a projection of an `event` log. Enables branching /
  time-travel / `reasoning-only MessageEvent` rendering.
- **v0.16.3** — Condenser pattern (#173). Depends on event log.

## v0.15.4 — Error observability + extended-thinking bug fix (2026-06-08)

Two things bundled: the underlying bug behind v0.15.2/v0.15.3's
recurring "Travis hit an error while thinking through that turn"
messages, and the observability infrastructure to track future
errors instead of staring at them blind.

### The bug

v0.15.2 enabled Anthropic's extended-thinking parameter on every
primary agent-loop call (`thinking_budget: 4000`). Anthropic's API
has a constraint we missed: when extended thinking is enabled,
`tool_choice` can be `auto` only — `Specific(...)` and `Required`
both get rejected with a 400. The agent loop forces
`ToolChoice::Specific(extraction_name)` on the LAST iteration as
its safety net, so every conversation with thinking enabled was
one bad turn away from this error.

Fix in `src-tauri/src/llm/claude.rs`: when `thinking_budget` is set,
coerce `Specific` and `Required` tool_choice down to `Auto`. The
system prompt + retry directive still steer the model toward the
right tool; we just don't *force* it at the API layer when the
constraint forbids forcing.

### Error observability

New `error_event` table (migration 0035) capturing every fail-soft
path that fires in journal_ingest. Schema: `kind`
(`llm_api` / `parse` / `iter_cap` / `tool_call` / `capture_bg` /
`other`), `message`, `detail_json` (raw response snippet + err_msg
+ context), `source` (where in the code the error fired), timestamp.

New module `src-tauri/src/diagnostics/mod.rs`:
- `record_error()` — best-effort persistence; never propagates further
  errors.
- `list_recent_errors` + `clear_error_log` Tauri commands for a future
  Diagnostics UI.

The synthesis fallback in `journal_ingest` now calls `record_error`
with the full `err_msg` + a 2000-char snippet of the raw LLM response
each time it fires.

### Expandable error trace in chat

The synthesised "Travis hit an error" assistant message now carries
the underlying error detail in its `payload_json` under `errorDetail`.
The chat surface renders a collapsed `▸ error trace` block under the
error message. Click to expand and see:
- The LLM `err_msg` (parse error, HTTP status, etc.)
- A snippet of the raw response with character count
- A "copy for bug report" button that puts the whole trace on the
  clipboard

Means you can now actually tell us *what* went wrong instead of
opening devtools.

## v0.15.3 — Artifact retention + iterative refinement (2026-06-08)

The Claude.ai iterative-refinement loop now works in Travis. After
`run_python` generates a file, the script + inputs + outputs are
persisted as a `python_artifact` row with an id the LLM sees in the
tool response. When the user asks for a tweak ("remove the note",
"add 7 hours to row 1", "signature line a tiny bit down"), Travis
calls a new `edit_python_artifact` tool with the prior artifact id +
the edited script, re-runs it, and links the lineage via
`superseded_by`. No more from-scratch regeneration.

### New table: `python_artifact` (migration 0034)

- Stores the Python source, input doc IDs, output document IDs,
  stdout / stderr / execution time / error.
- `superseded_by` self-FK so the lineage is diff-able.
- Indexed by conversation, workspace, and supersedes pointer.
- v0.16's typed-edge memory graph will wire `EVOLVED_INTO` edges
  onto these rows once it lands (per the Open WebUI / LangGraph /
  OpenHands / AutoMem research synthesis).

### `run_python` changes

- After a successful run, persists a fresh `python_artifact` row
  with `superseded_by = NULL` (first-of-lineage).
- Tool response now includes `artifactId` so the LLM can reference
  it on a subsequent edit.
- Also: now uses `ctx.conversation_id` instead of hardcoded `None`,
  so artifacts attribute to the right thread.

### New tool: `edit_python_artifact`

- Inputs: `supersedesArtifactId`, `purpose`, `code` (the new full
  script with the edit applied), plus the usual `documentIds` +
  `libraries`.
- The LLM produces the edited script itself from its in-context view
  of the prior one — no internal LLM call inside the tool. Faster,
  cheaper, makes the diff visible to the user.
- Verifies the supersedes-link points at a real artifact before
  running; surfaces a clear error otherwise.
- Tool description signals: "use this for SMALL edits to a script
  you produced earlier — change a field, drop a row, adjust
  styling. For anything substantial, call `run_python` from
  scratch."

### Why this matters

The Claude.ai chat transcript that drove this whole arc shows
repeated patterns of: generate → user nudges → small edit. Each
edit was Claude rewriting the prior reportlab script with one
field changed, then re-executing. Travis can now mirror that exactly
— without paying for a regenerate, without losing the prior
artifact, with diff-able lineage for the eventual v0.16 case
substrate work.

## v0.15.2 — Extended thinking, capture goes background, cross-doc reconciliation, drive-the-process prompts (2026-06-08)

Five bundled architectural + behavioural improvements pulling Travis
toward Claude.ai parity.

### Extended thinking on the Claude provider

- Added Anthropic's extended-thinking parameter (`thinking: { type:
  "enabled", budget_tokens: 4000 }`) to every primary agent-loop LLM
  call. The model now gets dedicated cognitive-budget tokens before
  it produces tool calls or the final response — the same machinery
  Claude.ai uses for the visible "Thinking" boxes.
- Thinking content blocks come back in the response, parsed via a
  new `Thinking` variant on the `ContentBlock` enum, and surfaced
  to `ChatTurn.thinking_blocks`.
- In `journal_ingest`, each thinking block becomes a `Note` on the
  active manager step, so the worker's reasoning streams into the
  chat as it happens — same loop-doesn't-quit texture as Claude.ai.
- Cost: ~$0.06/turn at the 4000-token budget. Worth the depth for
  multi-doc reconciliation, constraint solving, forensic analysis.
- Retry path uses a 2000-token budget for the forcing prompt.

### Background capture (architectural split begins)

- New module `src-tauri/src/capture/mod.rs` with `CaptureSnapshot`
  + `run_background`. Task + reminder persistence (the two most
  visible "captured N new" pain fields) moves into
  `tauri::async_runtime::spawn` so the chat command returns
  immediately and persistence runs in the background.
- Emits a `capture-applied` Tauri event with counts so a future
  UI affordance can surface "tracked N in the background"
  notifications.
- Other capture fields (capability_gaps, entities, entity_facts,
  hypotheses, affect_signals, workspace_routing) still run inline
  for now — they touch more shared state and are higher
  refactoring risk. Queued for v0.15.3.

### Cross-document reconciliation prompts

- New `CROSS-DOCUMENT RECONCILIATION` section in the core system
  prompt: "compare overlapping fields across attached docs, flag
  discrepancies, name the authoritative source. A PO authorising
  payment overrides a sample from a previous engagement. A contract
  appendix overrides a downstream pricing sheet. A sign-in sheet
  overrides recollection."

### Drive-the-process recommendation prompts

- New `WHEN ASKED FOR A RECOMMENDATION` section: "lead with your
  recommendation, then justify it. Option-listing without a
  position is a cop-out. Push back on the user's stated instinct
  when it's wrong."

### Document handling rewritten in the core prompt

- Document editing is universal across professions; the core prompt
  now carries the generic `sample → analyze_styling → run_python →
  iterative refinement` pattern, multi-doc workflow guidance,
  spreadsheet-via-pandas, mid-workflow continuation cues. L2E pack
  fragment trimmed to just the L2E-specific bits: invoice numbering
  formula, default rates, the L2E-specific field enumeration,
  structured-action shortcuts.

### UI polish

- `StepRow` now auto-expands while a step is running (live thinking
  visible) and auto-collapses on success (clean completed
  view). Failed steps stay expanded so errors are surfaced.
- Multi-line `Note` content (thinking blocks especially) renders
  with `whitespace-pre-wrap` and consistent spacing.
- Completed steps show their summary as a muted "→ delivered" /
  "→ asked specific question" trailing line.

## v0.15.1 — Manager loop: the worker no longer gets to bail (2026-06-08)

Five releases of prompt-level enforcement (banned phrases, governing
principle, future-tense prohibitions, drive-the-process directive)
and the worker LLM kept producing handoff replies anyway — "reading
them now", "I'll generate", "give me a moment". The fix is
architectural, not prompt-engineering.

### The manager loop

Following the pattern visible in Claude.ai's chat (multiple
`Thinking` boxes per user turn — manager-driven sub-passes), Travis
now wraps the existing agent loop in an **outer manager loop** that
refuses to return until the worker actually delivered or asked a
real question.

- New module `src-tauri/src/manager/mod.rs`:
  - `ProgressKind { Delivered | AskedBlocker | Handoff }`
  - `evaluate_progress(extraction, generated_doc_ids, tool_calls_made)`
    — deterministic Rust function. Inspects the worker's output
    and decides which bucket. Detects placeholder/handoff phrases
    structurally, not by hoping the prompt holds.
  - `continuation_directive()` — the user-role forcing message
    injected between manager iterations to push the worker.
- `journal_ingest`: agent loop wrapped in `'manager: loop` with cap
  `MAX_MANAGER_ITER = 3` per user turn (PER request/response, NOT
  cumulative across the conversation). Each manager pass also gets
  `MAX_ITER = 8` tool-call rounds in the inner agent loop. Between
  manager iterations:
  - The worker's prior reply is appended as an assistant message.
  - The continuation directive is appended as a new user message.
  - The agent loop runs fresh with the augmented context.
- Each manager pass emits a visible `Thinking` step:
  - Pass 1 → "Working on it"
  - Pass 2 → "Forcing progress (pass 2)"
  - Pass 3 → "Forcing progress (pass 3)"
  Each completes with one of: `delivered` / `asked specific question`
  / `handoff — re-running` — same loop-doesn't-quit texture
  Claude.ai's chat has.
- The worker LLM is unchanged. The manager is just a Rust function
  watching what comes out.

### Why this matters

Worker LLMs are non-deterministic — Claude's training sometimes
overrides even very explicit prompt rules. The manager is a
*deterministic backstop*. If the worker says "reading them now" when
it should have read the docs, the manager detects the placeholder
phrase, injects a forcing directive, and runs the worker again. By
manager pass 2 or 3 the worker has either delivered or named a
specific blocker.

Cost guard: worst case is 24 LLM calls per user turn (3 manager × 8
agent). Typical case lands in 1 manager pass; the manager only
re-runs when the worker actually bailed. The v0.14.4 retry-on-empty
is now structurally redundant (manager catches empty responses too)
but kept in place as an extra inner safety net.

### Deferred to v0.15.2+

- File-level capture refactor (separate `capture::run_background`
  module + `tokio::spawn` second LLM call).
- Keychain file-fallback (only if your diagnostic info shows
  Windows Credential Manager is the issue).
- Opus 4.8 default model A/B against Sonnet 4.6 (only meaningful
  once the manager loop is proven — otherwise we won't know which
  fix did the work).

## v0.15.0 — Claude.ai-parity core + L2E moves to the pack (2026-06-08)

Minor-version bump reflects an architectural shift in the prompt
layering: the core system prompt is now domain-agnostic, and all
L2E-specific guidance (invoices, POs, sign-in sheets, schools,
$rates, services catalog) moves to the L2E pack's prompt fragment.
The chat surface should now feel like Claude.ai for general use,
with vertical depth layered on per enabled pack.

### Core prompt — generalist baseline

- Opening framing: "You are Travis — a personal AI assistant. You
  can help with anything Claude.ai can: writing, analysis, code,
  research, creative work, document handling, scheduling, and
  ops capture."
- Tool catalog organized by capability (writing/code/documents/
  memory/scheduling) instead of a flat alphabetic list.
- Examples cover non-ops use cases — drafting an email, analysing
  a spreadsheet in Python, summarising a project plan, pulling
  details from past memory. No invoice / PO / school references
  anywhere in the core prompt.
- Governing principle ("HOW YOUR TURN ENDS"), future-tense ban,
  and document-handling rules stay — they're universal.

### L2E pack fragment — domain depth

- Extended `src-tauri/src/packs/lead_to_empower/mod.rs::PROMPT_FRAGMENT`
  with the full invoice-generation workflow guidance previously
  embedded in the core:
  - PO + WO + sign-in-sheet → invoice PDF flow.
  - Invoice numbering rule (year + school code + sequence).
  - Default-rate references (Leadership Coaching ~$1,500/day
    school-funded vs $2,300/day DoF-funded).
  - Sample-→-analyze_styling-→-run_python pattern.
  - Spreadsheet handling with pandas.
  - Sample-→-adapt prompt template (the field-by-field
    enumeration pattern with `Bill to`/`Invoice #`/`Service
    dates`).
  - Workflow-continuation cues (mid-invoice doc uploads,
    numbered answers, constraints).
  - run_python vs structured action choice.

### Capture leaves the chat — behavioural split

- Primary LLM is told explicitly: "leave `tasks`, `entities`,
  `reminders`, `capabilityGaps`, `entityFacts`, `hypotheses`,
  `affectSignals`, `completedTaskIds`, `clarifyingQuestions`,
  `workspaceRouting`, `genericEntities` EMPTY. Don't narrate
  captures. A separate pipeline handles them."
- Inline persistence remains for now — anything the LLM does still
  emit gets stored silently. v0.15.1 will land the architectural
  split (separate `capture::run_background` module + `tokio::spawn`
  + dedicated capture-only LLM call).

### Why the bump

This is the first release where the prompt is no longer pack-locked.
If someone disables L2E or ships Travis with a different pack
(tutoring, consulting), the core behaviour stays sensible —
generalist by default, vertical depth on top.

### Keychain diagnostics

User reported "Claude API key not found in your OS keychain"
recurring even after re-entering the key. The generic error gave
no way to tell whether the keychain wasn't being written to, was
returning an empty entry, or the OS itself was misbehaving.

- `secrets::lookup_api_key` returns a `KeyLookup` enum
  (`FromCache` / `FromKeychain` / `NoEntry` / `EmptyEntry` /
  `KeychainError(msg)`).
- The "key not found" error in `llm::build` now names the actual
  failure mode:
  - **NoEntry** → "Open Settings → LLM Provider and enter your key."
  - **EmptyEntry** → "key in your OS keychain is empty, re-enter."
  - **KeychainError** → "OS keychain returned an error: {msg}. The
    key may have been stored under a different OS account, or the
    keychain access is locked."
- INFO-level tracing line on every successful keychain read with
  the character count, so the dev console can confirm what's
  happening.

A file-based fallback (for users hitting Windows Credential Manager
issues) is queued for v0.15.1 once we know whether the problem is
upstream-keyring or environmental.

## v0.14.5 — Drive the process: ban future-tense replies, fix Excel preload, visible doc reading (2026-06-08)

Real-world testing of v0.14.4 surfaced three new behaviours: Travis was
(1) writing future-tense placeholder replies ("Reading the docs now",
"I'll generate the invoice with number…") and ending the turn before
doing the work, (2) erroring out when given Excel master sheets
(spreadsheet content blew up the doc preload), and (3) showing nothing
visible during doc preload so the user perceived dead time.

### Future-tense is banned in `response`

- Hard prompt directive: "If your response contains 'I'll generate',
  'I'll create', 'I'll extract', 'reading them now', 'let me check',
  'working on it', 'give me a moment', 'I'll come back', etc., you
  have FAILED this turn. Go call the relevant tool(s) BEFORE writing
  the response. Then report the result in PAST TENSE."
- The `response` field description in the JSON schema now spells the
  same rule with concrete bad/good examples — e.g. *bad*: "I'll
  generate the invoice with number 2026217002"; *good*: "Generated
  invoice 2026217002 — total $15,000 over 10 days (link below). I
  assumed the IS 217 default rate of $1,500/day from the services
  catalog; let me know if that needs adjustment."
- New "HOW TO DRIVE A MULTI-DOC WORKFLOW" prompt section: "ASSUME you
  have what you need. The user gave you 5 documents — that's not a
  trial balloon, that's the input set. Use them."

### Spreadsheet doc preload — tight summary, not full content

- v0.14.3/.4 preloaded the full extracted_json for every attached doc
  into the user message. For 380KB master sheets, that exploded the
  context and the LLM errored. v0.14.5 detects spreadsheets by mime
  type / extension and replaces the full content with: a 400-char
  structural preview plus the instruction "Spreadsheet — mounted at
  /inputs/<file>. Use run_python with pandas (pd.read_excel) to read
  it. DO NOT request the full content here; query it in Python."
- Mount filename is sanitised to match the interpreter's path-safety
  rules (`src/interpreter/main.tsx`'s safeName regex).

### Visible doc reading

- Doc preload now wraps in a `Step` (the same substrate the tools use)
  so the user sees `Reading attached documents · 3 docs` in the chat
  with per-doc notes streaming as each one is loaded. No more dead
  air between sending and Travis's first tool call.

### Full model power on every turn

- The Haiku tier-down for capture-style turns is **disabled**. Every
  turn now uses the full default model (Sonnet/Opus for Claude). The
  "Travis didn't drive the process" failures are partly a
  model-quality story, and we'd rather pay cents than ship a weaker
  experience. Re-introduces the tier once the background-capture
  split lands and capture truly runs in its own process.

## v0.14.4 — Unblock the empty-response dead-end + user-message visibility (2026-06-07)

v0.14.3 enforced the "finish or ask" rule by deleting the synthesis
fallback, but it traded the polite-placeholder problem for a hard-error
dead-end: when the LLM agent loop couldn't produce a reply, Travis
surfaced "Travis didn't produce a reply on that turn" and the user had
no recovery path. This release fixes that and a couple of related
chat-UX regressions.

### Empty-response retry

- When the primary agent loop returns no usable response, Travis now
  runs **one retry** with a forcing prompt ("Your previous attempt
  returned no `response` value. Re-read the HOW YOUR TURN ENDS rules
  — call report_extraction now with a substantive `response`
  value."). `max_tokens` bumped to 2000 for the retry; system prompt
  is cached so the second call only pays for the forcing tail.
- If the retry also returns empty, Travis now shows a **specific
  error** based on what went wrong — "ran out of tool-call
  iterations, try sending fewer documents at once", "transient
  parse error, please try again", etc. — instead of the bare "open
  the dev console" message.
- `tracing::warn!` lines log `err_msg` + raw-response length on every
  empty-response path so the dev console can show why each retry
  fired.

### User-message visibility

- `flushSync` the optimistic-message state update so the user bubble
  paints to DOM *before* React commits the busy=true / live-turn
  rendering churn. Without it, React's batching could render both
  updates in one frame and the live-turn would push the just-sent
  message above the smart-scroll fold.
- After the optimistic commit, Travis scrolls the new user bubble
  into view at the top of the visible area
  (`scrollIntoView({ block: "start" })`) so it's anchored even when
  the bubble + live-turn together exceed one viewport height.
- Each `ChatTurn` now carries a `data-message-id` attr so the
  scroll-into-view query has a stable target.

### Deferred to v0.14.5

The full background-capture LLM-call split (separate `tokio::spawn`
extraction pipeline) is held for v0.14.5 — it's a larger refactor and
the user is blocked *now*. v0.14.4 is the unblock.

## v0.14.3 — Governing principle: finish or ask, never hand off (2026-06-07)

The "captured 1 new" / "reading them now" / "I'll come back" pattern
was breaking the chat: Travis was handing the conversational turn back
to the user before finishing the work. This release enforces the
governing principle end-to-end and gets capture out of the chat path.

### The governing principle

A new top-of-prompt section drills the rule:

> Your turn ends ONLY when one of these is true:
> 1. You delivered an artifact.
> 2. You asked a SPECIFIC question that the user must answer.
> 3. You hit a real blocker.
>
> "I'll come back with what I found", "reading them now", "give me a
> moment", "working on it", "captured", "noted", "got it" are NOT
> acceptable as a complete reply. Use your tool-call iterations to
> DO the work.

### Capture leaves the chat path

- Primary LLM is told to leave `tasks`, `entities`, `reminders`,
  `capabilityGaps`, etc. empty. Capture is invisible to the chat.
- The synthesis fallback that produced "captured N new" / "Working
  on it" / "Got the document(s) — reading them now" is **deleted**.
  No more polite placeholders standing in for real work.
- Captures the LLM does emit anyway are still persisted to the DB
  silently — the chat just never mentions them. (Architectural
  split into a separate background LLM call ships in v0.14.4.)

### Tool headroom

- **`MAX_ITER` 4 → 8.** With capture extraction off the primary
  pass the model has way more room to call tools — read_document,
  analyze_document_styling, then one or two run_python passes —
  before finalizing.

### Document preload

- When the user's message references attached documents
  (`doc#N` markers), Travis now sees the documents' extracted
  content on iteration 1 — pre-injected into the user message
  under `== ATTACHED DOCUMENTS (pre-extracted summary) ==`. The
  LLM doesn't have to spend a tool-call iteration on
  `read_document` just to see what's there; it can spend that
  iteration on `analyze_document_styling` or `run_python`
  instead. Falls back gracefully (the LLM can still call
  `read_document` for the full body).

### Chat UX

- **Hover jerk fixed.** The copy/delete action row now reserves
  its space and fades opacity in on hover instead of mounting on
  demand. No more bubble jump on hover.

## v0.14.2 — Chat persistence, message actions, workflow continuation (2026-06-07)

Second feedback batch. Three categories of fix:

### Conversation persistence

- **Chat survives tab switches.** Active conversation id lives in
  `localStorage` (`travis.activeConversationId`) and is the authoritative
  restore source; the backend's `most_recent_awaiting_user` heuristic is
  a fallback for first-run only. Switching from Ask to Notes and back
  no longer wipes the transcript.
- **"New chat" is the only reset path.** Travis never clears the chat
  on its own, even when a workflow finishes.

### Per-message actions

- **Copy + delete on every bubble.** Hover (or focus) any message to
  reveal a small action row. Copy puts the message body on the
  clipboard.
- **Delete trims forward.** Deleting a message removes that message
  and every message after it in the thread (Claude.ai-style), keeping
  the surviving transcript coherent. A confirmation prompt always
  shows first; nothing is removed without an explicit click.
- **`delete_message_and_after` Tauri command.** One SQL `DELETE`
  scoped to the conversation; orphaned step rows stay (they belong to
  the conversation, not the turn).

### Chat-input UX

- **No more user-bubble flash.** Optimistic messages now keep their
  React key across the round-trip — the server thread is merged into
  the existing list instead of replacing it, so `AnimatePresence`
  sees no unmount.
- **Instant file-attach feedback.** Dropping or picking a file shows
  a `reading…` placeholder pill the same frame the path comes in;
  the placeholder swaps in-place for the real document card when
  ingest finishes.
- **Smart scroll.** The transcript jumps to bottom on first load and
  follows new content only when the user is already at the bottom.
  Scroll up and Travis stays where you parked. A floating "jump to
  latest" pill appears when there's new content off-screen — click
  it to come back down.

### Workflow continuation — the "captured 1 new" bug

Three reinforcing changes:

- **`response` field is now required** in the LLM JSON schema with
  `minLength: 1` and an explicit prompt directive: "NEVER respond with
  just 'captured' or 'noted' — write a substantive reply that
  advances the work."
- **Workflow-aware fallback.** When the LLM returns an empty
  response AND there's an active workflow OR the user just uploaded
  documents, the synthesised reply is "Got the document(s) — reading
  them now. I'll come back with what I extracted and any open fields"
  instead of "captured N new".
- **Task suppression mid-workflow.** When the user's message contains
  `doc#N` markers AND there's an active workflow on the thread, any
  tasks the LLM tried to extract are dropped before persistence with a
  `tracing::info!` line — preventing the chat from ever showing
  "captured" on a mid-workflow document upload, even if the model
  hallucinates tasks.

## v0.14.1 — Chat-loop and offline polish (2026-06-07)

First feedback batch after v0.14.0. Travis stopped "hanging up" mid-workflow,
the chat panel now actually behaves like a chat, and the Pyodide runtime ships
in the bundle instead of being fetched from a CDN on first run.

### Chat UX

- **Steps land with the message they belong to.** Pre-fix the SQLite step
  timestamps were string-compared against RFC3339 conversation timestamps;
  the encoding difference pushed every step block to the bottom of the
  transcript. Now both are normalised through `Date.parse` with a 5 s
  tolerance, so a "Reading document" or "Run Python" group renders directly
  under the Travis reply that triggered it.
- **Auto-growing textarea + send button.** Replaces the single-line input;
  wraps text up to 8 rows before scrolling, Enter to send, Shift+Enter for
  a newline, explicit click target for touch users.
- **Attached files show up in the user bubble.** PDFs and docs as a
  clickable card, images as an inline preview. Click to open in the
  document viewer.
- **"Your turn" cue.** When the last message is Travis and `busy === false`,
  the input border glows + caret auto-focuses so it's obvious he's done.
- **Scroll history is preserved.** The flex container was missing
  `min-h-0`, which silently capped the transcript at the viewport. Now
  the full conversation scrolls.

### Reasoning + step streaming

- **`thinking` field** on extractions. The LLM emits a short
  one-sentence narration of what it's about to do; the chat surface
  renders it under the assistant bubble as muted text.
- **Action handlers stream steps** like tools do — `ActionRegistry::dispatch`
  now wraps every handler call in a `Step` with a human-readable label
  ("Saving invoice", "Logging activity") instead of the internal kind.

### Workflow continuation prompt

- **No more "captured 1 new" mid-workflow.** The system prompt now
  has a dedicated section explaining that when Travis is mid-workflow
  (he just asked for fields), the user's reply is slot fill, not a
  fresh capture. Mirrors the Claude.ai pattern where a workflow stays
  active until the artifact is generated or the user changes topic.

### Pyodide bundle

- **Local Pyodide bundle via `vite-plugin-static-copy`.** `pyodide.asm.wasm`,
  `pyodide.asm.js`, `pyodide.mjs`, `python_stdlib.zip`, and the lock file
  are copied into `dist/pyodide/` at build time; `loadPyodide({ indexURL: "/pyodide/" })`
  picks them up. No CDN call on first launch — works fully offline.

## v0.14.0 — Code execution + Claude-class chat (2026-06-08)

Travis can now do anything a smart user asks of it via in-app Python
execution, multimodal visual styling analysis, and a chat surface that
shows its work — without losing its persistent-memory + local-first
vertical-pack advantages. The end-state of the v0.14 spec
(`V0_14_0_SPEC.md`).

### What Travis can now do that it couldn't before

1. **Write Python in the moment** to generate any document layout that
   doesn't fit a hardcoded template. Sample-matching invoices,
   sign-in sheets matching a customer's template, constraint solving
   (find quantities that sum to $X exactly), reading .docx files,
   auditor-style cross-document reconciliation.
2. **See sample document styling.** Drop a sample PDF; Travis sends
   it to Claude vision, gets back structured JSON of header colours,
   fonts, table layout, signature placement. Feeds the JSON to the
   Python code so generated documents match the sample.
3. **Show its work step-by-step.** Every tool call, code execution,
   and reasoning step renders inline in the chat with name +
   checkmark + duration. Expandable for notes. No more silent
   "thinking…" spinners.
4. **Maintain multi-day cases.** A "case" survives across
   conversations with a rolling summary and decisions log. Resume
   "the PS 89 reconciliation" 3 days later and Travis picks up
   exactly where he left off.
5. **Save successful generations as reusable templates.** After Taylor
   confirms a custom-generated IS 217 invoice looks right, Travis
   saves the styling + working Python. Next time she invoices IS 217,
   the saved code runs instantly — no re-analysis, no fresh code
   generation.

### Slice-by-slice

**Slice 1 — Code interpreter substrate.**
- Hidden Tauri webview window hosts Pyodide (CPython compiled to WASM)
  with preinstalled reportlab/openpyxl/pypdf/pandas/pillow/python-docx
- New `interpreter` module + `run_python` Tauri command + LLM tool
- Documents mounted at `/inputs/`, outputs collected from `/outputs/`
- Outputs auto-register as Documents via the v0.12 substrate

**Slice 2 — Step-streaming backend.**
- Every tool call wraps in a Step (RAII helper) emitting typed events
- New `step` table for persistence; chat UI subscribes to live events
- Human labels ("Reading PO doc" not "read_document")
- Startup cleanup marks pre-crash 'running' steps as cancelled

**Slice 3 — Chat UI v2 (Claude-class).**
- Collapsible thinking sections, named steps with checkmarks
- Syntax-highlighted code blocks (`prism-react-renderer`) with copy
- Markdown rendering with tables/lists (`react-markdown` + `remark-gfm`)
- Inline file preview cards with OS default viewer integration
- Live step streaming during in-progress responses

**Slice 4 — Multimodal visual styling.**
- `analyze_document_styling` Tauri command + LLM tool
- Reuses Claude's native PDF input (same v0.12 mechanism, new prompt)
- Returns structured JSON: colours, fonts, layout, signature, margins
- Cached on `document.styling_json` for instant reuse

**Slice 5 — Fast/escape path dispatcher.**
- `WorkflowDef` gains `allow_code_escape` + `code_escape_hint`
- LTE invoice + sign-in-sheet workflows allow escape with detailed hints
- System prompt teaches when to use structured action vs `run_python`

**Slice 6 — Long-running cases.**
- New `travis_case` + `case_artifact` tables
- `open_case` / `note_case` / `close_case` / `find_case` LLM tools
- Active cases injected into journal prompt (same shape as initiatives)
- Frontend Tauri commands for case management surfaces

**Slice 7 — `pack_template` memory.**
- New `pack_template` table (workspace, pack, kind, label, counterparty)
- `save_pack_template` / `find_pack_template` / `get_pack_template` tools
- Saved styling JSON + Python code; counterparty-matched lookups
- `used_count` + `last_used_at` for "most reused" surfacing

**Slice 8 — Verification + version bump.**
- Acceptance scope: Taylor's 5 real tasks from the Claude.ai
  conversation (IS 217 invoice from sample, PS 19-style sign-in sheet,
  PS 89 reconciliation with smoking-gun mislabel, constraint solving,
  mid-conversation correction)
- Version 0.14.0 across package.json + Cargo.toml + tauri.conf.json
- Pyodide loads from jsdelivr CDN for v0.14 dev cycle; future polish
  bundles locally for offline use

### New migrations

- `0030_steps.sql` — step events persistence
- `0031_document_styling.sql` — cached styling JSON
- `0032_cases.sql` — travis_case + case_artifact
- `0033_pack_templates.sql` — reusable styling + code per counterparty

### Bundle size

Main JS bundle grew from 284 KB → 537 KB (gzip: 158 KB) from
markdown + syntax highlighting + chat components. Pyodide loads
lazily from CDN. Acceptable cost for the capability unlock.

---

## v0.13.5 — Pin tauri-runtime/wry to ~2.10 (2026-06-07)

v0.13.4 cleared the JS↔Rust version preflight (4m29s — got into the
actual Rust compile) but failed deep in `tauri-2.10.3/src/webview/
mod.rs:707` with a `Fn + Send` vs `Fn + Send + Sync` trait mismatch.
Root cause: the `tauri` crate is pinned to 2.10.3 but its transitive
deps `tauri-runtime` and `tauri-runtime-wry` weren't pinned, so cargo
lifted them to 2.11.2 — the newer runtime traits don't match what
tauri 2.10's webview implementation expects.

Added explicit `tauri-runtime = "~2.10"` and `tauri-runtime-wry =
"~2.10"` pins in Cargo.toml so the whole tauri 2.10 family stays
together.

---

## v0.13.4 — Pin Rust tauri-plugin-dialog to 2.4.2 (2026-06-05)

`tauri info` locally revealed the real mismatch the CI logs kept
referring to: the **Rust** `tauri-plugin-dialog` crate was 2.7.1 while
the **JS** `@tauri-apps/plugin-dialog` was 2.4.2 — Tauri's preflight
requires same major.minor on both sides. I'd been chasing
`@tauri-apps/api` version issues; the actual culprit was the plugin
itself. Plugin-dialog 2.5+ depends on tauri 2.11, which isn't in
our resolved tree.

Pinned `tauri-plugin-dialog = "~2.4"` in Cargo.toml, regenerated
Cargo.lock — Rust crate now resolves to 2.4.2 matching the JS side.

---

## v0.13.3 — Pin @tauri-apps/plugin-opener to 2.5.3 (2026-06-05)

The real root cause of the v0.13.0/.1/.2 CI failures: `@tauri-apps/
plugin-opener@2.5.4` (released this week) bumped its `@tauri-apps/api`
peer dependency to `^2.11.0`, while every other Tauri plugin in our
tree still uses `^2.8.0`. With our generous `~2.5.0` pin, npm hoisted
the latest matching patch (2.5.4), which triggered the tauri-action
preflight mismatch — even though the API itself was resolving to
2.10.1 via overrides, the *declared peer* in node_modules disagreed.

Pinned `plugin-opener` to exactly `2.5.3` until tauri 2.11 is on
crates.io and we can do a coordinated bump across the entire stack.

---

## v0.13.2 — npm `overrides` to force `@tauri-apps/api` consistency (2026-06-05)

v0.13.1's `~2.10.0` direct pin on `@tauri-apps/api` wasn't enough — the
new `@tauri-apps/plugin-dialog` carries `@tauri-apps/api: ^2.8.0` as a
transitive dependency, and npm hoisted the unbounded latest (2.11.0)
into the tree even with the lock present. Added an `overrides` block
that forces every reference in the tree to `~2.10.0`, regenerated the
lock from scratch. Also pinned `@tauri-apps/cli` to `~2.10.0` so the
build CLI stays aligned with the runtime crate.

---

## v0.13.1 — Pin @tauri-apps/* npm packages to ~2.10 (2026-06-05)

The v0.13.0 build failed in CI because npm install picked up
@tauri-apps/api@2.11.0 (latest) while the Rust `tauri` crate is still
2.10.3 on crates.io. Tauri's preflight check rejects the major/minor
mismatch. Pinned all four @tauri-apps/* JS packages to `~2.10.x` /
matching minors so the npm tree stays aligned with the Rust crates
until tauri 2.11 publishes to crates.io. Code unchanged from v0.13.0.

---

## v0.13.0 — Five-piece response to Taylor's first real test (2026-06-04)

Taylor's feedback after using v0.12.3 against her real workflow:
1. "Engagement and contract is too broad — might mean the same thing"
2. "We don't always invoice the full amount at once. A contract can
   have many invoices until the amount is complete"
3. "Upload the PO (or WO) and Travis can create a contract from it"
4. "There's no way from the UI/Ask/chat interface where files can be
   uploaded"
5. "The UI should show that files have been uploaded and the workflow
   drive should always be running/active"

All five land in this release.

### Collapsed contract + engagement into one record (LTE pack v0.7.0)

The two-table distinction was an abstraction I added that didn't match
her real work. Migration `0005_collapse_contract_engagement` extends
the `engagement` table with every contract-shape field (`ref`,
`ceiling_cents`, `term_start`, `term_end`, `signed_at`,
`parent_solicitation`, `pdf_path`, `counterparty`, `contract_status`),
backfills data from any standalone `contract` rows, and synthesises
engagement rows for orphan contracts so no data is lost. The standalone
`contract` table stays for backward compat but is hidden from the
sidebar — engagement IS the contract now.

UI / chat / extraction prompts say "Contract" everywhere. The SQL
table stays named `engagement` for code stability — only labels change.
Pack prompt fragment has an explicit "in this app, contract and
engagement refer to the same record" note at the top so the LLM
doesn't drift back to the old vocabulary.

### Many invoices per contract — draw-down tracking

`propose_program_invoice_draft`'s reply now includes a draw-down line:
"Draw-down: $5,500 invoiced of $7,064 total · $1,564 remaining". If
the new invoice would push past the contract ceiling, the reply warns
"⚠ over ceiling by $X". `lte_find_contract` already surfaced
invoiced/remaining/burn percent; that surface now queries the
engagement table directly.

### PO/WO → contract

New workflow recipe `lte_create_contract_from_doc` (slots: source
document, kind = `po` or `wo`). New action handler
`CreateContractFromDocHandler` extracts vendor, school, period, total
from the document's extracted JSON, resolves/creates the school,
inserts the contract (engagement row) with all fields pre-populated,
and links the source document via `document_link`. The same workflow
takes PO or WO — both represent a contract per Taylor.

### File upload in AskTab (main app chat)

The Ask tab in Manage was a chat surface with no file affordance.
Now mirrors the overlay's wiring:
- Drag-drop listener on the main window
- Paperclip button → `tauri-plugin-dialog`'s native file picker
  (added new dependency `tauri-plugin-dialog` + capability)
- Chip strip showing attached documents
- Each chip expands to the same `DocumentExtractCard` from the
  overlay
- Submit appends `[Attached: name (kind, doc#N)]` to the chat payload
  so the LLM sees the attachment, then clears the strip

### Active workflow indicator (always-visible status)

New `ActiveWorkflowPill` React component, rendered above the input
in both AskTab and the overlay. Shows what Travis is currently
working on, how many slots are filled, what's still missing, and
what the next ask is. Refreshes via a new `workflow-state-changed`
event the backend emits after processing workflow ops, so it stays
in sync without polling. Tappable to expand into the full slot
breakdown.

### Backend changes

- New `tauri-plugin-dialog` plugin registered, `dialog:default` and
  `dialog:allow-open` capabilities added.
- New Tauri command `get_active_workflow(conversationId)` →
  `WorkflowSurface` (recipe info + per-slot filled state + next ask).
- New `workflows::cmd` module.
- New action `lte_create_contract_from_doc` registered with the
  action registry.
- `lte_find_contract` tool rewritten to query `engagement` (with
  the new contract fields) instead of the legacy `contract` table.

---

## v0.12.3 — In-app update banner (2026-06-04)

The v0.12.2 background poll already fires a native OS notification
when a new version is available; this release adds a non-intrusive
in-app banner so the prompt is visible inside Travis itself even when
the OS notification has been dismissed or notification permission is
not granted. Banner appears at the top of the main window, shows the
new version number, and has Install / Dismiss buttons. Dismissals are
per-version per-session.

---

## v0.12.2 — Auto-update polls in the background (2026-06-04)

Travis no longer requires Taylor to remember to check Settings for
updates. A background poll runs every 4 hours: when a newer version is
published in the release feed, Travis emits an `update-available` event
the frontend can listen for AND fires a one-shot system notification.
First check happens ~60 seconds after launch (gives the app room to
settle into its other startup tasks). Once-per-version dedup so back-
to-back polls don't re-notify the same version twice in a session.

The existing Settings "Check for updates" button still works as the
manual path; the new poll just removes the need to remember it.

---

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
