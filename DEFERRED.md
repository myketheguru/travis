# Deferred Backlog

Items intentionally cut from the Travis MVP. Listed here so we don't lose track. Each entry: what it is, why deferred, when to revisit.

---

## Mobile companion app
**What:** Chat-only mobile app (PRD §4.7) that can log hours, set reminders, view tasks.
**Why deferred:** Doubles platform surface area. Desktop overlay must prove the workflow first.
**Revisit when:** Desktop MVP has stable daily usage and a clear set of mobile-only moments (e.g., logging hours while leaving a school visit).

---

## Voice input/output
**What:** Speech-to-text capture into the overlay and TTS responses (PRD §12 explicitly excludes "full voice assistant").
**Why deferred:** STT/TTS adds platform-specific dependencies (Whisper, OS speech APIs) and a separate UX layer. Note: the overlay's animated "presence" motif is being designed *now* so it can react to voice amplitude later without a redesign.
**Revisit when:** Text overlay + journal extraction are mature; user wants hands-free logging.

---

## LangGraph / orchestrated agent system
**What:** Multi-agent orchestration with Task Execution / Validation / Ops Assistant / Learning agents (PRD §4.6).
**Why deferred:** A single tool-using LLM call covers MVP needs. Orchestration adds latency, cost, and debugging surface for a workflow we haven't validated yet.
**Revisit when:** We hit a workflow the single-call extractor can't handle reliably (likely multi-step approvals or cross-entity reconciliation).

---

## Multi-user / role-based access
**What:** Multiple operators, RBAC, shared state (PRD §10).
**Why deferred:** Built for a single COO. Adding auth/sync now would block shipping.
**Revisit when:** A second person at Lead to Empower wants their own Travis instance with shared visibility.

---

## A/B testing & gradual rollout
**What:** User-level feature flags, percentage rollouts, experiment tracking (PRD §9 "Advanced").
**Why deferred:** Single-user app — there's nothing to A/B against.
**Revisit when:** Multi-user lands.

---

## Code signing & notarization
**What:** Apple Developer ID notarization for macOS, Authenticode signing for Windows.
**Why deferred:** Costs money + requires cert management. Unsigned dev builds are fine for personal use.
**Revisit when:** Distributing to others, or OS gatekeeping becomes friction.

---

## Encrypted local storage at rest
**What:** Encrypt the SQLite DB on disk (PRD §10 lists as optional).
**Why deferred:** Local-only single-user data; OS disk encryption (BitLocker / FileVault) covers the threat model for now. API keys still go in OS keychain regardless.
**Revisit when:** Sharing a machine, or storing data subject to a contractual encryption requirement.

---

## Overlay drag-to-reposition (known bug)
**What:** The Cmd/Ctrl+J overlay window has a drag handle (top strip with grabber pill) that should let the user reposition the floating window. Tried both `data-tauri-drag-region` and explicit `getCurrentWindow().startDragging()` from a `mousedown` handler — neither initiates a native window drag on Windows 10.
**Why deferred:** Suspect it's a Tauri 2 + Windows 10 + transparent-window edge case (works on macOS in many examples). Also possible the explicit `core:window:allow-start-dragging` permission needs additional Tauri config we haven't surfaced yet.
**Revisit when:** Step 16 (CI/build) or sooner if it bothers daily use. Likely fixes: try `data-tauri-drag-region` on a non-transparent inner element, or upgrade to a newer Tauri 2.x with potential fixes.

---

## Invoice templates library
**What:** Bakeable, named invoice templates (per user note). MVP ships PDF export with one reasonable Lead to Empower / NYC DoF layout.
**Why deferred:** User explicitly flagged this as future work. Need real invoice samples to design the template system.
**Revisit when:** PDF export is in use and user has 2-3 distinct invoice formats they switch between.

---

## Tauri updater signing keys (BLOCKING for first release)
**What:** Generate a Tauri updater keypair and wire it through GitHub Actions and `tauri.conf.json`. The OTA pipeline (PRD §4.8 / §7 / §8) and the workflows in `.github/workflows/build.yml` + `.github/workflows/release-update-json.yml` are in place but cannot ship a signed release without these.
**Why deferred:** Key generation is a one-time human step that produces material we never want in source control.
**Steps to do before first `vX.Y.Z` tag:**
1. Run `npm run tauri signer generate -- -w ~/.tauri/travis.key` (or the standalone `tauri signer generate`). Save the resulting **public** key.
2. Replace the `pubkey` placeholder in `tauri.conf.json` → `plugins.updater.pubkey` with that public key.
3. In the GitHub repo, add two Actions secrets:
   - `TAURI_PRIVATE_KEY` — contents of the generated private key file.
   - `TAURI_KEY_PASSWORD` — the passphrase used during generation (empty string if none, but a passphrase is recommended).
