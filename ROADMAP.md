# Travis — Evolution Roadmap

This is the long-form plan for taking Travis from a local-first single-user
desktop app into a Jarvis-shaped personal assistant with a sustainable
open-core business under it. It captures the design decisions made across
the workspace / pack / cloud architecture conversations and turns them
into a phased build.

## Thesis in one paragraph

Travis is **open-core**: the entire client, sync protocol, knowledge
graph, pack/tool system, and self-hostable cloud relay all live on the
open-source repo forever. Travis Cloud is **a hosted convenience layer**
that absorbs the operational pain — managed relay, hosted LLM gateway,
pre-registered OAuth apps for integrations, premium tools provisioned
per account. People who want to run their own everything always can;
most people pay a subscription because the convenience is worth more
than the effort. Enterprise customers buy custom-tool provisioning
bolted on top of that.

The line that matters: **never paywall a feature that already works
locally.** The cloud only adds *capabilities you can't get locally*.
That preserves trust and dodges the "open source bait-and-switch"
reputation that kills these models.

---

## The architecture in seven layers

Travis is an orchestra, not a model. Each layer is independently
swappable; the boundaries between them are where the design pays off.
The phased build below ships these layers in slices over time — but
the conceptual shape is fixed.

1. **Identity + state ("self").** One user identity across every
   embodiment — phone, desktop, car, home. State lives encrypted on a
   cloud relay; keys live on devices. Cloud sees ciphertext; devices
   decrypt. Without this layer the product can't be trusted at scale.

2. **Knowledge graph ("memory").** Typed entity / relation graph:
   people, places, projects, events, relationships, facts. Built
   incrementally through observation, never via upfront onboarding
   interrogations. Embeddings sit on top of the graph for fuzzy
   retrieval, not replacing it.

3. **Orchestrator ("brain stem").** Small + fast, not large + smart.
   Routes input to the right specialist, decides which slice of the
   knowledge graph is relevant, assembles context. Doesn't reason —
   it dispatches. Could be a small local model or heuristics +
   embeddings (the layered intent-routing pattern in Phase 3).

4. **Reasoner ("brain").** Frontier model called for actual hard
   thinking. Receives a trimmed, contextualised prompt. Outputs
   structured action proposals. Stateless across turns — context is
   always assembled fresh from the knowledge graph.

5. **Action layer ("hands").** Registry of capabilities — local and
   remote. Every action has a declared schema, a permission policy,
   and an audit entry. Reasoner *proposes*; policy + confirmation
   *decides*. This is where most of the engineering work goes; the AI
   part is the easy part.

6. **Perception ("senses").** Microphones, cameras, screen observers,
   location, calendar, notifications. Each runs at its own latency
   tier. The orchestrator chooses what to attend to. Multimodality is
   layered (specialised STT / TTS / vision components) not unified.
   Every sensor has a kill switch.

7. **Trust + audit ("ethics").** Append-only signed log of everything
   Travis did and why. User can review at any time, undo any action,
   change the rule that allowed it. Four autonomy classes: read
   (always), write-local (often auto), external-action (usually
   confirm), irreversible (always confirm with reason shown).

These layers map onto the phased build: Phase 4 plants the knowledge
graph, Phase 3 + 5 split orchestrator from reasoner, Phase 5 + 8
mature the action layer, Phase 6 establishes identity + state, Phase
11 brings perception online. Every phase ships a vertical slice
through the layers it touches.

## Cross-cutting principles

- **Local-first by default, cloud-by-choice.** Inference can be local
  (slow, private) or cloud (fast, with strict privacy contracts). User
  picks. Data is never on the cloud unencrypted. This isn't a
  marketing line — it's an architectural constraint that shapes every
  layer.
- **Models are commodities; the system is the moat.** In 2030 frontier
  models will be 10× cheaper and roughly fungible. The defensible thing
  is what surrounds them: the knowledge graph that's been growing for
  years, the action integrations, the trust the user has placed in the
  audit + privacy story.
