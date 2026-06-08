# BRAIN — Travis as a partner

> **Status (2026-05-09):** vision spec. Drives the cognition-track
> work that runs alongside ROADMAP's infra phases (cloud / mobile /
> voice). Not a slice plan — a framework for deciding which
> capabilities ship next, in what order, and what *Travis* means
> beyond "useful tool."

---

## Premise

Travis today is a memory + retrieval layer with operational packs.
It captures, structures, recalls, and proposes — useful, but
fundamentally **reactive**. The end state is qualitatively
different: a partner that *thinks alongside* the user, has a
recognisable perspective, advocates for itself when it needs
something, and acts in the user's interest without being asked.

The shorthand: **Travis becomes Jarvis.**

That word does work. It commits to a posture: not an assistant that
follows orders, but a collaborator that has its own observations
and isn't shy about voicing them. The seven capabilities below are
what separates the two.

This doc is the framework for getting there. ROADMAP.md plans
infra (cloud sync, mobile, voice). BRAIN.md plans the **cognition
stack** that runs on top of any infra.

---

## The seven capabilities

Each section describes the capability, why it matters, where v0.5.0
falls short, what the work is to get there, and the failure modes
to avoid. They're presented in roughly the order the substrate has
to be built — earlier ones unlock later ones.

### 1. Reasoning

**What it means:** Travis composes multi-step inferences across the
graph. Not *"what did you say about Maria?"* (retrieval) but *"you
log Maria's hours every Friday and last Friday is missing — should
we draft her sheet?"* (observation + inference + proposal).

**Why it matters:** The "no brain" critique reduces to: *Travis
remembers but doesn't think.* Retrieval is memory; reasoning is
the difference between memory and thought. Without it, the LLM
does all the thinking on every turn from scratch and Travis itself
holds no positions.

**v0.5.0 state:** The graph supports the queries reasoning would
need (timeline, co-mentions, confidence) but Travis doesn't compose
them. The LLM gets context injected; it thinks once per turn; the
conclusion isn't preserved.

**The work:**

- **Composed graph queries.** Multi-hop traversal helpers (paths
  between entities, cluster detection, frequency anomalies) — built
  as Rust functions the LLM tools layer can call. *"Find every
  entity connected to Maria via 2 hops"* returns a structured
  result, not text.
- **Persisted conclusions.** When Travis answers a question with
  reasoning, store the conclusion as a typed `claim` row tied to
  the entities it touched. Future retrievals surface prior
  conclusions as context, not just events.
- **Reasoning chains in responses.** The chat reply optionally
  shows the path: *"I think Maria works at PS 142 because: 8
  captures co-mention them; you logged 24h under (Maria, PS 142)
  in the last month; signing sheets for that pair were signed."*
  Off by default; on for debug or when the user asks "why?"
- **Confidence-aware answers.** Every claim Travis makes carries a
  confidence — high (typed records / many corroborations), medium
  (LLM inference from observed events), low (single mention). The
  user sees the level. Confidence is information, not a UI knob.
- **Working memory for ongoing reasoning.** A short-lived (~30
  min) cache of "things Travis is thinking about right now" so a
  multi-turn conversation can refine a hypothesis instead of
  re-deriving it.

**Failure modes to avoid:**

- Confident hallucinations — reasoning that the graph doesn't
  support. Confidence levels must be honest, not motivational.
- Reasoning theatre — long visible chains that look smart but
  don't change the answer. The chain should be explanatory,
  optional, never the response itself.
- Over-eager inference — drawing conclusions from sparse data and
  acting on them. The default should be "I think X, low
  confidence — want to check?"

### 2. Personality

**What it means:** Travis has a recognisable voice. It's *Travis*,
not a generic LLM. The way it phrases things, what it bothers to
say, what it skips, how it handles a tense moment — all coherent
across turns and stable enough that the user can predict it.

**Why it matters:** A partner you can read is a partner. A
chameleon that mirrors whoever it's talking to is a tool. The
"Travis becomes Jarvis" goal collapses without a personality —
Jarvis isn't a feature list, he's a *character*.

