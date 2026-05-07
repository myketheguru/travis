# Travis — Vertical Target Market

Travis goes vertical-first: own a small number of operational niches
deeply rather than chase horizontal volume. This file is the working
list of candidate verticals, ranked by how reachable they are from
where Travis is today.

For why vertical-first see [POSITIONING.md](./POSITIONING.md).
For when each vertical is unblocked by the build see
[ROADMAP.md](./ROADMAP.md).

Filter applied to every entry below: **structured operations, a
non-technical operator audience, painful or expensive existing tools,
and clear ROI per seat.** Verticals that don't pass all four are not
on this list.

---

## Tier A — Same shape as L2E, ship a pack and you're selling tomorrow

These reuse roughly 80% of the L2E pack (contractors → sites → hours →
signed proof → payer invoiced). Travis can sell into them right after
Phase 1 of the roadmap lands. All desktop-primary; no mobile or voice
required.

1. **After-school / enrichment program operators.** ~30k US orgs
   running music, sports, STEM, art programs in schools. Currently on
   Excel + ProCare + paper signing sheets. **WTP $200–400/mo.**
   Travis's first customer (your COO at L2E) is exactly this shape —
   first-best wedge.
2. **Sports coaching businesses (private clubs, league operators).**
   Coaches placed at facilities, billable hours, parent invoicing.
   **WTP $150–300/mo.**
3. **Tutoring & test-prep agencies.** Tutors paired with students
   (in-person or remote), session logs, progress reports to parents,
   billing. ~20k US orgs. **WTP $200–500/mo.**
4. **Non-medical home care agencies.** Caregivers to clients' homes,
   hourly billing, signed visit notes (Medicaid-required), insurance +
   family invoicing. ~12k US agencies, heavy compliance burden.
   **WTP $300–800/mo.** Big willingness-to-pay because compliance.
5. **Cleaning & janitorial services (small operators).** Cleaners to
   sites, photo proof of completion, recurring invoicing. Currently on
   Jobber / Housecall Pro at $100+/mo. **WTP $100–250/mo.**

## Tier B — Therapy / counseling-shaped: records-heavy, painful incumbents

Different shape (no contractors-at-sites; clinicians have caseloads),
but operationally rich. The existing tools (SimplePractice,
TherapyNotes) are 15-year-old and very expensive. **Needs Phase 5 +
Phase 6 of the roadmap (audit-log + encrypted state) before serious
sales — HIPAA bar.**

6. **Therapy / counseling private practices.** Clinicians with
   caseloads, HIPAA-compliant session notes, insurance + private-pay
   billing. **WTP $50–150/seat/mo.**
7. **Speech-language pathology / occupational therapy itinerant
   practices.** Therapists travel between schools / homes / clinics,
   very L2E-shaped (contractor, site, signed log, billing payer).
   Often Medicaid-billed. **WTP $200–500/mo.**
8. **Independent psychiatric practices.** Same as therapy plus
   prescription tracking. **WTP $100–250/seat/mo.**
9. **Doula / midwife / birth professional practices.** Clients,
   appointments, on-call rotations, postpartum visit logs.
   Underserved market. **WTP $50–150/mo.**
10. **Trauma-informed coaching / non-clinical mental wellness
    practices.** Growing market, less regulatory burden than therapy,
    similar caseload shape. **WTP $50–150/mo.**

## Tier C — Field service variants (blocked on Phase 9 / mobile)

Bigger markets than the above, but Travis needs mobile capture first —
techs are in the field, not at desks. Hold off serious sales motion
until the mobile companion ships.

11. **Independent HVAC / plumbing / electrical contractors.** Techs
    dispatched to job sites, time on job, parts used, signed work
    orders. ServiceTitan owns enterprise but starts at $300+/seat.
    **WTP $200–400/mo** for SMBs.
12. **Pest control companies (small / mid).** Techs to properties,
    EPA-regulated product logs, recurring service contracts.
    **WTP $150–300/mo.**
13. **Lawn care / landscaping / snow removal crews.** Crews to
    properties, recurring invoicing, photo proof.
    **WTP $100–250/mo.**
