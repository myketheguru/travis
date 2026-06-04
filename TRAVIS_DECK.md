# Travis — Presentation Build Spec

A self-contained spec for an AI agent to build a beautiful, scroll-driven, GSAP-animated single-page web presentation about Travis. Every section below has finalized copy, visual direction, animation behavior, and asset slots. Pass this entire file to the building agent.

---

## 0. Build target

- **Format:** single-page web app, scroll-driven storytelling
- **Framework:** React + Vite + TypeScript (matches Travis's own stack)
- **Animation:** GSAP + ScrollTrigger as primary; Framer Motion acceptable for smaller component-level transitions
- **Styling:** Tailwind CSS v4 with `@theme` tokens (mirror `src/styles.css`)
- **Responsive:** desktop-first, but must read cleanly on tablet and mobile
- **Output:** static site (Vercel / Netlify deployable)
- **Performance budget:** LCP < 2s on a fresh load, smooth 60fps scroll on mid-tier laptops

---

## 1. Brand kit (do not deviate)

### Palette

| Token | Hex | Role |
|---|---|---|
| `--color-ink` | `#07080b` | Primary background (deepest) |
| `--color-ink-2` | `#0d0f14` | Section panels, cards |
| `--color-ink-3` | `#161922` | Borders, dividers, raised cards |
| `--color-bone` | `#ececf1` | Primary text, headings |
| `--color-bone-2` | `#b3b5bf` | Secondary text, body |
| `--color-bone-3` | `#6a6d78` | Captions, metadata |
| `--color-pulse` | `#7c5cff` | Primary accent — violet |
| `--color-pulse-2` | `#4ad6ff` | Secondary accent — cyan |
| `--color-warn` | `#ffb86b` | Highlight / alert |

### Signature gradient

Used on body background, hero glow, button hovers, key accent strokes:

```css
background:
  radial-gradient(1200px 800px at 50% -10%, rgba(124, 92, 255, 0.10), transparent 60%),
  radial-gradient(900px 600px at 100% 110%, rgba(74, 214, 255, 0.06), transparent 60%),
  var(--color-ink);
```

For accent text / lines / divider sweeps:
`linear-gradient(135deg, #7c5cff 0%, #4ad6ff 100%)`

### Typography

- Family: **Inter** (variable; weights 300, 400, 500, 600, 700)
- Display sizes: hero 96–128px / section heads 64–80px / sub-section 36–48px / body 18–20px / caption 14–16px
- Letter-spacing: tighter for display (`-0.02em`), default for body
- Line-height: 1.0–1.1 on display, 1.5–1.6 on body
- Numeric: use tabular-nums for animated counters

### Voice (copy tone)

Match Travis's product voice: **direct, quietly confident, no jargon, no marketing puffery, no emojis**. Read like a thoughtful operator wrote it. Sentence-case headings (not Title Case). Short paragraphs. Pull-quotes set in pulse-violet or cyan.

### Motion principles

- **Calm**, not flashy. Animations serve comprehension, not spectacle.
- **Long pinned sections** for the architecture and timeline; short reveals for everything else.
- Ease: `power3.out` for entrances, `power2.inOut` for transitions
- Stagger: 60–100ms between sibling elements
- Default duration: 0.6–1.2s for hero/headings, 0.3–0.6s for body
- Never animate continuously without a scroll trigger (no infinite spinners except the hero orb)

---

## 2. Suggested tech setup

```json
{
  "dependencies": {
    "react": "^19",
    "react-dom": "^19",
    "gsap": "^3.12",
    "@gsap/react": "^2.1",
    "framer-motion": "^11",
    "clsx": "^2",
    "lucide-react": "^0.460"
  },
  "devDependencies": {
    "vite": "^7",
    "tailwindcss": "^4",
    "@tailwindcss/vite": "^4",
    "typescript": "^5.8"
  }
}
```

Register `ScrollTrigger` once at app root. Use `useGSAP` hook per section.

---

## 3. Page architecture

14 sections, vertically stacked. Some are full-viewport snap; some flow. Section IDs in parentheses are anchors for nav.

```
┌─ 01 Hero               (#hero)            full-viewport
├─ 02 Origin story       (#origin)          pinned scroll
├─ 03 Architecture bet   (#architecture)    pinned scroll
├─ 04 Journey/phases     (#journey)         long pinned timeline
├─ 05 BRAIN capabilities (#brain)           horizontal card scroll
├─ 06 GTM thesis         (#gtm)             grid + animated counters
├─ 07 Where revenue starts (#revenue)       two-column
├─ 08 What stands between us (#whats-left)  phase deep-dive
├─ 09 Education vertical (#edu)             pinned scroll
├─ 10 EDU audiences      (#audiences)       three-column scroll-snap
├─ 11 What we'd build    (#edu-build)       checklist grid
├─ 12 Open questions     (#questions)       category grid
├─ 13 Founding team      (#team)            three portraits
└─ 14 Closing / ask      (#closing)         full-viewport
```

A persistent thin top nav appears after scrolling past the hero — pulse-violet underline indicates active section.

---

## 4. Section specs

### 01 — Hero (#hero)

**Purpose:** establish the product in one breath; set tone.

**Layout:** full viewport. Centered headline + sub. PresenceOrb animation behind/below.

**Copy:**

> **Eyebrow** (small, bone-3, letter-spaced uppercase): `TRAVIS — v0.11.0`
>
> **Headline** (display, bone, 128px): `The thinking + execution layer for the work nobody else automates.`
>
> **Sub** (bone-2, 22px, max-width 720px): `A local-first AI operations assistant. Open-core. Built for the operators who quietly hold whole organizations together.`
>
> **Tertiary line** (bone-3, 14px, fades in last): `Scroll →`

**Visual:**
- The Travis "presence orb" — a radial gradient sphere (~480px) anchored bottom-center, gently pulsing at 0.06 Hz
- Gradient stops: `#1a1230 0%, #0a0a18 65%, #07080b 100%` (matches `src/components/PresenceOrb.tsx`)
- Subtle violet glow at top-left and cyan glow at bottom-right (matches body background)
- Faint cursor blink at end of headline (1Hz, pulse color)

**Animation:**
- Headline animates in word-by-word, stagger 80ms, `power3.out`, 1.1s total
- Sub fades in 600ms later
- Orb scales from 0.92 → 1.0 with a 1.8s bloom on load
- "Scroll →" indicator pulses subtly to encourage interaction

**Assets needed:** none — fully CSS/SVG-generated.

---

### 02 — Origin (#origin)

**Purpose:** the human story. Why Travis exists.

**Layout:** pinned section, ~2.5x viewport scroll length. Left column: text reveals. Right column: stylized "before/after" visual.

**Copy (three text beats that reveal sequentially):**

> **Beat 1:**
> **It started with a COO who was drowning in paper.**
> Taylor runs operations for Lead to Empower, a coaching firm placing instructors in NYC schools. Every month: dozens of work orders, sign-in sheets to chase, purchase orders to reconcile against invoices, contracts that quietly drift toward expiration.

> **Beat 2:**
> **A $5,013.30 invoice error became the spark.**
> A unit price mismatched. A date fell outside the school year. A renewal slipped. The cost wasn't the dollar amount — it was the half-day of forensic work to find where the system broke. Multiply by every operator quietly holding an organization together and you have a category nobody is serving well.

> **Beat 3:**
> **The thesis was simple: what if there was a tool that just remembered?**
> Not another GPT wrapper. Not a notes app with AI sprinkled on top. A real operations assistant — local-first, audit-logged, with memory that compounds. The first commit landed in mid-2025. Today, eleven minor versions later, Travis runs Taylor's entire month.

**Visual (right column):**
- Stylized illustration: scattered "paper" cards (work orders, sign-in sheets, invoice numbers) on the left half
- Right half: clean Travis chat interface (use a real screenshot — see asset slot)
- Animated cards slide from chaos → into the Travis interface as user scrolls
- **[ASSET-SLOT-1]**: screenshot of Travis chat showing an invoice draft proposal

**Animation:**
- Each beat fades in + slides up 24px, triggered by scroll progress
- Paper cards have parallax (different scroll speeds)
- The "consolidation" moment (when cards slide into Travis) happens at ~70% scroll progress

---

### 03 — Architecture bet (#architecture)

**Purpose:** prove this isn't a GPT wrapper. Show the layered architecture.

**Layout:** pinned, 2x viewport. Centered diagram with text beats fading in alongside.

**Copy:**

> **Headline:** `Travis is an orchestra, not a model.`
>
> **Sub:** `Seven layers, each independently swappable. The defensible thing isn't the AI — it's everything that surrounds it.`

**The seven layers** (animate in bottom-up like a stack):

1. **Identity & state** — one user, many embodiments. Phone, desktop, car.
2. **Knowledge graph** — typed entities, relations, events. Memory that compounds.
3. **Orchestrator** — small + fast. Decides what slice of memory is relevant.
4. **Reasoner** — frontier model. Called only for hard thinking.
5. **Action layer** — declared schemas, permission policies, audit trails.
6. **Perception** — mics, cameras, screen, calendar — each with a kill switch.
7. **Trust & audit** — append-only signed log. Every action reversible.

**Pull-quote (bottom):**
> `"Models are commodities. The system is the moat."` — pulse-violet, large, italic

**Visual:**
- Stacked layered diagram on the right. Each layer is a horizontal bar with icon + name + one-line role
- Connected by thin pulse-violet → cyan gradient lines
- Background grid pattern at 4% opacity (subtle technical feel)
- **[ASSET-SLOT-2]**: optional — actual architecture diagram from internal docs if one exists

**Animation:**
- Layers reveal bottom-up as scroll progresses, each layer "snapping" into place with a 0.4s ease
- Connection lines draw on with `stroke-dashoffset` animation
- When all 7 are visible, the pull-quote fades in below

---

### 04 — Journey (#journey)

**Purpose:** show what's shipped and what's coming. Build credibility through progress.

**Layout:** long pinned section. Horizontal-scroll timeline driven by vertical scroll (i.e., user scrolls down, timeline moves left-to-right).

**Copy headline:** `From v0.1.0 to v0.11.0 — six months of weekly shipping.`

**Timeline entries** (each is a card):

| Phase | Status | Headline | One-liner |
|---|---|---|---|
| 0 | ✅ shipped | Foundation | Local-first desktop scaffolding |
| 1 | ✅ shipped | De-domain | Travis becomes a pack platform |
| 2 | ✅ shipped | Workspaces | Multi-context with sensitive-data isolation |
| 3 | 🟡 partial | Token economy | Prompt-cache hygiene — full intent router pending |
| 4 | ✅ shipped | Knowledge graph | Seven BRAIN capabilities live |
| 5 | ⬜ next | Tool spec compiler | Custom tools per enterprise customer |
| 6 | ⬜ next | Cloud relay | One identity, every device, end-to-end encrypted |
| 7 | ⬜ next | LLM gateway | First product-led paid tier |
| 8 | ⬜ later | Pre-registered OAuth | One-tap Gmail, Outlook, Slack |
| 9 | ⬜ later | Mobile companion | Capture from anywhere |
| 10 | ⬜ later | Web client | Browser-resident Travis |
| 11 | ⬜ later | Voice / ambient | From "tool I use" to "presence I have" |
| 12 | ⬜ year-2 | Marketplace / teams | Family / team Travis |

**Animated stats above timeline** (count up on scroll enter):

- `11` minor versions shipped
- `7` BRAIN capabilities live
- `27` Tauri commands exposed
- `2` packs (Lead to Empower + Tutoring)
- `0` external dependencies on a single AI vendor

**Visual:**
- Timeline track is a thin pulse-violet → cyan gradient line
- Each phase is a card with status pill, headline, and one-liner
- Shipped cards: full opacity, pulse-violet border
- Next cards: 60% opacity, dashed border
- Year-2 cards: 30% opacity, no border
- Cursor blink marker on the "now" position (between Phase 4 and Phase 5)

**Animation:**
- Section pins, timeline scrolls horizontally as user scrolls down
- Each card fades in as it enters viewport
- Stats count up via GSAP `to({ value: n }, { duration: 1.4 })` triggered when card enters viewport

---

### 05 — BRAIN capabilities (#brain)

**Purpose:** make the cognitive layer feel real and impressive.

**Layout:** horizontal scroll-snap of 7 cards. Each card ~80% viewport width on desktop, full-width on mobile.

**Headline:** `The BRAIN — seven capabilities, all running locally.`

**Cards:**

1. **Memory** — typed graph + embeddings + claims + 30-min working memory. Travis doesn't forget what you mentioned last Thursday.
2. **Personality** — a single voice across every surface. Voice corrections accumulate over time; Travis sounds more like you each month.
3. **Learning others** — per-entity personality slots. Travis notices Maria prefers brevity; Travis notices Jacob signs everything in the morning.
4. **Collaboration** — initiatives layer. Named multi-session pushes with owners, last decisions, open questions.
5. **Proactivity** — observer + rhythm-aware timing. Travis notices the sign-in sheet that's been signed but not invoiced.
6. **Self-advocacy** — recurring gap surfacing. When the same blocker keeps appearing across captures, Travis names it once.
7. **Wellbeing** — affect signals. Travis notices tone shifts. *These signals never leave your device.* Period.

**Visual per card:**
- Large numeral (display, bone-3) in top-left
- Capability name (display, bone, 64px)
- One-paragraph description (bone-2, 22px)
- Small "in-the-app" mini-screenshot or animated mock at the bottom
- Card background: `--color-ink-2` with a thin pulse-violet glow on the left edge
- **[ASSET-SLOT-3a through 3g]**: 7 mini-screenshots — one per capability if available, otherwise abstract illustrations

**Animation:**
- Horizontal scroll snap — one card at a time
- Numeral counts up (1, 2, 3 …) as card enters
- Background glow intensifies on active card

---

### 06 — GTM thesis (#gtm)

**Purpose:** show the path to revenue. From MARKET.md.

**Layout:** two-row grid. Top: headline + thesis. Bottom: tier visualization.

**Copy:**

> **Eyebrow:** `GO-TO-MARKET`
>
> **Headline:** `Vertical-first. Own the operators nobody else is serving.`
>
> **Sub:** `We're not chasing horizontal volume. We're shipping one well-built pack at a time into structured operations niches where existing tools are expensive, dated, and built for a different decade.`

**The filter (small caption):**
`Every vertical on our list passes four tests: structured operations · non-technical operator audience · painful/expensive incumbents · clear ROI per seat.`

**Tier grid (4 columns):**

| Tier A — Ship now | Tier B — After Phase 6 | Tier C — After Phase 9 | Tier D — Year 2 |
|---|---|---|---|
| L2E shape — 80% pack reuse | HIPAA-grade audit needed | Mobile required | Records-heavy pro services |
| **After-school programs** $200–400/mo | Therapy practices $50–150/seat | HVAC/plumbing $200–400/mo | Legal practices $100–300/seat |
| **Sports coaching** $150–300/mo | SLP/OT itinerant $200–500/mo | Pest control $150–300/mo | CPA firms $100–250/seat |
| **Tutoring agencies** $200–500/mo | Psychiatric practices $100–250/seat | Lawn care $100–250/mo | Appraisers $100–200/mo |
| **Home care agencies** $300–800/mo | Doula practices $50–150/mo | Pool service $100–200/mo | Translation agencies $200–500/mo |
| **Cleaning services** $100–250/mo | Coaching/wellness $50–150/mo | Mobile pet/vet $100–200/mo | IT MSPs $200–500/mo |

**Highlight (animated reveal at bottom):**
> `Tier A item #1 is exactly Taylor's shape. Travis's first customer is her network's archetype. We can sell into her network the day we close Phase 1 cleanly.`

**Animated stats:**
- `~30,000` US after-school orgs
- `~20,000` tutoring agencies
- `~12,000` home care agencies
- `60%` of Tier A reachable with one well-built pack

**Visual:**
- Tier columns ramp up in saturation from D (faded) to A (full pulse-violet)
- Vertical lines connect each row, drawing on as user scrolls

**Animation:**
- Stats count up
- Tier A column pulses softly to draw attention

---

### 07 — Where revenue starts (#revenue)

**Purpose:** show the *near-term* revenue path, not just the long-term TAM dream.

**Layout:** split-screen. Left: "Today" panel. Right: "Month 2-4" panel.

**Copy:**

> **Headline:** `Two revenue paths. Both compatible. Both starting soon.`

**Left panel — TODAY:**
> **Concierge tier — $300–500/mo per vendor**
> Founder-led setup. Weekly office hours. Custom pack tweaks for each customer. No cloud infrastructure required. No license gate. Revenue can start *this month* — the only question is whether Taylor's network has 3 specific names willing to commit.
>
> Why this works: it's exactly what the COO is already getting in beta. Codify, charge, repeat.

**Right panel — MONTH 2–6:**
> **Pro tier — usage-based on hosted LLM**
> Travis Cloud proxies LLM calls. No API keys to manage. Stripe billing. Self-hosters can still BYOK. This is Phase 7 of the roadmap, and it sits on top of Phase 6 (encrypted cloud relay + identity).
>
> Why this works: it respects the open-core thesis — *the local product is never paywalled.* Customers pay for hosted convenience, not for features that already work on their machine.

**Bottom pull-quote:**
> `Never paywall a feature that already works locally. That's not marketing — that's an architectural constraint.`

**Visual:**
- Left panel anchored in the present (deep ink background, clearer text)
- Right panel anchored in near-future (subtle cyan accent, "in progress" tag)
- Connecting arc between the two panels — gradient line drawing on as user scrolls

**Animation:**
- Panels slide in from opposite sides
- The connecting arc draws on at 60% scroll

---

### 08 — What stands between us (#whats-left)

**Purpose:** be specific and honest about the build work between today and self-serve paying customers.

**Layout:** two large feature blocks (Phase 6, Phase 7) stacked or side-by-side, with detailed bullets.

**Copy:**

> **Headline:** `Two phases stand between one beta user and fifty paying ones.`

**Block 1 — Phase 6: Encrypted cloud relay + identity (3–4 months)**

- One user identity across every device
- CRDT-based sync (Yjs / Automerge) over WebSocket
- End-to-end encryption — cloud sees ciphertext, can't decrypt anything
- Self-hostable relay (Docker image, k8s chart) — same protocol the hosted version uses
- Sign-in via Google or Microsoft (existing OAuth)

> *First major paid product. The day this ships, "Travis Cloud" becomes a real SKU.*

**Block 2 — Phase 7: Hosted LLM gateway (1–2 months on top)**

- Cloud proxies LLM calls to Anthropic / OpenAI on our keys
- Per-account quotas with Stripe usage-based billing
- Self-hosters keep BYOK, works exactly as today
- Over-quota response gracefully degrades

> *Main consumer revenue mechanism. Flat subscription + usage rates.*

**Bottom callout:**

> **Bridge plan, months 1–4:** concierge revenue starts immediately. We onboard 3–5 EDU vendors at $300–500/mo with white-glove service while Phase 6+7 ship. By month 4, concierge customers can graduate to Pro tier OR stay on the high-touch enterprise plan. Either way, we're sustainable.

**Visual:**
- Two large dark cards with pulse-violet borders
- Mini-architecture diagram inside each: Phase 6 shows device-to-relay-to-device flow with encryption indicators; Phase 7 shows app → gateway → LLM provider flow with quota meter
- **[ASSET-SLOT-4]**: architecture diagrams (can be SVGs the builder generates)

**Animation:**
- Cards slide up sequentially
- The encryption padlock icons "lock" with a small animation when card enters viewport
- Quota meter on Phase 7 card fills 0 → 80% as a teaser

---

### 09 — Education vertical (#edu)

**Purpose:** make the EDU bet concrete. Why education, why now.

**Layout:** pinned scroll, ~2x viewport. Headline + four "why now" panels appearing sequentially.

**Copy:**

> **Eyebrow:** `THE VERTICAL`
>
> **Headline:** `Education is the right shape, right now.`
>
> **Sub:** `Not because we want to be an "EdTech company." Because the operators inside the education ecosystem are precisely who Travis was built for, and our first customer's network is full of them.`

**Four "why now" panels** (slide in sequentially):

1. **Distribution already exists.** Taylor sells into NYC schools daily. Her phone has the numbers of every coaching firm and PD vendor in her network. We don't need cold outbound to start.

2. **NYC alone is a category.** Thousands of schools, hundreds of vendors, a single procurement entity (NYC DOE) for sub-vendors. Sticky multi-year master agreements. Recurring contract cycles. Government dollars flow predictably.

3. **The pain is uniquely visible.** Paper sign-in sheets. Faxed POs. Excel-based contract tracking. Email threads as project management. The status quo in education ops is not "old software" — it's often *no software*.

4. **The vertical generalizes.** "People sent to places, hours billed, signed proof, payer invoiced" — that's Tier A of MARKET.md. Education ops is one expression of a much larger operator shape.

**Visual:**
- Each panel has a small accompanying illustration (school building, NYC skyline silhouette, paper-to-digital transition, network graph)
- Subtle map of NYC behind the second panel with school dots glowing

**Animation:**
- Pinned section with text panels appearing as scroll progresses
- Each panel slides + fades in with 0.8s ease
- Map dots in panel 2 pulse softly

---

### 10 — EDU audiences (#audiences)

**Purpose:** show the *expanded* surface area — who in education benefits from a Travis-shaped tool.

**Important framing line at top of section:**
> *Today, Travis serves vendors who sell into schools (Taylor's company). The audiences below describe the natural expansion path — what Travis could serve in 6, 12, 24 months as the platform matures.*

**Layout:** four-column scroll-snap on desktop, vertical scroll on mobile. Each column is one audience.

**Column 1 — Vendors & service providers (today)**
- Coaching firms, tutoring agencies, enrichment program operators
- Tax: contracts, POs, sign-in sheets, invoices, compliance
- This is Travis today, v0.11.0. Live.
- Customer zero: Lead to Empower.

**Column 2 — Schools & institutions (Q1–Q2 next)**
- District operations teams, charter networks, school admins
- Procurement workflow (POs, contracts, vendor management)
- Compliance tracking (Title IX, FERPA audits, board reports)
- Cross-vendor coordination from inside the institution
- Memory across school years — institutional knowledge that doesn't leave when staff turns over

**Column 3 — Teachers (Q3–Q4)**
- Lesson plans that emerge from captured thinking ("I want to teach quadratics like a story")
- IEP / 504 deadline tracking with proactive nudges
- Parent communication queue with drafted messages
- Sub-prep packs assembled from current units
- PD reflection that becomes learning over time

**Column 4 — Students (Year 2, optional)**
- Executive function support — Travis's journal-to-task pattern applied to assignments
- Stale-draft observer flags ignored projects before they're late
- Group-project coordination across classmates
- Reflection journal that supports college applications
- **Privacy paramount.** Local-first by default. Not a surveillance tool. Schools never see student affect signals.

**Visual:**
- Each column has a distinct color accent (still in palette): vendors = pulse-violet, institutions = cyan, teachers = warn-amber-tinted, students = bone-2 (cool/neutral)
- Each column has 3–5 mini bullet items with checkmark/arrow indicators
- Column headers have an iconography (briefcase, building, book, backpack)

**Animation:**
- Horizontal scroll-snap one column at a time
- Each bullet fades in with stagger when its column is active
- "Today" / "Q1–Q2" / "Q3–Q4" / "Year 2" timeline pip at the top of each column

---

### 11 — What we'd build for EDU (#edu-build)

**Purpose:** make the EDU expansion concrete, not aspirational.

**Layout:** four-row checklist grid matching the audiences from §10.

**Copy headline:** `What it takes — concretely.`

**Row 1 — For vendors (mostly there)**
- ✅ Multi-vendor pack config (company profile + editable catalog)
- ⬜ Contract renewal lifecycle observer
- ⬜ Payment tracking + AR aging buckets
- ⬜ Polaris/DOE invoice format variant
- ⬜ Compliance document vault with expiry alerts
- ⬜ Onboarding wizard for non-builders

**Row 2 — For institutions**
- ⬜ District-shaped pack (POs from the buyer side, vendor management, board reporting)
- ⬜ FERPA-aware data isolation policy
- ⬜ Multi-user workspaces (early element of Phase 12 pulled forward)
- ⬜ Sub-vendor coordination views

**Row 3 — For teachers**
- ⬜ Education-flavored pack (lesson plans, IEPs, parent communications)
- ⬜ School-year calendar integration
- ⬜ PDF generation for standardized forms (IEP, 504)
- ⬜ Classroom-mode UI affordances

**Row 4 — For students**
- ⬜ Lightweight single-vertical onboarding
- ⬜ Mobile companion (Phase 9 pulled forward or desktop-first pilot)
- ⬜ Education-tier pricing (free or low monthly)
- ⬜ Explicit consent + visible audit log for any school data integration

**Visual:**
- Each row is a horizontal strip
- ✅ items in pulse-violet, ⬜ items in bone-3 with a thin dashed border
- Row 1 visually leads (fully saturated); Rows 2–4 progressively fade

**Animation:**
- Rows reveal sequentially
- Each item's checkmark "tick" animates in

---

### 12 — Open questions (#questions)

**Purpose:** signal the team is thinking critically. Invite collaboration.

**Layout:** category grid — 6 categories, 2-3 questions each.

**Copy headline:** `What we still need to decide.`

**Sub:** `Honest open questions. Decisions worth getting right before we sprint.`

**Categories:**

**1. Pricing & business model**
- Pricing per vendor / per school / per teacher / per student — what tiering survives contact with actual buyers?
- Concierge → Pro graduation path: do early customers grandfather in, or migrate up?
- Government contracts (MTAC, MWBE, district master agreements) — pursue directly or via partner channels?

**2. Demand validation**
- Real names + emails of 3–5 vendors who'd pay $300–500/mo within 30 days?
- District-level interest beyond Taylor's immediate network — who introduces us?
- What's our churn risk profile look like in months 4–6?

**3. Architecture & open-source thesis**
- Does the open-core thesis survive contact with EDU buyers? (Some districts prefer hosted SaaS over local installs.)
- License choice: MIT/Apache (permissive, faster adoption) or AGPL (anti-commodity-clone)?
- When — if ever — do we paywall a local feature? (Default answer: never.)

**4. Compliance & risk posture**
- FERPA stance: do we ever touch student PII? If yes, what's the data flow and audit trail?
- SOC 2 — when does it become non-optional, and who owns it?
- Data residency — what happens when a district demands US-only data centers?

**5. Team & capital**
- Equity allocation — Taylor as co-founder vs. early team vs. advisor?
- Customer success: when does it stop being founder-led and become a hire?
- Bootstrap vs. raise: at what revenue/customer count does the math change?

**6. Product priority**
- Mobile companion — pull forward for student use case, or hold for Phase 9 as planned?
- Voice / ambient — does the teacher use case justify pulling Phase 11 earlier?
- Education-specific pack — separate from L2E pack, or extension?

**Visual:**
- 6 cards in 3x2 grid (or 2x3 on smaller screens)
- Each card has a category title, then 2-3 bulleted questions
- Question marks render in pulse-violet for emphasis

**Animation:**
- Cards fade in with stagger
- On hover (desktop), card lifts 4px with a subtle pulse glow

---

### 13 — Founding team (#team)

**Purpose:** introduce the people. Build trust.

**Layout:** three portrait blocks in a row.

**Copy headline:** `Three of us. Travis writes code with us. The market sees four.`

**Sub:** `Small team by design — we're letting the operator's pain set the pace, not a headcount plan.`

**Member 1 — Michael**

- **Role:** Founder, engineering + product
- **One-liner:** Built Travis solo from v0.1.0 to v0.11.0. Background in [SLOT — fill in your background]. Lives in the codebase, sees the COO's workflow as the design partner.
- **What he owns:** Architecture, build pace, technical roadmap, the BRAIN.

**Member 2 — Taylor**

- **Role:** Co-founder, go-to-market
- **One-liner:** COO at Lead to Empower. Travis's customer zero. Knows the EDU operator network — every coaching firm, every PD vendor, every DOE contact in NYC.
- **What she owns:** Distribution, the EDU vertical thesis, first 10 paying customers, the operator's voice in product decisions.

**Member 3 — David**

- **Role:** Digital marketing
- **One-liner:** Brand, content, top-of-funnel. Translates technical capability into operator-readable language.
- **What he owns:** Site, content strategy, top-of-funnel acquisition, narrative.

**Visual:**
- Each member: portrait (or abstract avatar in pulse-violet if no photo), name, role, bio
- Connected by a thin gradient line — they're a team, not three solo operators
- **[ASSET-SLOT-5a, 5b, 5c]**: portraits

**Animation:**
- Portraits fade in with stagger
- Connecting line draws on after all three are visible

---

### 14 — Closing / ask (#closing)

**Purpose:** call to action. Where we're going.

**Layout:** full viewport, centered.

**Copy:**

> **Headline (display, bone, 96px):** `From one beta user to fifty paying customers.`
>
> **Sub (bone-2, 22px):** `The pack is ready. The vertical is identified. The phases are mapped. What we need now is the network, the capital, and the right pace.`

**Three-column "what's next":**

| Month 1–3 | Month 4–6 | Month 7–12 |
|---|---|---|
| Concierge revenue starts | Phase 6 ships — Travis Cloud goes live | Phase 7 ships — Pro tier opens |
| 3–5 EDU vendors onboarded | First district pilot begins | Second vertical (tutoring or home care) opens |
| Phase 1 cleanup finalized | Phase 5 tool spec compiler | Mobile companion in dev |
| Initial $1.5–2.5k MRR | $5–10k MRR | $20–40k MRR target |

**CTA section:**
- Primary button: `Get on the beta list →` (pulse-violet, fills with cyan gradient on hover)
- Secondary: `Read the roadmap` (links to a public ROADMAP.md mirror)
- Footer: `travis.app · @travis_ai · hello@travis.app` (placeholder — fill with real)

**Visual:**
- Same orb as the hero, smaller, top-center
- Background: same gradient as body, slightly intensified

**Animation:**
- Final headline reveals letter-by-letter with stagger 40ms
- Orb pulses
- Buttons glow softly on idle

---

## 5. Cross-cutting elements

### Navigation

- Sticky top nav appears after scrolling 100vh past hero
- Logo (small pulse-violet orb) on the left
- Section anchors on the right: `Origin · Architecture · Journey · BRAIN · GTM · EDU · Team`
- Active section gets a pulse-violet underline animated via GSAP

### Footer

- Minimal: logo, three columns (Product / Company / Resources), social, copyright
- Background: `--color-ink-2`

### Scroll progress indicator

- Thin pulse-violet → cyan gradient line at the very top of viewport, growing left-to-right as user scrolls
- 2px tall

### Cursor (optional, desktop only)

- Custom cursor: small pulse-violet dot that follows mouse
- Grows + glows when hovering interactive elements
- Mix-blend-mode: difference for legibility over light/dark areas

---

## 6. Asset inventory needed from Michael

Before the agent builds, gather these. Use placeholder boxes (`--color-ink-3` filled rectangles with bone-3 label "screenshot pending") for any missing assets so the deck still ships.

- [ ] **ASSET-SLOT-1:** Screenshot of Travis chat showing an invoice draft proposal — for §02 Origin
- [ ] **ASSET-SLOT-2:** Architecture diagram (optional — agent can render if absent) — for §03
- [ ] **ASSET-SLOT-3a–3g:** Seven mini-screenshots or animated mocks, one per BRAIN capability — for §05
  - 3a: Memory (graph visualization or recall popover)
  - 3b: Personality (settings screen showing voice corrections)
  - 3c: Learning others (entity recall tooltip)
  - 3d: Collaboration (initiatives list)
  - 3e: Proactivity (proactive nudge in overlay)
  - 3f: Self-advocacy (recurring gap surface)
  - 3g: Wellbeing (abstract — no real screenshots since affect data is sensitive)
- [ ] **ASSET-SLOT-4:** Phase 6 / Phase 7 architecture diagrams — for §08 (agent can SVG-render these)
- [ ] **ASSET-SLOT-5a, 5b, 5c:** Portraits of Michael, Taylor, David — for §13
- [ ] Logo / wordmark SVG (if exists; otherwise agent generates a clean type-based wordmark using Inter Bold)
- [ ] favicon

---

## 7. Reviewer-friendly content checklist

Before launch, verify the deck reads cleanly to each of these audiences:

- [ ] **An EDU vendor** (Taylor's network) — do they see themselves in §02, §10 Column 1?
- [ ] **A potential angel investor** — does §06 GTM + §08 What's Left + §14 Closing tell a real revenue story?
- [ ] **A potential team hire** — does §13 Team + §12 Open Questions make this feel like a place where ideas matter?
- [ ] **A school admin** (eventual buyer) — does §10 Column 2 describe their actual pain?
- [ ] **An open-source contributor** — does §03 Architecture + §07 Revenue (open-core thesis) feel principled?
- [ ] **A skeptical reader** — does §12 Open Questions show we're thinking honestly, not selling fantasy?

---

## 8. Final delivery checklist for the building agent

- [ ] All 14 sections built per spec
- [ ] Brand kit applied exactly (no off-palette colors)
- [ ] All animations triggered on scroll, none on infinite loops (except hero orb pulse)
- [ ] Lighthouse score >90 on Performance and Accessibility
- [ ] Mobile-responsive (test 375px, 768px, 1280px, 1920px)
- [ ] Dark mode is the only mode — no light theme toggle needed
- [ ] All copy renders without orphan widows on common breakpoints
- [ ] Asset placeholder boxes for any missing images
- [ ] Deployable on Vercel/Netlify with a single command
- [ ] Source published to `github.com/myketheguru/travis-deck` (or similar)

---

*This spec describes the inaugural Travis pitch deck. It is meant to be iterated — sections may evolve as the conversation with prospects matures. Treat copy as v1.0; visual direction as binding.*
