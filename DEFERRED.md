# Deferred Backlog

Small, scoped items that don't fit cleanly into any phase of
[ROADMAP.md](./ROADMAP.md). Most former entries here have either
shipped (mobile / voice — phased; tools, calendar, email, signing key,
proactive — done) or were absorbed into the roadmap. What remains is
the genuinely-not-yet-phased odds and ends.

If an item here grows in scope or starts depending on other work, it
graduates into a roadmap phase.

---

## Web search tool
**What:** A `web_search(query)` tool to complement the shipped
`web_fetch(url)`. Travis can already follow a specific URL; it can't
yet broaden a question into a search.

**Why deferred:** Needs a backend choice and we want to stay free /
cheap. Realistic options:
- **Self-hosted SearXNG** in a sidecar Docker container. Zero per-call
  cost, but a docker-compose dependency we don't ship today.
- **Brave Search API** free tier (2000 queries / month). Independent
  index, real API key, compile-time `TRAVIS_BRAVE_KEY`.
- **Jina AI Reader** companion (`r.jina.ai/<url>`) — clean markdown
  for fetched URLs; pairs with whichever search backend wins.

**Skip outright:** DuckDuckGo HTML scraping (brittle), Stract (beta
churn), Common Crawl (petabyte infra).

**Revisit when:** A user-visible workflow keeps wanting "look this up"
and `web_fetch` alone isn't enough.

---

## Code signing & notarization
**What:** Apple Developer ID notarization for macOS, Authenticode
signing for Windows. Today the unsigned bundles trigger Gatekeeper /
SmartScreen warnings; users have to run
`xattr -dr com.apple.quarantine` on macOS or click "More info → Run
anyway" on Windows.

**Why deferred:** Costs money ($99/yr Apple, $200–400/yr Windows OV
cert) and adds cert rotation overhead. Direct distribution is fine
during preview.

**Revisit when:** First paid Travis Cloud customer or first batch of
non-technical users — they won't follow the bypass instructions.

---

## Encrypted local storage at rest
**What:** Encrypt the SQLite DB file on disk with a key derived from
device passkey or OS keychain.

**Why deferred:** Currently OS-level disk encryption (BitLocker /
FileVault) covers the threat model. API keys already live in the OS
keychain. Phase 6's E2E cloud relay handles the in-transit /
at-rest-in-cloud story; this is the local-disk-also-encrypted angle.

**Revisit when:** Someone is sharing a machine and using Travis under
a separate user account, or under contractual encryption requirements.

---

## Overlay drag-to-reposition (known bug)
**What:** The Cmd / Ctrl + J overlay window has a drag handle (top
strip with grabber pill) that should let the user reposition the
floating window. Tried both `data-tauri-drag-region` and explicit
`getCurrentWindow().startDragging()` from a `mousedown` handler —
neither initiates a native window drag on Windows 10.

**Why deferred:** Suspect Tauri 2 + Windows 10 + transparent-window
edge case. Likely fixes: try `data-tauri-drag-region` on a
non-transparent inner element, or upgrade to a newer Tauri 2.x with
potential fixes.

**Revisit when:** It bothers daily use, or as part of the overlay
polish pass that comes alongside Phase 9 (mobile) when the cmd-J
surface is being rethought across devices anyway.

---

## A/B testing & gradual rollout
**What:** User-level feature flags, percentage rollouts, experiment
tracking.

**Why deferred:** Single-user app today. There's nothing to A/B
against. The `flags.rs` module already provides the shape (server-fed
config), so this is a UX + analytics layer on existing infra, not net
new infrastructure.

**Revisit when:** Phase 12 (multi-user) lands and the user base is
big enough for percentages to mean something.

---

## Plugin platform: pack onboarding hooks
**What:** Packs declare config they need at install time (e.g., L2E
needs the default invoice prefix; HVAC would need the default labour
rate; therapy might need a billing-code list). Onboarding inserts a
"Configure {pack name}" step after the pack picker, with one input per
declared `FieldDef`. Values stored in `meta.pack.<slug>.config.<field>`
so pack code reads them via `crate::packs::config::get(slug, field)`.
Settings → Packs gains an "Edit configuration" button per pack with
non-empty `onboarding_fields()`.

**Why deferred:** No pack currently needs this. L2E hardcodes
`L2E-{year}-NNNN` as the invoice prefix; tutoring has no install-time
config. Spec'd in `PLUGIN_PLATFORM.md` slice 8 but not implemented —
adding the mechanism without a real consumer = dead code that ages
without being exercised.

**Revisit when:** A pack gets its second customer and you discover
they want a different default for some hardcoded value. That's the
moment the pack author needs `onboarding_fields()`. Half-day to add
the trait method + Tauri commands + onboarding step.

---

## Plugin platform: ref resolution
**What:** Auto-CRUD list/detail views show foreign-key fields as
`coach#5` instead of `Coach Maria`. The pack's `TableDef` already
declares `display_field` for each table (almost always `name`); the
auto-CRUD just doesn't JOIN through to fetch the referenced row's
display value yet.

**Why deferred:** Cosmetic. The integer ID is unambiguous and clickable
becomes drill-in once `RefPicker` lands. Skipping the JOIN keeps the
SQL builder simple in v1.

**Revisit when:** First customer demo where someone asks "why does it
say `coach#3`?". Half-day to add the JOIN + cache.

---

## Plugin platform: list-view filters & search
**What:** ListView shows a filter bar above the table. Per-field
filters by `FieldType` (date range, enum dropdown, ref equality, text
contains). Generic SQL builder extends `pack_table_list` to accept a
filter object. Schema metadata already has the type info needed.

**Why deferred:** Lists today fit on one page for most packs; sort
suffices. Filters become essential once a pack has 100+ rows of
something.

**Revisit when:** Someone with a year of L2E data hits the page-size
limit and asks for filtering. ~1 day.

---

## Plugin platform: pack-grouped Manage tabs
**What:** With L2E + Tutoring both enabled the Manage tab bar gets
long (Ask · Threads · Tasks · Coaches · Schools · Hours · Signing
Sheets · Invoices · Tutors · Students · Sessions · Progress Reports ·
Reminders · …). Group pack tabs under collapsible sub-navs:
`[Ask] [Threads] [Tasks] [Lead to Empower ▸] [Tutoring ▸] [Reminders]`.

**Why deferred:** Two-pack deployments are tolerable as flat tabs;
3+ packs make the bar unworkable. Pure UX polish.

**Revisit when:** Three or more packs ship and the tab bar overflows
on a normal-width window. ~half-day.

---

## Invoice templates library
**What:** Multiple named invoice layouts the user can pick between,
beyond the single NYC DoF design that ships today.

**Why deferred:** L2E pack-internal feature now that Phase 1 has
shipped (v0.2.0). Add a `templates/` dir under
`src-tauri/src/packs/lead_to_empower/pdf/` with handlebars-shaped
layouts and a per-invoice template selector. No external
dependencies; just product work.

**Revisit when:** The L2E pack has 2–3 distinct invoice formats users
switch between, OR a customer of the invoicing pack needs branding /
layout customisation.