14. **Pool service operators.** Same shape as lawn care, narrower
    market. **WTP $100–200/mo.**
15. **Mobile pet grooming / mobile vet services.** Techs to clients,
    routes, recurring appointments. **WTP $100–200/mo.**

## Tier D — Records-heavy professional services

Heavier on document workflows, lighter on hours-tracking. Different
pack shape — file-and-deliverable-centric. Each requires deeper
integration into existing systems (court e-filing, IRS e-file,
insurance EDI). Save for after the pack-marketplace shape solidifies.

16. **Independent legal practices (solo + 2–5 attorney).** Case files,
    billable hours, court deadlines, client invoicing. Heavy
    compliance + privilege concerns. Existing tools (Clio, MyCase)
    are expensive and dated. **WTP $100–300/seat/mo.** Big TAM, big
    incumbents.
17. **Independent CPA / bookkeeping firms.** Client files, deadlines
    (tax dates), billable hours, deliverables. Painful seasonal work.
    **WTP $100–250/seat/mo.**
18. **Independent appraisers (real estate, insurance, art).** Jobs,
    comp pulls, deliverable reports, billing. Niche but high WTP per
    job. **WTP $100–200/mo.**
19. **Translation / interpretation agencies.** Translators, jobs,
    words/hours, deliverables, multi-language pairs. Often itinerant
    interpreters (court / medical) — same L2E shape.
    **WTP $200–500/mo.**
20. **IT MSPs (small managed service providers).** Clients, tickets,
    recurring services, hours, invoicing. ConnectWise / Datto own
    enterprise but are clunky and expensive. **WTP $200–500/mo.**
    Devs love AI tools — this is also where you get the most
    evangelism for free.

---

## Reading the list strategically

- **The L2E shape generalises further than just "education ops."**
  Tiers A + item 19 are all variants of "people sent to places, hours
  billed, signed proof, payer invoiced." That's the core pack.
  Roughly 60% of the list is reachable with one well-built pack
  (`contractors-and-hours` + `invoicing` + `signed-sheets` +
  per-vertical thin overlay).
- **Healthcare-adjacent (Tier B + parts of Tier C) is the highest WTP
  per seat** but requires HIPAA-grade audit / encryption work that
  lines up with the Phase 5 + 6 roadmap. Don't sell into therapy
  until those land — but it'll be the highest-margin vertical when
  they do.
- **Field service (Tier C) is the biggest TAM by far** but blocked on
  mobile (Phase 9). Worth holding back until mobile is solid.
- **Tier D verticals are the path to $1k–5k/mo per customer** but
  each requires deeper integration into existing tools (court
  e-filing, IRS e-file, insurance EDI). Save for after the
  pack-marketplace shape solidifies.

## What to target next month

Pick **two from Tier A**, sell deep, prove the pack abstraction with
real customers paying real money:

1. **#1 (After-school / enrichment program operators)** — your COO is
   already that customer; her network has more like her. Land 5–10
   paying orgs at $300/mo and you've validated the shape, generated
   $1.5–3k/mo recurring, and proven the model.
2. **#3 (Tutoring & test-prep agencies)** *or* **#4 (Non-medical home
   care)** as the second wedge. Tutoring is closer (lower-friction
   sale, lower regulatory bar). Home care is higher willingness-to-pay
   (because compliance pain) but takes longer to close. Pick based on
   how patient your sales pipeline can be.

**Avoid picking from three tiers at once.** Focus is what makes
vertical-first work. Each vertical you commit to is a sales cycle, a
content marketing arc, a few demo videos, and a referral motion.
Doing two well beats doing five badly.

## How this maps to the "Jarvis / Linux of AI" arc

Each vertical pack Travis ships is a piece of the standard library.
By the time there are packs for after-school, tutoring, home care,
cleaning, therapy, legal, and IT MSP — that's the Linux distribution
shape: a kernel (Travis core) + repositories of installable software
(packs) + community contributions. The Jarvis ambition isn't built by
chasing it directly — it's the emergent shape that comes out of
having (a) the universal client, (b) 30+ operational packs that prove
the abstraction, and (c) the cloud + identity + sync layer that makes
one Travis follow you everywhere.