- **One identity, many embodiments.** Phone Travis, desktop Travis,
  car Travis are not different products — they're the same identity
  rendering different capabilities. Each embodiment runs the
  orchestrator + the tool subset its hardware supports. Same knowledge
  graph behind all of them.
- **Trust is a substrate, not a feature.** Audit log, reversibility,
  per-action policy, kill switches — these aren't a checklist of
  things to add. They're the floor everything else stands on. Without
  them, the product fails the day Travis sends a wrong email to the
  user's boss.
- **The action layer is where engineering bulk goes.** Roughly 80% of
  the work is integration code — calendar APIs, email APIs, smart home
  protocols, work tools. Plan for this. Two great integrations beat
  ten mediocre ones.

---

## Phases

### Phase 0 — Foundation *(v0.1.x)*

Local-first desktop, single user/device. Open source. L2E-coupled —
Travis worked, but the data model and prompts assumed one customer's
domain. Phase 1 unlocks everything downstream.

### Phase 1 — De-domain *(SHIPPED in v0.2.0)*

**Goal:** Travis is generic at the data-model level; the L2E stuff is a
pack like any other.

**Shipped:**

- ✅ `PackHandle` trait — slug, name, version, migrations, prompt
  fragment, declared entity kinds + action kinds, `register_tools` /
  `register_actions` hooks. Per-pack migration runner tracks
  `meta.pack.<slug>.schema_version` independently of core's `_sqlx_migrations`.
- ✅ L2E lives entirely under `src-tauri/src/packs/lead_to_empower/`:
  typed domain modules (coach / school / coach_hours / signing_sheet /
  invoice), Tauri command surface, PDF generator, the
  `propose_invoice_draft` action handler, the system-prompt fragment.
- ✅ Cargo feature `pack-lead-to-empower` (default-on) gates
  compilation. `tauri::generate_handler!` accepts `#[cfg]` per-item, so
  L2E commands disappear cleanly when the feature is off.
- ✅ Universal spine — `entity` (generalised from `entity_index`),
  `relation`, `event`. Three core tables that don't presume a domain
  shape; every pack writes to them from its CRUD paths so cross-pack
  retrieval works from day 1.
- ✅ `task` graduated to core as a thin opt-in. CHECK constraint
  dropped; new `entity_id` column links to the spine.
- ✅ Action registry replaces static dispatch. Built-in handlers
  pre-register; pack handlers register at startup.
- ✅ Tool registry takes the pack list and lets each pack contribute
  its own tools.
- ✅ Journal extraction is dynamic — entity buckets and the
  `proposedActions.kind` enum come from the live pack registry +
  action registry.
- ✅ Pack prompt fragments concatenated into all four system-prompt
  assembly sites (journal, proactive, summary, ask).
- ✅ Frontend gates pack-supplied UI (the Invoices tab) on a
  pack-installed flag exposed via `appStatus.enabledPacks`.

**Still open before "Phase 1 fully done":**

- 🟡 **Validate the abstraction with a second pack.** Build the
  tutoring pack from scratch — proves the format isn't accidentally
  shaped around L2E. This is the test that says "we did Phase 1 right."
- 🟡 **Runtime-installable packs.** Today packs ship at compile time
  via Cargo features. Drag-and-drop a `.zip` → install at runtime
  comes once a second pack exists to test it with.
- 🟡 **Settings → Packs UI** for toggling enabled packs (depends on
  runtime install).
- 🟡 **L2E-flavoured prose still in core prompts.** The journal
  extraction system prompt still mentions invoice drafting and coach
  examples directly. Step-10 plumbed pack fragments alongside; a
  follow-up trims core back to a fully neutral baseline.

**Open source:** all of it. **Cloud play:** none yet.

### Phase 2 — Workspaces as context organisation *(3–6 weeks)*

**Goal:** Multiple workspaces per install, with cross-workspace retrieval
and one universal conversation thread.

- `workspace_id` on every relevant table.
- Per-workspace `cross_visible` flag (default true; profiles override
  for sensitive categories — health, therapy, legal, finance default to
  isolated).
