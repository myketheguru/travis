# Travis Workflow Framework — Capability Backlog

The workflow framework shipped in v0.12 (Slice 1 of the docs/workflows substrate) is intentionally narrow — it covers Taylor's LTE-pack workflows (invoice generation) end-to-end but assumes a particular shape: linear slot-filling, document/entity inputs, local DB finalize action.

That shape doesn't cover the full space Travis core needs to drive if we're scaling horizontally beyond EDU. This document enumerates the capabilities core needs so any pack (today or future) can declare any kind of workflow without forking the framework.

**Status legend:** ✅ shipped · 🟡 partial · ⬜ not built.

---

## 1. Slot kinds — what a workflow can ask for

| Kind | Status | Notes |
|---|---|---|
| `Text` | ✅ | Free single-line text. |
| `Date` / `DateRange` | ✅ | ISO 8601. |
| `Entity { kind }` | ✅ | Existing graph entity. |
| `Document { kind }` | ✅ | PDF / file linked to the workflow. |
| `Money` / `Number` | ✅ | Cents int / float. |
| `LongText` | ⬜ | Multi-paragraph free text (memo body, email content). |
| `Boolean` | ⬜ | Yes/no toggles. |
| `MultiSelect { options }` | ⬜ | Pick N from a finite list. |
| `SingleSelect { options }` | ⬜ | Pick exactly one from a finite list. Distinct from Entity. |
| `Person { kind: 'internal' | 'external' }` | ⬜ | Distinct from Entity — first-class person concept with contact metadata. |
| `Email { mailbox? }` | ⬜ | Existing thread or drafted message. |
| `CalendarEvent` | ⬜ | Event reference (existing or to-be-created). |
| `Url { domainHint? }` | ⬜ | A web source — for research workflows. |
| `WebSearchResult { query? }` | ⬜ | A scraped/searched web hit, with citation. |
| `File { mimeTypes }` | ⬜ | Arbitrary file (image, audio, spreadsheet) — not just PDF. |
| `ToolResult { tool }` | ⬜ | Slot filled by invoking a specific Travis tool (e.g. `calendar.find_availability`). |
| `Composite` | ⬜ | A nested slot group — for slot trees beyond flat lists. |

## 2. Workflow shapes — beyond linear slot-filling

| Shape | Status | Notes |
|---|---|---|
| Linear required-then-optional | ✅ | Today's invoice flow. |
| **Branching** | ⬜ | Slot order depends on prior answers ("if amount > $X, route through approval; else skip"). |
| **Loops** | ⬜ | Gather N items until user says "done" — e.g., line items on a quote, attendees on an event. |
| **Parallel slots** | ⬜ | Ask multiple independent slots in one turn instead of one-at-a-time. |
| **Optional groups (oneOf)** | ⬜ | "Either fill group A or group B" — e.g., recipient is either an existing contact OR a new email address. |
| **Sub-workflows** | ⬜ | A workflow can spawn another workflow as a slot resolver — "to fill `competitors`, run the `research_competitors` workflow." |
| **Workflow stacking** | ⬜ | More than one active workflow per conversation; user can navigate between. Today: one-at-a-time, new one abandons old. |
| **Long-running workflows** | ⬜ | Spans days/weeks with check-ins (job search, project arc, learning topic). State persists; surface re-engagement nudges. |
| **Reactive workflows** | ⬜ | Triggered by external events (incoming email arrives → triage workflow), not user intent. |
| **Multi-actor workflows** | ⬜ | Requires input from someone else mid-flow (approval from Jacob, info from a school principal). Today assumes single user. |

## 3. Slot resolution strategies — auto-fill before asking

| Strategy | Status | Notes |
|---|---|---|
| User typed in chat | ✅ | |
| User dropped a document | 🟡 | Wired in workflowOps shape; ingest pipeline is Slice 2. |
| Extracted from a document | 🟡 | Same — slice 3. |
| Graph query / prior-context match | 🟡 | LLM can do this today via existing tools; not formalised as a slot resolver. |
| **Tool invocation** | ⬜ | Slot says "fill me by calling `calendar.find_availability` with these args." |
| **External API lookup** | ⬜ | Enrich a person slot with LinkedIn / company DB / etc. |
| **LLM inference from conversation context** | ⬜ | Slot value derived from earlier turns without explicit ask. |
| **Default values / templates** | ⬜ | Slot pre-fills from user profile / workspace defaults / last similar workflow. |
| **Recursive sub-workflow** | ⬜ | Already mentioned above; mechanics here. |

## 4. Finalize action varieties

| Variety | Status | Notes |
|---|---|---|
| Local DB action (current `ActionHandler` path) | ✅ | |
| **External API call** | ⬜ | gmail.send, calendar.create, slack.post, stripe.charge. Permission gates per ROADMAP.md autonomy class. |
| **Multi-action finalize** | ⬜ | Compose several actions atomically — "send the email AND create the calendar event AND log the task." Rollback if any fail. |
| **Conversational only (no action)** | ⬜ | The workflow output is the synthesized response — reflection prompts, decision logs, summaries. No DB write at finalize. |
| **Workflow → workflow handoff** | ⬜ | Finalize of workflow A starts workflow B (e.g., draft invoice → send invoice). |