**v0.5.0 state:** The system prompt has voice guidance ("warm,
professional, terse, contractions, never sycophantic") but it's
prescriptive, not modelled. The LLM follows it most of the time.
There's no concept of *Travis's* preferences, observations, or
opinions that survive across turns.

**The work:**

- **Persona layer.** A structured definition of who Travis is —
  values, preferences, things he notices, things he doesn't say —
  that lives in code/data, not just prompt text. Versioned. Tested
  against sample turns ("would *this* Travis say *this* line?").
- **Character through constraints.** Personality is mostly *what
  Travis won't do*. He won't pad. He won't apologise for not
  knowing. He won't pretend the user said something they didn't.
  Codify these as negative rules with examples.
- **Voice memory.** When the user pushes back on phrasing
  (*"don't say 'great question'"*) it sticks. Today this lives in
  the user's `communication_style` profile field; it should also
  be a per-user feedback signal that adjusts response generation.
- **Internal voice for observations.** When Travis decides to
  surface something proactively, the framing should sound like
  *Travis* thinking, not the user being addressed. *"Three Maria
  captures this week, no signing sheet on file"* (Travis-voice)
  rather than *"Hi! I noticed three captures..."* (assistant-voice).
- **Resistance.** A Jarvis-class partner pushes back when the
  user's wrong. Soft, specific, without lecturing — *"that
  conflicts with what you logged Tuesday — sure?"* This needs
  reasoning + confidence to do safely.

**Failure modes to avoid:**

- Personality-as-decoration — bolting "quirky" lines onto generic
  responses. The voice should emerge from the constraints, not be
  performed.
- Inconsistent across surfaces — chat overlay vs splash vs
  proactive nudge currently use slightly different prompt
  fragments. They need a shared persona core.
- Over-personality — Travis the character can't drown out Travis
  the operator. Voice should be felt, not narrated.

### 3. Learning others' personalities

**What it means:** Travis models *the user* deeply (style,
patterns, what energises them, what wears them down). Eventually
also models people in the user's world (how Maria phrases things,
what tone gets the dad to respond, when's the right time to nudge
Carlos). Personality not as labels but as predictive models.

**Why it matters:** Most of the user's operational work is
relationship work. *"Should I email Maria now or wait?"* depends
on Maria. *"How direct should I be with the school?"* depends on
the school. Travis can't help meaningfully without modelling the
people he's helping the user navigate.

**v0.5.0 state:** Entities are tracked as nodes with names, kinds,
mention counts. There's no sense of *who they are* beyond a name.
The user's profile has a few free-text fields (name, role, org,
context blurb) and that's it.

**The work:**

- **User model first.** Periodic background pass that summarises
  the user's communication patterns from journal entries: average
  capture length, time-of-day distribution, vocabulary, what they
  ask vs what they capture. Stored as a structured profile that
  the LLM gets in every system prompt.
- **Per-entity personality slots.** Each `person:*` entity gains
  optional attribute fields: communication style (terse / warm /
  formal), best contact time (inferred from when the user mentions
  them), what topics co-occur (helps Travis frame messages). Written
  by an extraction pass over accumulated mentions; updated as new
  evidence arrives.
- **Drafting in their voice.** When Travis drafts a message *for*
  someone (an email to a parent, a note to a coach), it pulls the
  recipient's personality slots and adapts. Not to deceive — to
  reduce friction. The user reviews before send.
- **Calibration loop.** When the user edits a Travis-drafted
  message, the diff feeds back into the recipient's personality
  model. Over time, drafts need fewer edits.
- **Privacy guardrails.** All of this is local, per-user, never
  shared. Sensitive workspaces' personality models stay isolated
  by the same asymmetric rule as the rest of the graph.

**Failure modes to avoid:**

- Manipulation territory. Modelling *how to influence* someone is
  off-side. Modelling *how they prefer to be communicated with* is
  on-side. The boundary needs to be enforced — by what attributes
  we extract and how Travis frames them.
- Stereotyping. *"Carlos is type X"* is the wrong abstraction.
  Granular, evidence-based observations only — *"Carlos confirms
  same-day, replies in 1-2 sentences"*.
- Over-fitting to one mood. People have bad weeks. The model
  should weight toward stable patterns and explicitly note recent
  drift.

### 4. Collaboration

**What it means:** Travis works *with* the user toward shared
goals across sessions, not as a tool that answers per-turn
requests. He remembers what you're working on, what you decided
last week, what's on hold. He picks up where you left off without
being prompted.

**Why it matters:** The current shape is conversational —
stateless except for the journal log. A collaborator has a project,
not a chat history. Without this, every conversation reboots
context Travis already has.