- Retrieval ranks active workspace highest, falls back across visible.
- Universal conversation thread, auto-close after 7 days inactive.
- Related-past-conversations injected into system prompt.
- Clarifying-question heuristic for cross-workspace ambiguity (only ask
  when the resolution is genuinely contested; default silently to the
  active workspace when there's a confident match).
- Workspace switcher UI.

**Open source:** all of it. **Cloud play:** still none.

### Phase 3 — Token economy *(1–2 weeks, slipped into Phase 2's tail)*

**Goal:** Per-call cost goes down by an order of magnitude.

- System-prompt cache hygiene — pull the date out of the cached prefix
  so Anthropic prompt-cache hit rate stays at ~100% within the 5-minute
  window.
- Heuristic fast-path for greetings + obvious task ops (no LLM call at
  all for "good morning" / "mark task 12 done").
- Embedding-based intent router with confidence-gated retrieval
  trimming. Reuse fastembed; bank of ~30–80 example utterances per
  intent; route to a trimmed prompt when confident, full prompt when not.
- Tiered LLM call (Haiku → Sonnet) for ambiguous cases.

This isn't a "product" phase, it's a "we're not on fire when the bill
arrives" phase. Worth doing before any cloud play because cloud
profitability is sensitive to per-call cost.

**Open source:** all of it.

### Phase 4 — Knowledge graph foundation *(2–3 months)*

**Goal:** Travis stops being a notes app with extras and becomes a memory.

- Typed entity/relation graph alongside the pack tables:
  `entity (id, type, name, attrs)`, `relation (from, to, kind)`,
  `event (entity_id, kind, attrs, at)`.
- Pack tables can *project into* the graph (a `coach` row also exists
  as an `entity` of type `person:contractor`).
- Inference layer: when a name appears N times in journal entries,
  propose adding it as a contact ("I've noticed you mentioned Maria a
  few times — want me to track her?").
- Embeddings index on graph nodes plus their text payloads, not just
  journal blobs.
- Graph-aware retrieval becomes the default for memory-heavy queries.

**Open source:** all of it. **Why this matters commercially:** the
graph is the moat. A user with 12 months of graph history has
switching costs measured in life-organisation, not dollars. Cloud sync
of this graph is the thing they'll pay to never lose.

### Phase 5 — Tool spec compiler *(2–3 months, can overlap with Phase 4)*

**Goal:** Tools become declarative (with native escape hatch). Remote
provisioning becomes safe and live.

- `ToolSpec` JSON format (id, params schema, capability tags, data
  access, guards, audit, body — declarative ops or `native: "fn_name"`).
- `tool_engine` Rust crate that loads + validates + dispatches specs.
- `flags.rs` extended to deliver per-account tool spec lists.
- Native tools register via spec wrapper; legacy `Tool` trait keeps
  working as the escape hatch.
- Pack manifests use ToolSpec for their tool definitions.

**Open source:** the engine, the spec format, all packs. **First
commercial wedge:** *enterprise customers can have custom tools
provisioned to their account without a Travis release.* This is what
your COO and future enterprises actually pay for — solutions-engineering
work that becomes a sustainable revenue stream.

### Phase 6 — Encrypted cloud relay + identity *(3–4 months)*

**Goal:** Same Travis from any device. Privacy preserved.

- Identity: sign in with Google or Microsoft (existing OAuth infra).
- Sync engine: CRDT-based (Yjs / Automerge) state replicated to a
  cloud relay over WebSocket.
- E2E encryption: per-user key derived from device passkey; cloud sees
  ciphertext; can't decrypt anything.
- Self-hostable relay (Docker image, k8s chart). Same protocol the
  hosted version uses.
- Conflict resolution at the data-model level — known-tricky, eat the
  time to do it right.

**Open source:** client code, sync protocol, relay server. **First
major paid product:** Travis Cloud (managed relay). Self-host is a
button-mash-and-config-files affair; cloud is "sign in and it works."

### Phase 7 — Hosted LLM gateway *(1–2 months on top of Phase 6)*

**Goal:** Cloud users don't need API keys.

- Travis Cloud proxies LLM calls to Anthropic / OpenAI on our keys.
- Per-account quotas + Stripe usage-based billing.
- Self-hosters keep BYOK; works exactly as today.
- Quotas enforced server-side; over-quota responses gracefully degrade
  ("you've used your monthly tokens; bring your own key for unlimited
  or upgrade").

**Open source:** client; the gateway server. **Commercial:** flat
subscription + over-quota usage rates. This is the main consumer
revenue mechanism.

### Phase 8 — Pre-registered OAuth apps *(part of cloud, ongoing)*

**Goal:** Cloud users connect Gmail / Outlook / Slack / Notion in one tap.

Today: each user registers their own Google Cloud project / Azure app.
That's a 20-minute per-integration setup that ~99% of users won't do.

- Cloud users get OAuth flows that go through Travis's pre-registered
  apps.
- Self-hosters keep registering their own (the docs already exist).
- Per-integration verification dance with Google/Microsoft (annoying
  but one-time per integration).

**Open source:** the OAuth client code (already done). **Commercial:**
"you sign in once and Gmail just works" is a massive convenience pull.

### Phase 9 — Mobile companion *(3–4 months)*

**Goal:** Capture from anywhere. Read-mostly UI.

- Native Swift (iOS) + Kotlin (Android), or React Native if speed
  matters more than polish.
- Capture-first: voice/text → routed to cloud relay → synced to desktop.
- Read views: tasks, recent conversations, search.
- Push notifications for proactive nudges + reminders (already built
  on backend).
- Some write actions (mark task done, snooze reminder, send a queued
  email).

**Open source:** the mobile clients. **Commercial:** mobile is a
Cloud-tier feature in practice (no relay = no point) but technically
self-hostable.

### Phase 10 — Web client *(2 months, can sit alongside mobile)*

**Goal:** Use Travis from a browser when you can't install.

- Reuses the existing React frontend with adjustments.
- Connects to relay over the same sync protocol.
- Read-mostly initially; write parity with mobile.

**Open source:** yes. **Commercial:** part of cloud; trivial to
self-host.

### Phase 11 — Voice / ambient *(6+ months, last)*

**Goal:** From "tool I use" to "presence I have."

- Wake word on device (TFLite / native).
- Streaming STT (Whisper API or local Whisper.cpp).
- Streaming TTS (ElevenLabs API or native).
- Optional ambient screen awareness with explicit consent + visible
  "recording" indicator.
- Voice-first interaction model — different prompt design, different
  turn-taking, different latency budget.

**Open source:** all of the wiring. **Commercial:** STT/TTS API costs
absorbed by Cloud tier; self-hosters use local Whisper + system TTS
(works, slower).

### Phase 12 — Marketplace / multi-user / shared workspaces *(year 2+)*

**Goal:** Family/team Travis. Definitely cloud-tier.

- Sharing model: invite to a workspace with permissions.
- Privacy boundaries between users in the same household.
- Shared knowledge graph slices.
- Enterprise admin console.

This is when Travis becomes a real B2B SaaS shape, with all the
operational implications.

---

## Disciplines

Operational rules that apply across all phases. These aren't optional
once there are real users — they prevent the kind of damage that's
hard to walk back.

### Profiles vs packs

- **Packs are the granular unit.** A pack ships a small slice of
  functionality: typed tables (with namespaced names like
  `lead-to-empower-ops.coach`), tools, prompt fragment, UI hints, its
  own migration set, its own `schema_version`.
- **Profiles are curated bundles of packs** for onboarding cold-start.
  "Coaching agency" = `contractors-and-hours` + `invoicing` + `sites`
  + `signing-sheets`. "Solo creator" = `clients` + `invoicing` +
  `projects`. The shared `invoicing` pack means we don't write
  invoicing twice.
- **Profiles solve cold-start; packs solve the long tail.** A user
  picks a profile during onboarding for a sensible default; later
  they add or remove individual packs as their work changes.
- **Workspaces hold pack installations**, not profiles. A profile is
  a one-shot setup convenience; the workspace's actual state is
  "which packs are installed."

### Pack-update discipline

- **Migrations are additive within a major version.** New columns:
  yes, with defaults. New tables: yes. Renames or drops: only in a
  new major version, with a migration script that copies data and a
  deprecation window where old + new exist.
- **Each pack ships a `min_app_version`.** The app refuses to load a
  pack targeting a Travis newer than itself, and prompts to update.
- **Pack `schema_version` is independent of app version.** Migrations
  are cumulative within the pack.
- **Uninstalling a pack never drops user data.** Tables become hidden
  from the UI and tool registry, but rows persist. A separate
  explicit "purge data for pack X in workspace Y" action exists,
  requires confirmation, has a 7-day undo window. People panic when
  they realise they uninstalled the wrong thing.
- **CI integration-tests every pack migration** against the previous
  version's DB before the pack ships. Non-negotiable once there are
  customers depending on packs.
- **Workspace-state checkpoint before any pack migration runs.** If a
  migration partially fails, the workspace returns to the
  pre-migration state; the user sees an error rather than a
  half-broken DB. Cheap with SQLite — copy the file, swap back on
  error.

### Native vs remote tools

- **Native tools** ship in the binary. Anything that touches the local
  DB, OS, filesystem, clipboard, or has nontrivial logic. The L2E
  invoice / coach / school stuff is here. Provisioning at runtime is
  via a per-account flag list — code is in the binary, the cloud
  decides whether each tool is *visible / active* for the workspace.
- **Remote tools** are JSON-defined REST/HTTP wrappers the runtime
  executes generically. Schema for params, OpenAPI-shaped spec for the
  call, response shape. Fetched live from the cloud per account, no
  rebuild needed. The right surface for "build a wrapper for Acme
  Corp's quirky CRM" — exactly the niche-per-customer enterprise
  case. The runtime applies the same guards (permissions, audit,
  confirm cards) before dispatching.
- **The native escape hatch is part of the design**, not a fallback.
  The 20% of tools that need real logic declare
  `body: { native: "fn_name" }` and point at a registered Rust
  function; the declarative shell still owns permissions and audit.

### Cross-workspace confidentiality

- **`cross_visible` is a per-workspace boolean** (default `true`).
- **Profile-driven default overrides.** `health`, `therapy`, `legal`,
  `finance` profiles default to `false`. The picker at workspace
  creation shows the default and a one-tap toggle.
- **Retrieval, tool calls, and system-prompt context-summary all
  honour the flag.** A non-cross-visible workspace's data never
  surfaces in another workspace's session.
- **The user always sees the default at creation and can change it
  later** in the workspace's settings panel. Never silent.

### Conversation reopen rules

Auto-close after 7 days of `awaiting_user` is the default behaviour.
Reopening has two paths:

- **(a) The user explicitly references a closed conversation** ("what
  was the resolution on that PS 142 invoice?"). Travis matches by
  semantic similarity + entity overlap; if confident, reopens by
  flipping status back to active and threads the new turn onto the
  old conversation.
- **(b) The user is in a different conversation that turns out to be
  related to a closed one.** Travis *references* the past one in its
  reply (the related-past-conversations injection from Phase 2) but
  does NOT silently reopen it. Two open conversations that are
  secretly the same thread is worse UX than one open and one
  referenced.

Reopening is destructive-ish (it changes state); require a clear cue.

### The two-products risk

Travis-the-assistant and Travis-the-platform have different polish
bars. Easy to half-finish both. Decide which gets shipped first and
what level of polish the platform needs internally vs externally.

Default stance: **the platform exists to serve the assistant**, not
the other way around. Don't build platform features speculatively
because someone might want to write a custom tool — build them when a
real customer needs a real tool.

---

## The open-source / cloud line

| | Open source (forever) | Travis Cloud (paid) |
|---|---|---|
| Client (desktop, mobile, web) | ✓ | uses identical client |
| Sync protocol + relay server | ✓ self-hostable | ✓ managed relay |
| Knowledge graph, packs, tools | ✓ | ✓ |
| LLM access | BYOK | hosted gateway, no key needed |
| OAuth integrations (Gmail, Outlook, Slack…) | register your own apps | pre-registered, one-tap |
| Custom tool provisioning per account | DIY | done for you (enterprise) |
| Premium pre-built packs | ✓ where applicable | early access + niche packs |
| Multi-device sync | DIY relay | included |
| Ambient / voice with hosted STT/TTS | local-only | cloud-routed |
| Support / SLA | community | included by tier |

## Pricing skeleton *(rough, gut-checked against Linear, Notion, Plausible)*

- **Free** — BYOK, BYO relay, all features that work without cloud.
- **Hobby ~$8/mo** — managed cloud, hosted LLM with monthly token
  quota, 2 devices.
- **Pro ~$18/mo** — higher token quota, unlimited devices, premium
  integrations.
- **Team ~$12/seat/mo** — shared workspaces, admin console, audit
  retention.
- **Enterprise — custom** — private packs, dedicated infra, custom
  tool development, support.

The business shape: hobby + pro pay for ops; team is the volume play;
enterprise is the high-margin bespoke work (and the L2E shape — the COO
is essentially the first enterprise customer).

## License decision

Two real options:

1. **MIT / Apache** — maximally permissive, but a competitor can take
   the code and run a competing hosted Travis. Mitigated by (a) brand
   plus first-mover and (b) the cloud features being where the value
   is. Plausible Analytics does this; works.
2. **AGPLv3** — copyleft. Anyone running it as a service has to
   open-source modifications. Effectively prevents AWS-style commodity
   copies. Some enterprises won't touch AGPL though.

Recommendation: start MIT/Apache. The world has changed since the
early MongoDB-vs-AWS days; first-mover + cloud convenience tends to
win, and the open licence will help adoption (which fuels the moat).
Switch to AGPL only if a real commodity-clone threat appears — usually
2–3 years in, if at all.

## Honest timeline

This is a **two-year roadmap** to "real Jarvis" with a real business
under it. Phases 1–5 are roughly six months solo (current pace).
Phases 6–8 (cloud + identity + LLM gateway) are six more months — and
the moment when Travis goes from "open-source side project" to
"company." Phases 9–11 are the year-two arc.

## Honest risks

- **Cloud relay is operationally heavy.** The day there's one paying
  customer, someone is on call. Plan for incidents, support email,
  billing disputes. This is the single biggest "are you sure?" moment
  in the whole plan.
- **The action layer eats engineering forever.** Each new integration
  (Slack, Notion, Jira, …) is its own pack-of-pain. Plan to ship two
  great integrations rather than ten mediocre ones.
- **Voice/ambient is where most "AI assistant" startups die.** The
  latency + privacy + UX is brutal. Don't start it until the rest is
  solid.
- **The schema migration story has to be airtight before mobile.**
  Mobile means a copy of the data on a device that ships months out of
  date. Backwards-compatible migrations stop being optional.

## What to do next month

Phase 1 is the right next bite. It's the gateway to everything else,
it's mostly mechanical, and it ships value immediately — your COO's
domain becomes a pack like any other; later customers pick the same
shape. Concrete sequence:

1. Write the `Pack` manifest and `ToolSpec` design notes
   (`docs/PACK_MANIFEST.md`, `docs/WORKSPACE_CONTEXT.md`,
   `docs/TOOL_SPEC.md`).
2. Refactor `tools/mod.rs` so `available_tools()` filters by an
   `is_enabled(id, workspace)` check.
3. Carve out the `lead-to-empower-ops` pack from the existing code
   (rename tables, namespace tools).
4. Add the Pack registry + a "no-op" loader (everyone gets all packs
   active, same as today) so this ships without a behaviour change
   first.
5. Then the workspace concept on top.

After Phase 1 there's a foundation that doesn't need to be re-done at
any later phase. The cloud build, the mobile client, the marketplace —
all assume the pack/workspace shape laid out here. Get this part right
and the rest is execution.