## 5. Lifecycle hooks

| Hook | Status | Notes |
|---|---|---|
| `on_start` | ⬜ | Validate prerequisites, set defaults. |
| `on_slot_filled` | ⬜ | Cascade-fill related slots, trigger validators. |
| `on_complete` | ⬜ | Postscripts, notifications, sub-workflow launch. |
| `on_abandon` | ⬜ | Cleanup, draft-save ("you walked away from the invoice — want a draft saved?"). |
| `on_timeout` | ⬜ | After N hours of inactivity, prompt or auto-abandon. |

## 6. UX integration

| Capability | Status | Notes |
|---|---|---|
| Confirmation card at finalize | ✅ | Existing `Applied` path. |
| **Confirmation cards mid-workflow** | ⬜ | Validate a slot value before continuing — "I extracted 29.5 hours from your sheet, confirm?" |
| **Progress indicator** | ⬜ | "3 of 5 slots filled" in the chat UI. |
| **Cancel / pause / resume controls** | ⬜ | First-class UI for workflow lifecycle. Today: only abandonment via LLM detection. |
| **Selection chips for slot options** | ✅ (chip parser) | Markdown convention exists; not yet tied to workflow slot resolution. |
| **Document drop affordance** | 🟡 | Slice 2. |
| **Workflow history view** | ⬜ | "Show me what invoices we've generated in the last month" — list of completed workflows. |

## 7. Workflow discovery

| Capability | Status | Notes |
|---|---|---|
| LLM emits `start` with recipe name from catalog | ✅ | Today's pattern. |
| **Travis proactively offers a workflow** | ⬜ | "Looks like you just signed a sheet — want me to run the `generate_invoice` workflow?" (proactive thread triggers a workflow.) |
| **Workflow templates editable per user** | ⬜ | Taylor customises `generate_invoice` for her quirks; Jacob's instance has a different version. |
| **Per-user recipe variations** | ⬜ | Same recipe name, different slot list — recipe is per-workspace or per-user. |
| **Recipe versioning** | ⬜ | In-flight workflows survive when a recipe updates (slot rename, new optional slot). |

## 8. Permissioning + autonomy

| Capability | Status | Notes |
|---|---|---|
| Finalize action requires confirmation card | ✅ | Existing `Applied` path. |
| **Per-workflow autonomy class** | ⬜ | ROADMAP.md autonomy classes: read / write-local (often auto) / external-action (usually confirm) / irreversible (always confirm). Each workflow declares which class its finalize lives in. |
| **Per-slot guards** | ⬜ | A workflow might require human review of a particular slot even when the workflow as a whole is auto. |
| **Audit trail per workflow** | ⬜ | Append-only signed log of every slot fill, tool call, finalize. ROADMAP.md "trust + audit" layer. |
| **Reversibility** | ⬜ | Every finalize is undoable (or has an explicit "irreversible — confirm" gate). |

## 9. Composability

