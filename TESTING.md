# Travis Test Plan

Run `npm run tauri dev`, work through each section. Mark with `[x]` when verified, leave `[ ]` if broken. Add a note line under any failing test so I can see what to fix.

---

## 1. App boot & migrations
- [ ] App launches without errors in the dev terminal
- [ ] DB at `%APPDATA%\com.leadtoempower.travis\travis.db` exists after launch
- [ ] Splash shows version + `ready` pill after first paint
- [ ] If wiped: onboarding flow appears

## 2. Onboarding (regression)
- [ ] Welcome → Name → Role → Org → Provider → Credentials → Done all render
- [ ] Enter advances; ← Back goes back; orb pulses on keystroke
- [ ] **Test connection** in credentials step: green + latency for valid key, red error for invalid
- [ ] Finish creates `user_profile` row, sets `meta.onboarded = 'true'`, splash greets by first name

## 3. Splash & data layer
- [ ] Splash shows stats line: `0 tasks · 0 invoices · 0 coaches · 0 schools` initially
- [ ] After capturing tasks via overlay (§4), the `N tasks` count goes up on next splash visit

## 4. Cmd/Ctrl+J overlay (regression)
- [ ] Pressing Ctrl+J anywhere in the OS summons the overlay in <500ms
- [ ] ESC dismisses
- [ ] Click outside the overlay also dismisses
- [ ] Card opacity blocks underlying app content (no bleed-through confusion)
- [ ] **Drag handle** still broken — known issue, skip unless you want to retest
- [ ] Orb in upper-right reacts to typing
- [ ] Keystroke in input → orb shifts to `typing` state, settles back to `idle`