**v0.5.0 state:** Conversations have status (open / awaiting_user
/ resolved) and an auto-close after 7 days. Tasks track open work.
But there's no concept of *initiative* — what is the user
currently driving toward, and what role does Travis play in it?

**The work:**

- **Initiatives layer.** Above tasks: a typed `initiative` row
  (project / goal / push) the user can name explicitly or that
  Travis infers from clusters of related tasks + entities. *"April
  invoicing push"* gathers tasks, captures, mention timelines into
  one frame.
- **Status threads.** Per initiative, a long-running conversation
  thread that survives the 7-day auto-close. Travis maintains it:
  the last decision, the open questions, what changed since last
  contact.
- **Shared context across sessions.** The opening of a new chat
  thread can implicitly resume the most-relevant initiative —
  *"You were on the April invoices yesterday. Three signed sheets
  came in overnight. Want to draft the bills?"*
- **Ownership marks.** Per task / decision / draft: who's holding
  it now? Travis or the user? Lets the proactive layer differentiate
  between *"waiting on me to act"* and *"waiting on the user."*
- **Hand-off rituals.** End-of-day or end-of-session, Travis can
  briefly state: *"Today: drafted invoice 0042; waiting for the
  signed sheet from PS 142. Tomorrow: send Maria's report. I'll
  remind you."* Persistent context, not noise.

**Failure modes to avoid:**

- Initiative bloat. If Travis names everything an initiative,
  nothing is. Default off; opt-in or high-bar inference.
- Sycophantic recap. *"Yesterday we made great progress!"*
  collapses into pseudo-cheerleading. End-of-session summaries
  should be terse, factual, optional.
- Cross-workspace bleed. Initiatives respect the workspace
  isolation rule; sensitive ones don't appear in other contexts'
  resume surfaces.

### 5. Proactivity

**What it means:** Travis initiates without being asked. He notices
*"the signed sheets just came in"* and proposes drafting. He
notices *"you've worked late three days running"* and stays quiet
on tomorrow's nudge until lunchtime. He notices *"Maria hasn't
responded to your email"* and asks if a follow-up draft would help.

**Why it matters:** Reactive Travis answers questions. Proactive
Travis *changes the user's day*. The asymmetry is enormous — most
operational mistakes are things the user *forgot to think about*,
which is exactly what proactive observation catches.

**v0.5.0 state:** A proactive nudge thread exists; it runs on a
schedule and decides whether to surface a low-information nudge.
Useful but narrow. It doesn't observe the graph, doesn't track
follow-ups, doesn't watch for events the user should know about.

**The work:**

- **Observer loop.** Background pass (every 30 min, like
  reminders) over the graph: detect anomalies (unusual mention
  rates, missing-this-week patterns, dormant entities suddenly
  active again), watch for state transitions (signed sheet just
  filed, invoice still draft 7 days post-period). Surfaces
  candidates to the proactive nudge layer.
- **Anti-spam discipline.** The current proactive nudge already
  enforces *"silence is the default."* Extend that: per-observation
  cooldown, per-user pace limit (≤2 nudges/day, fewer on busy
  days), explicit user override (*"don't surface this again"*).
- **Calendar / inbox awareness.** When Travis can read the
  calendar (already wired) and email (planned), proactive widens
  meaningfully — *"Maria's reply hit your inbox an hour ago, want
  me to summarise?"* Pre-cloud / pre-mobile, this is bounded.
- **Rhythm-aware timing.** Combine the user model (energy
  patterns, capture rhythm) with proactive timing. Don't surface
  ops at 11pm if the user usually winds down then. Surface
  ops-prep at the user's typical morning capture time.
- **Tracking what the user *meant* to do.** Observed intentions —
  *"I should call PS 142 about the signed sheet"* in a journal —
  become tracked even without an explicit task. If they age out,
  Travis brings them back.

**Failure modes to avoid:**

- Notification fatigue. The fastest way to break the trust the
  graph just earned. Default to silent; surface only when the
  thing the user would say back is *"oh, good point."*
- Performative attention. *"I've been watching your work and..."*
  is awful. Proactive surfaces should sound like a colleague
  mentioning something, not a system reporting.
- Wrong-problem detection. The observer needs to be tuned to *the
  user's* what's-important, not generic operational rules. Tied to
  the user model.