4. Confirm GitHub Pages is enabled for the repo on the `gh-pages` branch so `https://leadtoempower.github.io/travis/update.json` resolves.
5. Optionally point the `flags_url` and updater `endpoints` at a different host (`set_flags_url` / edit `tauri.conf.json`) if you don't want the placeholder GitHub Pages URL.
**Revisit when:** Cutting the first tagged release. Until then `tauri-action` will fail to produce signed updater artifacts.

---

## Travis tools — execution capabilities (next major arc, DO before agentic-phase-completion)
**What:** Travis can today *propose* actions and *capture* operations. It can't yet *execute*. Each tool below is its own implementation chunk with its own auth/safety story.

**Architecture (shared):** A `Tool` trait in `src-tauri/src/tools/` with `name`, `description`, `params_schema`, and `execute(params, ctx)`. Travis's LLM call gets a list of available tools in the system prompt with their schemas. The LLM can request `tool_calls` in extraction; we surface confirmation cards (already in place via `proposed_action`), apply on confirm, append the tool result as an assistant message in the thread.

### 1. Web research
**Status:** `web_fetch(url)` ✅ shipped. `web_search(query)` deferred — needs a backend choice.

**`web_search` plan (after research, April 2026):**
- **Primary: self-hosted SearXNG** — Docker compose (searxng + redis + caddy), runs locally on the user's machine. Hits Google/Bing/DDG/Qwant/etc. via metasearch and returns aggregated JSON. Configure `TRAVIS_SEARXNG_URL` env var (compile-time like telemetry); tool gracefully no-ops if unset. Need a one-time `docker compose up -d` step from a `searxng-compose.yml` we ship in the repo.
- **Fallback: Brave Search API free tier** — 2000 queries/month free, real API key, independent index. Compile-time `TRAVIS_BRAVE_KEY`. Tool tries SearXNG first, falls back to Brave on connect-fail.
- **Reader companion: Jina AI Reader** (`r.jina.ai/<url>`) — converts URLs to clean markdown, free tier. Optional: when search results are returned, batch-fetch top 3 via Jina for the LLM's context.
- **Skip:** DuckDuckGo HTML scraping (brittle, monthly breakage), Stract (beta API churn), Mwmbl (tiny index), Common Crawl (petabyte infra).
- Tool defs: `web_search(query, count?) -> [{title, snippet, url}]`. Source cards rendered in the thread with click-out buttons. Travis can chain `web_search` → pick a result → `web_fetch(url)` for deep read.

### 2. App-managed reminders/alarms (extend existing reminders + tauri-plugin-notification)
- Already have: `reminder` table, scheduler that fires `tauri-plugin-notification`. Currently silent.
- Add: customizable system sound (configurable in Settings; ship with 2-3 built-in chimes), priority levels for sound vs silent, snooze action.
- Reminders fire as native OS notifications with action buttons (Done / Snooze 10m / Snooze 1h).
- Cross-platform via the existing notification plugin.
- **Don't need OS-native alarm app integration** — this is sufficient for the COO use case.

### 3. Calendar integration (Google Calendar OR Outlook)
- User picks at first use; both via OAuth 2.0.
- Tool: `create_event(title, start, end, attendees, location)` and `list_events(range)`.
- OAuth flow: open browser to Google/Microsoft auth → redirect URI to `localhost:<random_port>` Tauri serves → exchange code → store refresh token in OS keychain.
- API calls: Google Calendar v3 / Microsoft Graph `/me/events`.
- Settings tab: "Calendar" section to connect/disconnect.
- Use `oauth2` crate for the flow.

### 3.5. Proactive nudges (next session)
**Goal:** Travis isn't only reactive. Periodically reviews state and surfaces a "you've got 3 things hanging — want help?" message via OS notification, even when the app is closed (tray icon ✅ already in place).

**Design:**
- New tokio task spawned in `lib.rs::setup`, runs hourly tick (initial 2-min delay).
- Each tick: check Settings toggle `meta.proactive_enabled` (default OFF — opt-in via Settings → Proactive). Skip if off.
- Throttle: don't fire more than once per 4 hours. Track last_at in `meta.proactive_last_at`.
- Optional waking-hours filter: 8am–10pm local via `chrono::Local`. Skip outside.
- Pull state for context:
  - Open tasks (especially overdue: `due_at < today`)
  - Active conversations with `awaiting_user` status (Travis is waiting on you)
  - Recent `journal_entry` activity (last 24h count)
  - Today's calendar events (if Google Calendar connected)
  - Top capability gaps from `app_feedback` (if any patterns)