## 5. Journal + LLM extraction (step 6)
**Setup:** Have a valid API key set (use the Re-enter API Key panel if needed).
- [ ] Type a journal entry like *"Need to follow up with Coach Maria at PS 142 by Friday about her March hours, and submit the DoF invoice for John's February sessions"* + Enter
- [ ] Toast shows ~`Captured 2 tasks · noted Coach Maria · PS 142… · 1 reminder`
- [ ] Recent task list updates to show the new tasks
- [ ] **Re-enter API Key** flow: dismiss the toast and clear keychain entry (or simulate by entering bogus key); next entry should show inline KeyEntry panel; saving a real key + re-pressing Enter should succeed
- [ ] Captured tasks have correct `dueAt` for relative dates (e.g. "by Friday" → next Friday's YYYY-MM-DD)
- [ ] Tasks have `source: 'journal'` (verify in SQLite if curious)

## 6. Semantic memory & Ask Travis (steps 7+8) — NEW
**First run will download ~130 MB BGE-small model. May take 30-90s on first call.**
- [ ] Calling `indexAllJournalEntries()` from the devtools console (open splash, F12) backfills any existing journal entries' embeddings into the `embedding` table
- [ ] Devtools: `await __TAURI__.core.invoke('ask_travis', { question: 'what did I say about invoices?' })` returns `{ answer, sources, model }`
- [ ] `answer` references actual journal content (not hallucinated)
- [ ] `sources` array has 0-5 hits with `score`, `similarity`, `recency`, `entityMatch`
- [ ] Subsequent calls don't re-download model (much faster — sub-second embedding)
- [ ] *Note:* there's no UI surface for Ask Travis yet — it's a backend capability; we'll add a UI in the next iteration

## 7. Reminder system (step 9) — NEW
- [ ] Capture a journal entry that includes a time-bound reminder (e.g. "Remind me to email John at 5pm tomorrow")
- [ ] Verify a row in the `reminder` table with `kind='time'`, `source='journal'`, populated `remind_at`
- [ ] Manually create one via devtools `await __TAURI__.core.invoke('create_reminder', { input: { text: 'Test reminder', kind: 'time', remindAt: '<now+1min YYYY-MM-DD HH:MM:SS>' } })`
- [ ] Within 30s of `remind_at`, an OS notification fires titled "Travis"
- [ ] `fired_at` gets stamped after firing; the same reminder doesn't fire twice
- [ ] `dismiss_reminder(id)` works; dismissed reminders don't re-fire
- [ ] *Edge:* reminders without `remind_at` are not auto-created from journal

## 8. Behavioral patterns (step 10) — NEW
- [ ] After several mutations (create some tasks, complete a few, ingest 2-3 journals), `event_log` has rows
- [ ] Devtools: `await __TAURI__.core.invoke('list_events', { limit: 20 })` returns recent events with kinds like `task_created`, `task_completed`, `journal_ingested`, `coach_hours_logged`
- [ ] `await __TAURI__.core.invoke('detect_patterns')` runs without error and returns a count
- [ ] `await __TAURI__.core.invoke('list_patterns')` returns detected patterns (empty array is OK if no 3-window repeats yet)

## 9. Summarization (step 11) — NEW
- [ ] `await __TAURI__.core.invoke('generate_daily_summary', { date: '<today YYYY-MM-DD>' })` returns a `Summary` row with sensible 2-4 sentences
- [ ] The same call with a date that has zero activity returns an error like "nothing to summarize"
- [ ] `await __TAURI__.core.invoke('generate_weekly_summary', { weekStart: '<7-day-ago YYYY-MM-DD>' })` returns 4-6 sentence summary
- [ ] `await __TAURI__.core.invoke('list_summaries')` shows generated summaries

## 10. Identity model (step 12) — NEW
- [ ] After a few journal entries with entities, `await __TAURI__.core.invoke('list_entities', { limit: 20 })` returns rows for the coaches/schools/depts mentioned, with `mentionsCount` ≥ 1
- [ ] Repeated mentions of the same coach (e.g. "Coach Maria" twice) increment the same row's count (verifies normalization)
- [ ] `await __TAURI__.core.invoke('get_profile_blurb')` returns a string like `"User: Bethel, COO at Lead to Empower. Known coaches: Maria, John. Schools: PS 142..."`

## 11. PDF invoice export (step 13) — NEW
**Setup:** Create a coach + school + a few coach_hours rows + a draft invoice referencing them.
- [ ] Devtools: `await __TAURI__.core.invoke('export_invoice_pdf', { invoiceId: <id>, destPath: 'C:/Users/bethel-babashola/Desktop/test-invoice.pdf' })` returns the saved path
- [ ] PDF opens in any reader; layout has header, bill-to/from blocks, line items table, total, footer
- [ ] Money values render as "$1,234.56" format
- [ ] Period dates match the invoice's period
- [ ] Line items are the coach_hours rows in that period

## 12. Email sending (step 14) — NEW
**Setup needed:** valid SMTP config (Gmail with app password, Mailgun, etc.)
- [ ] `await __TAURI__.core.invoke('set_smtp_config', { input: { host: '...', port: 587, username: '...', fromAddress: '...', fromName: 'Travis', useTls: true }, password: 'your-app-password' })` succeeds
- [ ] `await __TAURI__.core.invoke('get_smtp_config')` returns the config (no password)
- [ ] `await __TAURI__.core.invoke('send_invoice_email', { invoiceId: <id>, recipient: 'you@yourself.com' })` sends the email with the PDF attached
- [ ] Recipient inbox shows email with subject `Invoice {number} from {org}` and the PDF attachment
- [ ] `await __TAURI__.core.invoke('list_emails_sent')` returns audit row with `status: 'sent'`
- [ ] On failure (e.g. wrong password): `status: 'failed'` with the SMTP error in `errorMessage`

## 13. Feature flags (step 15) — NEW
- [ ] `await __TAURI__.core.invoke('get_flags')` returns `{ features: {} }` (or cached) immediately
- [ ] App launch logs include "startup flags refresh failed" since the placeholder URL doesn't exist yet — that's expected; cached/empty fallback works
- [ ] `await __TAURI__.core.invoke('set_flags_url', { url: 'https://httpbin.org/json' })` updates the URL
- [ ] `await __TAURI__.core.invoke('refresh_flags')` returns the parsed JSON from that URL

## 14. OTA updater (step 16) — NEW (skeletal)
- [ ] App boots without complaining about updater config (the placeholder pubkey is OK for runtime; failures only show up on signed release)
- [ ] `await __TAURI__.core.invoke('check_for_update')` returns `null` (no updates published yet)
- [ ] `.github/workflows/build.yml` and `release-update-json.yml` files exist
- [ ] DEFERRED.md mentions the signing key generation step
- [ ] *Real OTA verification* requires generating signing keys, publishing a v0.1.0 tag, then a v0.1.1 tag — full test deferred until first release

## 15. UI feel — visual polish
- [ ] Splash orb is vibrant, transitions colors, has occasional listening pulses + cleaning sweeps
- [ ] Onboarding feels like Typeform — one question per screen, smooth transitions
- [ ] Overlay opens fast, ESC closes fast
- [ ] Dark mode is consistent — no white flashes on transitions
- [ ] Text contrast is readable everywhere
- [ ] Animations don't stutter (60fps subjectively)

## 16. Regressions to watch
- [ ] Existing tasks still load on splash
- [ ] Onboarding still completes cleanly with all three providers
- [ ] LLM chat still works end-to-end
- [ ] Keychain key persistence still works between launches

---

## How to use this list

When testing, paste the section header + the failing line(s) back to me with what you saw. I'll fix and we'll re-test.

If everything in §1-§4 + §6 (memory) + §7 (reminders) passes, the new MVP layer is verified end-to-end. The other sections gate fully when their UI surfaces land.