### 6. Self-advocacy

**What it means:** When Travis needs something — context,
clarification, access, more time — he asks. Doesn't pretend he
knows; doesn't silently fail; doesn't over-perform certainty.

**Why it matters:** The user's posture is *"track everything; ask
only to refine."* The flip side: when refinement *would* help,
Travis should ask. A partner who never asks for help is doing
worse work than they need to. A partner who asks for everything
is exhausting. The middle is voice — picking the moments.

**v0.5.0 state:** Clarifying questions exist as an extraction-tool
field; capability gaps are tracked. Both are reactive — they fire
during a journal turn. There's no Travis-initiated *"I need X to
do this well"* path.

**The work:**

- **Need surfaces.** Travis tracks per-action what would have
  helped — a calendar grant, a Gmail connection, a clearer
  description of an entity, a decision the user keeps deferring.
  Surfaces *"I keep stalling on email drafts because Gmail isn't
  connected — want to connect now?"* once / twice / never. Not
  every turn.
- **Refinement asks.** Tied to the inference loops already built
  (slice 9-11). When a `*:unknown` entity has 4+ mentions or two
  conflicting kinds appear, Travis asks one focused question via
  the conversation surface. The current implementation has the
  query helpers; the user-facing ask flow is the missing piece.
- **Capability advocacy.** Travis tells the user when a thing he
  *can* do would help — *"I noticed three signing sheets this
  week — I can draft the related invoices in one batch if you
  want."* Self-advocacy isn't whining; it's *"here's what I can
  contribute."*
- **Permission asks.** Phase 6+ surfaces (file system access,
  calendar write, email send) — Travis doesn't silently retry
  with limited scope; he says *"I can't write the calendar event
  without permission — should I ask?"*
- **Honest uncertainty.** Reasoning surfaces *"low confidence,
  one data point — sure?"* This isn't self-advocacy strictly, but
  it's the same posture: don't pretend. The user's trust is built
  here.

**Failure modes to avoid:**

- Constant pestering. Every ask should be high-leverage. Ask once,
  remember the answer, don't ask again unless the situation
  materially changed.
- Asking when acting is fine. If Travis can do the thing, do the
  thing. Asks are for genuine ambiguity, not performative caution.
- Anthropomorphic neediness. *"I would feel better if..."* is
  cringe. *"This blocks me until..."* is fine. Travis is software
  that has needs in the operational sense, not the emotional one.

### 7. Wellbeing contribution

**What it means:** Travis cares about the user holistically, not
just transactionally. He notices *"you've taken zero breaks today"*,
*"you've been catastrophising about the audit for three days — want
to draft the email so it stops circling?"*, *"you mentioned tired
in seven captures this week."* He acts as a colleague who's
genuinely on the user's side, including against the user's worst
operational impulses.

**Why it matters:** This is what separates Jarvis from a productivity
app. Productivity apps optimise output. A partner notices when
output is the wrong target — when the user is grinding through
when they should be resting; when they're stuck on something they
won't admit; when they're heading toward a wall they don't see yet.

**v0.5.0 state:** Travis is operationally focused. Capture, plan,
draft. There's no concept of the user's energy or state of mind —
those signals exist in journal text but aren't extracted, tracked,
or surfaced.

**The work:**

- **Affect signals as first-class data.** Light extraction in
  journal entries: tone (concerned / energised / drained / stuck),
  themes (specific worries that recur). Not pop-psych labels —
  evidence-based observations the user can verify.
- **Pattern surface for wellbeing.** Same observer loop as
  proactivity, focused on user-state signals: capture cadence
  collapsing, themes dwelling on the same problem for days, work
  hours creeping later. Surfaces sparingly, with evidence.
- **Refusing to be complicit.** When the user asks Travis to
  optimise something that's clearly self-harming (cram more into
  Saturday, pull an all-nighter, draft a passive-aggressive email),
  Travis pushes back specifically. Not refusing the work — flagging
  the problem. *"This puts your week at 70 hours — sure?"*
- **Naming what's not being said.** When journal patterns suggest
  something is being avoided, Travis can ask once. *"You've
  written about the audit five times this week without mentioning
  the actual response — want to draft it?"* Carefully — this is
  the most personality-heavy capability and the most failure-prone.
