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