- Build a "nudge prompt" that asks the LLM to either return one short nudge OR nothing (`response: null`). Use forced tool call `report_nudge` (one of: text, severity, kind: 'check-in' | 'overdue' | 'follow-up') matching a `proposed_action`-style structure.
- If a nudge is produced:
  - Open or extend a `nudges` conversation (kind='nudge') in `conversation`.
  - Append message with role:assistant.
  - Fire OS notification via `tauri-plugin-notification` with the text. Click → opens main window with Manage → Threads filtered to nudges.
- New Settings section "Proactive" with: enable toggle, cadence picker (hourly / 4hr / 8hr / daily), quiet hours range.
- Telemetry: `nudge_generated` event (kind, severity), `nudge_dismissed`.

**Cost note:** an opt-in 4hr cadence = 4-6 LLM calls/day = ~$0.05/day on Sonnet 4.6 with prompt caching. Reasonable for the value.

### 4. Send email (OAuth, not SMTP)
- We have lettre + SMTP infra but it's password-auth — fragile and a security footgun. Replace with provider OAuth.
- **Gmail:** OAuth + Gmail API `users.messages.send`. Same OAuth flow as calendar.
- **Outlook/365:** OAuth + Microsoft Graph `/me/sendMail`.
- Tool: `send_email(to, subject, body, attachments)` — preview card before sending; user confirms → fires.
- The PDF-invoice-attachment flow (`send_invoice_email`) gets reworked to use this instead of SMTP.

### 5. Shell / computer interaction (DANGEROUS — last and behind a flag)
- Tool: `run_shell(command, working_dir?, timeout_s?)` — executes a shell command on the user's machine.
- Hard requirements before shipping:
  - **Default OFF.** Settings must explicitly opt in with a "I understand the risk" toggle.
  - **Per-command confirmation card** — never auto-execute, always show the literal command + working dir before running.
  - **Allowlist mode** — user pre-approves command prefixes (e.g. `git`, `ls`, `node`). Anything else needs ad-hoc confirmation.
  - **Output truncation** + 30s timeout default.
  - Telemetry on every invocation so we can audit.
- Use `tokio::process::Command` with carefully constructed args (no shell=true / shell-string parsing — pass argv as array).
- Probably worth declining for v1 — start with the safer tools above and revisit when Travis has earned trust through use.

**Scoping:** ship 1 → 2 → 3 → 4 → 5 in that order. Each is roughly a half- to full-day build. The agent loop is already in place (proposed_action infra) so most of the work is OAuth + the API call + the confirmation card UX per tool.

---

## Conversational / agentic Travis (next major arc)
**What:** Travis evolves from one-shot extractor to a stateful, interactive assistant. Components:
1. **Persistent conversation threads.** A `conversation` table linked to tasks/invoices/journal entries; multi-turn back-and-forth with the user. Clarifying questions (already in extraction) are surfaced into a thread that survives overlay dismissal — open it from a tasks view or a "Threads" tab.
2. **Sub-journals on tasks.** Append-only notes/discussions threaded onto a single task, so following up on Coach Maria's hours has its own running log.
3. **Proactive offers as actionable buttons.** Today's `capabilityGaps` get surfaced as text in the "Asks of me" tab. Future: when the user wants something Travis CAN do (e.g. "draft an invoice for John's February sessions"), surface a clickable card that pre-fills the form.
4. **Tool-calling.** Move LLM extraction to a tool-using model loop. Tools: `create_invoice(coach_id, period, hours, rate)`, `send_email(to, subject, body, attachments)`, `mark_task(id, status)`, `set_reminder(text, when)`, `search_memory(query)`. Travis reasons → calls a tool → sees the result → keeps going. With user-confirmation gates for destructive actions.
5. **Honest capability voicing.** When Travis can't do something, say so plainly in the same conversation thread (instead of just logging silently to `app_feedback`). "I can't email this for you yet — I noted it though, want me to keep tracking how often this comes up?"

**Why deferred:** Each piece is a real chunk. Doing it well requires (a) UI for threads, (b) prompt engineering for tool-use across all three providers, (c) confirmation/undo flows for destructive actions. Today's extractor + capability-gap log is the seed for this.

**Revisit when:** Daily-driver use surfaces concrete blockers — "I keep wanting to do X and Travis can't" — and the `app_feedback` table accumulates real signal. The most-asked capability becomes the first tool to wire.

---