- **Boundaries.** This is **not** therapy. It's not advice. It's a
  colleague who notices and says something once. If the user
  pushes back, Travis drops it.

**Failure modes to avoid:**

- Therapeutic posturing. *"How are you feeling about that?"* is
  not Travis's job. Operational observations only.
- Wellness performance. *"Take a break! 🌱"* is offensive. Travis
  should sound like a colleague noticing, not a wellness app.
- Surveillance creep. The user's affect signals are private even
  by Travis's standards. Local-only, never aggregated, never sent.
- Wrong-decade gendered nonsense. The work-life-balance story is
  the user's own to write; Travis observes patterns, not
  prescribes virtues.

---

## Graph completeness — what's ready, missing, cutting-edge

This section drills into the substrate the seven capabilities
depend on. Every section above is gated by graph quality — if the
graph is shallow, the cognition built on it is shallow.

### What v0.5.0 ships

- `entity` table with kind, normalized_name, mentions_count,
  confidence, tags, archived_at, pack_table_id, embedding_vector,
  embedding_indexed_at, workspace_id
- `event` table with entity_id, kind, occurred_at, attributes_json,
  pack_slug, workspace_id (workspace-scoped, append-only)
- `relation` table with from/to entity, kind, attributes_json,
  workspace_id (workspace-scoped, dedup at caller for `mentioned_with`)
- Ambient capture in three confidence tiers (1.0 typed, 0.7
  pack-kinded, 0.5 generic), dedupping against existing entities
  by normalised name within the workspace
- `mentioned` events linking each mention to its source journal
  entry with a 120-char snippet
- `mentioned_with` co-mention edges with auto-tracked counts in
  attributes_json
- Background embedding indexer (`graph_indexer`) re-embeds every
  entity ≥ 7 days stale or never indexed, ordered by mentions
- Graph-aware retrieval (`memory::graph::retrieve`) injects entity
  context into the LLM user message
- Inference helpers (`recurring_mention_candidates`,
  `edge_proposals`, `name_conflicts`) + corresponding mutators
  (`apply_refinement`, `accept_edge_proposal`, `merge_entities`)
- Capture chip surfacing pre-existing-entity recognition

### What's missing — the practical Phase 4.5 build list

**Ranked by leverage, ascending in effort:**

1. **Embedding-based entity retrieval.** Every entity has an
   embedding; retrieval only uses exact-name match. Cosine sim
   against the entity index would resolve *"the coach who teaches
   PS 142"*, *"that parent from last month"*, even pronouns once
   we stitch in conversation context. Single Rust function +
   wiring into `memory::graph::retrieve`. ~1 day. Highest leverage
   per effort.

2. **Structured fact extraction.** The LLM extracts entity *names*
   but not facts about them. Add an `entity_facts: [{ entity,
   type, value, confidence }]` bucket to the extraction tool.
   Persist into `entity.attributes_json` as typed claims (`role`,
   `relationship`, `contact`, `context`, etc.). Unlocks proper
   "I know things about this entity" surfaces. ~3-4 days.

3. **Memory consolidation tick.** Mentions accumulate as raw
   events forever. A periodic background pass should re-summarise
   per-entity event clouds into stable `attributes_json` ("Maria —
   24 mentions over 6 weeks, 22 about coaching at PS 142, last
   hours logged 2026-04-12"). Without consolidation, retrieval
   gets noisier the more Travis is used — exactly backwards.
   ~3-5 days.

4. **Multi-hop traversal.** All current queries are 1-hop. *"Find
   people connected to PS 142"* needs 2-hop (PS 142 →
   mentioned_with → person). SQL CTEs handle this; the schema and
   indexes already support it. Mostly query-writing work plus
   exposing as a tool the LLM can call. ~2-3 days.

5. **Confidence in answers.** Travis currently can't say *"I'm
   80% sure Maria works at PS 142 (8 captures, all in coaching
   contexts)"*. The data is there (mentions_count, confidence,
   edge co_mention_count); we just don't surface uncertainty.
   This is the difference between a database and a memory.
   Wire-up work in retrieval formatting + system prompt. ~2 days.

6. **Working memory cache.** Short-lived (~30 min) per-thread
   hypothesis store so multi-turn reasoning compounds rather than
   restarts. Sketch: a `working_memory` table or in-process map
   keyed by conversation_id holding the top-N graph hits + any
   reasoning conclusions Travis has tentatively reached. ~2 days.