| Capability | Status | Notes |
|---|---|---|
| Workflows from multiple packs coexist | ✅ | Registry walks all enabled packs. |
| **Workflows emit events** | ⬜ | `workflow.completed { recipe, slots }` — other workflows listen and trigger. |
| **Cross-workflow slot sharing** | ⬜ | The school filled in workflow A is the default for workflow B in the same conversation. |
| **Conversation-scoped vs persistent slot values** | ⬜ | Some values are ephemeral (the date you mentioned); others are durable (the school's PO number pattern). |

## 10. Observability + safety

| Capability | Status | Notes |
|---|---|---|
| Why-did-this-workflow-start audit | 🟡 | `started_intent` captures user's words; not exposed in UI. |
| **Slot resolution rationale** | ⬜ | "This slot was filled by graph_resolved because we found an exact match on engagement_id=42." |
| **Tool calls within a workflow logged** | ⬜ | Audit trail per ROADMAP.md. |
| **Time per slot / per workflow** | ⬜ | Diagnostic — which slots take longest, which workflows stall most. |
| **Failure / abandonment analytics** | ⬜ | Which recipes get abandoned most often; which slots cause friction. Feeds recipe refinement. |

---

## Workflow categories core needs to support eventually

Beyond LTE-pack-shape ("documents in → entities → documents out → action"), Travis core should be capable of driving these workflow families:

### A. Document-output workflows (LTE shape) ✅
Invoice generation, quote drafting, contract drafting, sign-in sheet curation, work order generation, report assembly, compliance docs.

### B. Email / messaging workflows ⬜
Draft + send email response (with thread context), Slack / SMS / iMessage, reply by template, newsletter assembly, cold outreach sequences, meeting follow-ups.
*Needs:* Email slot kind, gmail/outlook external action, threaded conversation context as slot resolver.

### C. Scheduling workflows ⬜
Book a calendar event, find time across attendees, reschedule recurring, set office hours, create reminder series.
*Needs:* CalendarEvent slot kind, calendar.find_availability tool, multi-actor approval shape.

### D. Research workflows ⬜
Multi-source web research with citation, comparative analysis, person/company briefing, market scan, reading-list curation.
*Needs:* Url / WebSearchResult slot kinds, web search tool resolver, loop shape ("keep adding sources until done"), citation tracking.

### E. Decision workflows ⬜
Pros / cons elicitation, trade-off matrix, risk register, decision log entry, recommendation memo.
*Needs:* Composite / nested slots, branching shape, conversational-only finalize.

### F. Creative / writing workflows ⬜
Draft a doc from outline, edit/proofread, compress to one-pager, audience-adapt, pitch deck assembly.
*Needs:* LongText slot, iterative refinement (loop with critique), multi-output finalize (sections).

### G. Code workflows ⬜
Multi-step code change, bug investigation, code review prep, PR description, migration drafting.
*Needs:* File slot, tool resolvers (read/grep/edit), branching shape, reversibility.

### H. Data workflows ⬜
Query construction, data validation, reconciliation, spreadsheet assembly, chart selection.
*Needs:* ToolResult slot, branching, multi-output finalize.

### I. Onboarding / setup workflows ⬜
Project onboarding, employee onboarding, form filling, account setup.
*Needs:* Long-running shape with check-ins, default/template resolver, branching.

### J. Project / task workflows ⬜
Goal → task breakdown, daily / weekly planning, retro / postmortem, sprint planning, OKR drafting.
*Needs:* Loop shape, sub-workflows, multi-output finalize (creates many tasks).

### K. Reflective / journaling workflows ⬜
Morning page, daily standup, mood / energy check-in, weekly review.
*Needs:* Conversational-only finalize, recurring trigger, optional-slot-heavy shape, [[feedback-wellbeing-privacy]] for affect data.

### L. Procurement / shopping ⬜
Vendor comparison, RFQ generation, quote tracking, vendor selection.
*Needs:* Research workflow primitives + Decision workflow primitives.

### M. Travel ⬜
Trip plan, expense report, visa renewal.
*Needs:* Multi-step external API actions (flight booking, hotel, calendar).

### N. Sales / negotiation ⬜
Discovery call prep, proposal drafting, objection handling rehearsal, pipeline update, customer health check.
*Needs:* Research workflow primitives + Writing workflow primitives + Persistent CRM state.

### O. Customer support ⬜
Triage ticket, draft response, escalate per policy, follow up.
*Needs:* Reactive shape (event-triggered), branching, external API actions.

### P. Marketing / content ⬜
Content brief → outline → draft → publish, campaign assembly, A/B test setup, social variants.
*Needs:* Writing + Multi-output + Sub-workflow primitives.

### Q. Hiring ⬜
Role spec → JD draft → interview loop, reference check, offer letter, candidate scorecard.
*Needs:* Long-running shape, multi-actor approval, Writing primitives.

### R. Finance / bookkeeping ⬜
Reconcile transactions, categorise expenses, budget variance, forecast assembly, tax prep.
*Needs:* Data workflow primitives + external integrations (Plaid / bank APIs / accounting software).

### S. Health (sensitive workspace) ⬜
Symptom log, med tracking, doctor visit prep, insurance claims.
*Needs:* Wellbeing-grade privacy posture, sensitive-workspace isolation, HIPAA-grade audit (Phase 6 in ROADMAP.md).

### T. Learning ⬜
Course outline, study session, quiz / flashcard generation, knowledge gap analysis.
*Needs:* Research + Writing + Sub-workflow primitives.

### U. Personal / life ops ⬜
Gift list, event planning, move / relocation, home maintenance.
*Needs:* Multi-output, long-running, recurring.

### V. Legal / contract ⬜
Contract review checklist, redline assembly, term sheet generation, NDA drafting.
*Needs:* Writing primitives + Decision primitives + sensitive-workspace posture.

---

## Sequencing principle

Build the **slot kinds and workflow shapes** that unblock the most workflow categories first. Highest-leverage adds (in order):

1. **LongText slot + Email slot + Conversational-only finalize** — unlocks categories B, E, F, K (email, decisions, writing, reflection). Roughly half the backlog.
2. **Tool-result slot resolver + branching shape** — unlocks D, G, H, L (research, code, data, procurement).
3. **Loop shape + Multi-output finalize** — unlocks J, P, Q (project mgmt, marketing, hiring).
4. **Reactive trigger + Multi-actor approval** — unlocks O (customer support) and any workflow needing external sign-off.
5. **Sub-workflows + Long-running state** — unlocks I, M, T, U (onboarding, travel, learning, personal).

The first item alone makes Travis useful for ~50% of the workflow categories above. Worth scheduling right after Slice 6 of the docs/workflows substrate lands.

---

## Related design docs

- [BRAIN.md](./BRAIN.md) — the cognitive substrate the workflow framework rides on
- [ROADMAP.md](./ROADMAP.md) — phases, especially #5 (tool spec compiler) and #8 (action layer)
- [MARKET.md](./MARKET.md) — vertical packs and their workflow shapes
- [TRAVIS_DECK.md](./TRAVIS_DECK.md) — pitch shape (not a design doc but referenced for context)