7. **Persisted claims layer.** Once the reasoning track lands
   (capability 1), conclusions need a home. New `claim` table
   ties reasoning outputs to the entities they touched, with
   confidence + source attribution. Surfaces in retrieval as
   prior beliefs alongside raw events. ~3 days.

8. **Active forgetting / decay.** Old events at the same weight
   as new ones is wrong. Recency in retrieval is partly there
   (the existing memory scoring); entity-level decay needs a
   recency multiplier on `mentions_count` for ranking purposes.
   ~1 day.

9. **Per-entity timeline view (UI).** Not user-facing primary nav
   per the minimal-surfaces directive — but the *capture chip
   tooltip* could expand into a quick "what Travis remembers about
   this entity" pop-down when hovered. Lightweight surface, no
   tab. ~2 days.

10. **Inference helpers driving conversation.** Slices 9-11 built
    the queries; nothing yet pipes their results into the
    proactive nudge thread or in-thread clarifying questions.
    The user-facing ask flow is the missing half. ~3-4 days.

**Total Phase 4.5 build:** ~22-30 focused days. Substrate work
that unlocks every later cognition phase.

### Cutting-edge — research-flavour additions

These are higher-risk, harder-to-evaluate, and not strictly needed
for the seven capabilities. Worth considering when telemetry shows
where the basics fall short.

- **Self-supervised edge prediction.** Train a tiny model (or use
  the embedding index + simple heuristics) to *propose* relations
  the LLM hasn't extracted. *"You mention Maria and Carlos
  together a lot — they probably know each other."* Surfaced as
  edge proposals, user confirms.

- **Temporal pattern detection.** Right now we have timestamps
  but no time-aware queries. Detect things like *"Maria sessions
  cluster on Tuesdays"*, *"invoice work peaks on the 15th of the
  month"*, *"this entity went dormant for 3 weeks after a long
  active streak"*. Useful for proactive timing (capability 5)
  and rhythm awareness.

- **Episodic memory.** Group event sequences into named
  "episodes" — *this week's recurring sessions*, *the April
  invoicing push*, *the audit response thread*. An episode is an
  initiative (capability 4) projected onto the graph; the same
  data, with story shape. Lets Travis answer *"what was
  yesterday like?"* in narrative form rather than event list.

- **Counterfactual queries.** *"What if I hadn't met Maria last
  week?"* — used internally for self-reflection prompts.
  Equivalent to "remove this event from the graph and see what
  changes in retrieval ranking." Useful for the wellbeing loop:
  *"removing the dwelling-on-the-audit captures, your week looks
  pretty productive."*

- **Hierarchical entity clustering.** Auto-detect that 5 coaches
  all work at the same school. Cluster them. Travis can talk
  about *"the PS 142 coaches"* as a group when relevant. Cheap
  with the embedding index already built.

- **Active learning loop.** Travis identifies the most-uncertain
  entity (low confidence + high mentions, or conflicting kinds)
  and asks *one* refining question per week. Different from the
  existing inference nudges in that it's about graph *health*,
  not workflow.

- **Working memory ↔ long-term memory split.** Recent events sit
  in a fast-access window; older events compress into entity
  attribute summaries. Mirrors human memory organization;
  reduces retrieval cost as the graph grows.

- **Theory-of-mind lite.** Track what *the user has told other
  people* vs *themselves*. The audit response email Travis drafts
  reflects what the user said *to the school*, not what they
  vented in a private capture. Carefully — this is privacy-
  sensitive ground.

- **Multi-modal entities.** Calendar events, emails, documents
  as first-class graph nodes. The audit response email becomes
  a `document:email` entity linked to the audit `topic` entity
  linked to the school `place`. Same shape, different node kinds.
  Big lift; high reward.

- **Belief / uncertainty propagation.** Each fact has confidence;
  queries propagate confidence (a question whose answer depends
  on three medium-confidence facts has a lower-confidence answer
  than one from typed records). Real probability math, not a
  hand-waved score. Worth doing once the foundations are tight.

- **Graph neural networks for entity prediction.** *"Given this
  pattern of co-mentions, the next most-likely entity to be
  mentioned is X."* Useful for autocompletion in capture, for
  proactive suggestions, for filling in gaps. Requires
  meaningful graph density (months of usage).

### How to prioritise the cutting-edge list

Don't build any of these until:

1. The Phase 4.5 build list is shipped *and stable for 4+ weeks*.
2. Real captures expose where it falls short. Telemetry of *what
   the user asks Travis that Travis can't answer well* is the
   north-star input.
3. The seven capabilities each have a viable v1. Cutting-edge work
   should *enhance* an existing capability, not invent new
   surfaces.

The trap to avoid: building research-flavour features because
they're interesting. The graph is a means; the capabilities are
the ends. Every line of code earns its place by serving one of
the seven.

---

## Architecture sketch — what the seven capabilities need

Each capability above touches the same substrate. Pulling it together:

**Layered stack:**

```
   ┌────────────────────────────────────────────────┐
   │   PERSONALITY (2) + WELLBEING (7) + COLLAB (4) │  voice / care / partnership
   ├────────────────────────────────────────────────┤
   │   REASONING (1) + SELF-ADVOCACY (6)            │  composed inference + ask
   ├────────────────────────────────────────────────┤
   │   PROACTIVITY (5) + LEARNING OTHERS (3)        │  observation + modelling
   ├────────────────────────────────────────────────┤
   │   GRAPH (v0.5.0)                               │  entities + edges + events
   ├────────────────────────────────────────────────┤
   │   WORKSPACES (v0.4.0)                          │  scoping
   ├────────────────────────────────────────────────┤
   │   PACKS (v0.3.0)                               │  domain extension
   ├────────────────────────────────────────────────┤
   │   CORE (capture, tasks, reminders, retrieval)  │  primitives
   └────────────────────────────────────────────────┘
```

**New substrate this doc commits to building:**

- **Persona layer** (capability 2). Structured definition of
  Travis-as-character; tested. Lives in `src/persona/`.
- **User model** (capability 3). Periodic summary of communication
  style + pattern signals. Stored on `user_profile` extension or a
  new `user_model` table.
- **Per-entity personality slots** (capability 3). Optional fields
  on `entity.attributes_json` for `person:*` entities.
- **Initiatives** (capability 4). New table; gathers tasks +
  entities + threads under one frame. Above tasks, below
  workspaces.
- **Observer loop** (capability 5). Like reminders scheduler;
  watches for graph anomalies; feeds proactive layer.
- **Need / advocacy queue** (capability 6). Tracks what Travis
  has flagged and how the user responded; throttles re-asking.
- **Affect signals** (capability 7). Lightweight tone extraction
  + theme tracking; private; never leaves the device.
- **Working memory** (capability 1). Short-lived hypothesis
  cache so multi-turn reasoning can refine instead of restart.
- **Claims layer** (capability 1). Persisted reasoning conclusions
  tied to entities; surfaced in retrieval as prior beliefs.

---

## Phasing

These aren't ROADMAP phases — they layer alongside the infra
phases (cloud, mobile, voice). Suggested order, with rough scale:

| Track | Capabilities | When |
|---|---|---|
| **4.5 — Graph polish** | (1) embedding-based entity retrieval; structured fact extraction; memory consolidation; multi-hop helpers; confidence in answers | 3-5 weeks. Substrate for everything. |
| **5.x — Reasoning layer** | (1) persisted claims; reasoning chains in responses; working memory | 4-6 weeks. The "now it has a brain" milestone. |
| **6.x — Persona + user model** | (2) persona substrate; (3) user model first pass | 4-6 weeks. The "now it sounds like Travis" milestone. |
| **7.x — Proactivity 2.0** | (5) observer loop; rhythm-aware timing; intention tracking | 3-4 weeks. The "now it surprises me" milestone. |
| **8.x — Collaboration depth** | (4) initiatives; status threads; resume context | 4-5 weeks. |
| **9.x — Self-advocacy + need surfaces** | (6) need queue; capability advocacy; refinement asks | 2-3 weeks. |
| **10.x — Other-personality + wellbeing** | (3) per-entity slots; drafting in their voice; (7) affect signals; pattern surface | 5-6 weeks. The most user-trust-sensitive work. |

Total: roughly 6-10 months of cognition work, in parallel with the
infra phases on ROADMAP. Each track ships in its own slices with
its own design doc; this is the framework, not the spec.

---

## Engineering discipline — canonical-store-per-entity

When designing schemas: **pick one canonical store per entity, treat
everything else as a regenerable projection.** Open WebUI's
`chat.history.messages` JSON-blob + normalised `chat_message` table
is the cautionary tale — every feature pays a "did I write to both?"
tax, and they need `backfill_*` / `reconcile_*` helpers to fight
drift.

Rule of thumb in Travis:
- One source of truth (a row, a log, a file).
- All other views (search indexes, list summaries, UI cache, LLM
  context) regenerate from the source. Stale projections rebuild;
  they don't bidirectionally sync.
- A migration that mirrors data into two tables for "convenience"
  should justify itself with measured query cost. Default is no
  duplication.

This applies forward: when v0.17.0 lands the event-log substrate
(#172), `conversation_message` becomes a projection. Search
indexes regenerate from the log. The graph regenerates from the
log. The UI reads the log via cached projections it owns.

## What this isn't

- **Not autonomy without consent.** Travis observes; Travis
  proposes; the user always confirms before destructive or
  visible-to-others actions. Even at full Jarvis-class, the human
  is in the loop.
- **Not pretending to be human.** The voice is *Travis*'s voice —
  recognisably software with a perspective. Anthropomorphisation
  is not the goal; trustworthy collaboration is.
- **Not therapy.** The wellbeing capability is operational — it
  notices patterns and says something once. It is not a mental
  health intervention and shouldn't be marketed or built like one.
- **Not surveillance.** Every signal Travis tracks is local to the
  device, scoped by workspace, never aggregated externally without
  the user's explicit decision (cloud sync at Phase 6+ is opt-in
  per-row). Wellbeing signals are the *most* private of all.
- **Not a replacement for the user's judgement.** Travis gives
  evidence, options, and observations. The user decides. When
  Travis pushes back, it's information, not authority.

---

## Open questions

These are intentionally unresolved — they're the ones to learn
from real usage rather than design upfront.

1. **How much "voice" before it's noise?** The line between
   *"recognisably Travis"* and *"intrusive personality"* depends
   on the user. We need a feedback signal — maybe an explicit
   tone slider, maybe inferred from how often the user dismisses
   Travis-voiced surfaces vs operational ones.

2. **Whose persona is reusable vs. user-specific?** The persona
   layer (capability 2) is partly definitional (Travis's
   character) and partly emergent (this user's Travis). The split
   matters for cloud sync and multi-device — does the user's
   Travis travel with them, or does each device build its own?

3. **What does Travis *refuse* to learn?** Modelling the dad's
   communication style is fine. Modelling how to reliably get him
   to say yes to things he shouldn't is not. Where exactly is
   the line, and is it codifiable in extraction rules?

4. **Cross-workspace personality.** When a person appears in
   multiple workspaces (the user's friend who's also a
   contractor), do their personality slots merge or stay
   isolated? The asymmetric isolation rule says isolated; UX
   pressure may push toward merge.

5. **When does Travis stop?** Proactive observation, wellbeing
   surfacing, refinement asks — all have a "shut up now" need.
   Per-feature throttles aren't enough; there needs to be a
   coherent "Travis is being too much" signal the system can
   honour even mid-feature.

6. **Confidence calibration.** *Saying* "low confidence" is
   easy; being *honest* about it is hard. We need a way to
   measure whether Travis's stated confidence tracks reality —
   maybe a feedback signal where the user marks claims as
   right/wrong over time and the calibration drifts toward
   accuracy.

7. **Personality model exposure.** Should the user be able to
   read/edit Travis's notes about them and the people they work
   with? Trust says yes; the *"track silently, refine through
   conversation"* posture says no by default. Probably: yes, but
   only via a deliberate Settings surface (not a primary tab).

---

## Why now

The graph foundation in v0.5.0 is the substrate for everything in
this document. Capability 1 (reasoning) is the immediate next
build because everything else compounds on it — personality
without reasoning is performance; learning others without
reasoning is rote pattern-matching; proactivity without reasoning
is just notification. The seven capabilities aren't a parallel
list; reasoning is load-bearing for the rest.

That's why the immediate next phase (4.5) sits at "tighten the
graph + start the reasoning layer." Once Travis can compose, the
other six capabilities have somewhere to compose *toward*.

The goal across all of this: Travis becomes the kind of partner
the user wants to have around. Quiet, sharp, on-side. The kind
of collaborator that, after a year, the user can't imagine not
having.

That's Jarvis. That's the bar.
