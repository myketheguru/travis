-- Lead to Empower pack — program-delivery schema (the "3 A's" + catalog).
--
-- The pack's billing spine (coach, school, coach_hours, signing_sheet,
-- invoice) lives in core's 0003_domain.sql for migration-history
-- continuity. This is the FIRST pack-owned migration for L2E, tracked
-- in `meta.pack.lead-to-empower.schema_version` independent of core's
-- `_sqlx_migrations`. It adds the half the pack was missing: what LTE
-- sells (the 21-line Appendix F catalog) and how it delivers it (the
-- Assessment -> Action Planning -> Accountable Planning state machine).
--
-- Source of truth: NYC DOE MTAC #R1179 application package, digested
-- 2026-05-19. See LTE_PACK_SPEC.md.
--
-- Every table carries workspace_id from the start (per AUTHORING_PACKS.md
-- — only pre-workspace tables backfill via a later migration). Default 1
-- = the seeded Personal workspace from core 0020_workspaces.sql.
-- Auto-CRUD (packs_cmd.rs) stamps the active workspace on insert and
-- spine-syncs tables whose TableDef sets entity_kind, so no typed Rust
-- module is needed for these.

-- ---------------------------------------------------------------------------
-- catalog_module — the 21 priced product lines (Appendix F). Reference
-- data: seeded below, user-editable (LTE re-prices between solicitations).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS catalog_module (
    id                      INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id            INTEGER NOT NULL DEFAULT 1,
    line_no                 INTEGER NOT NULL,
    name                    TEXT NOT NULL,
    pillar                  TEXT NOT NULL
        CHECK (pillar IN ('leadership_development','dddm_teacher_effectiveness')),
    grade_band              TEXT,
    audience                TEXT,
    description             TEXT,
    list_price_cents        INTEGER NOT NULL DEFAULT 0,
    sessions                INTEGER NOT NULL DEFAULT 1,
    hours_per_session       REAL NOT NULL DEFAULT 0,
    duration_weeks          INTEGER NOT NULL DEFAULT 1,
    min_participants        INTEGER NOT NULL DEFAULT 1,
    max_participants        INTEGER NOT NULL DEFAULT 1,
    instructors_per_session INTEGER NOT NULL DEFAULT 1,
    kind                    TEXT NOT NULL DEFAULT 'workshop'
        CHECK (kind IN ('workshop','coaching','school_assessment')),
    created_at              TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at              TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_catalog_module_workspace ON catalog_module(workspace_id);
CREATE INDEX IF NOT EXISTS idx_catalog_module_line ON catalog_module(line_no);

-- ---------------------------------------------------------------------------
-- engagement — one 3 A's run at a school. The unit everything hangs off.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS engagement (
    id                        INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id              INTEGER NOT NULL DEFAULT 1,
    name                      TEXT NOT NULL,
    school_id                 INTEGER REFERENCES school(id) ON DELETE SET NULL,
    stage                     TEXT NOT NULL DEFAULT 'assessment'
        CHECK (stage IN ('assessment','action_planning','accountable','closed')),
    contract_ref              TEXT,
    school_year               TEXT,
    metrics_agreement_signed  INTEGER NOT NULL DEFAULT 0,
    metrics_signed_on         TEXT,
    summary                   TEXT,
    created_at                TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at                TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_engagement_workspace ON engagement(workspace_id);
CREATE INDEX IF NOT EXISTS idx_engagement_school ON engagement(school_id);
CREATE INDEX IF NOT EXISTS idx_engagement_stage ON engagement(stage);

-- ---------------------------------------------------------------------------
-- assessment — the A1 diagnostic. Many per engagement (a walkthrough + a
-- survey + a data pull all count). Mirrored to spine as an event.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS assessment (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id        INTEGER NOT NULL DEFAULT 1,
    engagement_id       INTEGER NOT NULL REFERENCES engagement(id) ON DELETE CASCADE,
    method              TEXT NOT NULL DEFAULT 'walkthrough'
        CHECK (method IN ('leadership_survey','personal_eval','walkthrough',
                          'observation','interview','data_analysis')),
    conducted_on        TEXT,
    rubric_score        REAL,
    recommended_focus   TEXT,
    summary             TEXT,
    created_at          TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at          TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_assessment_workspace ON assessment(workspace_id);
CREATE INDEX IF NOT EXISTS idx_assessment_engagement ON assessment(engagement_id);

-- ---------------------------------------------------------------------------
-- engagement_module — the A2 scope of work. Which catalog modules are in
-- scope for an engagement, at agreed (possibly non-list) terms.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS engagement_module (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id                INTEGER NOT NULL DEFAULT 1,
    engagement_id               INTEGER NOT NULL REFERENCES engagement(id) ON DELETE CASCADE,
    module_id                   INTEGER NOT NULL REFERENCES catalog_module(id) ON DELETE RESTRICT,
    planned_start               TEXT,
    planned_end                 TEXT,
    participant_count           INTEGER NOT NULL DEFAULT 0,
    agreed_price_cents          INTEGER NOT NULL DEFAULT 0,
    coaching_sessions_planned   INTEGER NOT NULL DEFAULT 0,
    notes                       TEXT,
    created_at                  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at                  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_engagement_module_workspace ON engagement_module(workspace_id);
CREATE INDEX IF NOT EXISTS idx_engagement_module_engagement ON engagement_module(engagement_id);
CREATE INDEX IF NOT EXISTS idx_engagement_module_module ON engagement_module(module_id);

-- ---------------------------------------------------------------------------
-- accountability_review — the A3 metrics checkpoint. ~3/year per the MTAC
-- (Sept baseline, Jan mid, May/June reflection). Mirrored to spine event.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS accountability_review (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id    INTEGER NOT NULL DEFAULT 1,
    engagement_id   INTEGER NOT NULL REFERENCES engagement(id) ON DELETE CASCADE,
    period          TEXT NOT NULL DEFAULT 'baseline_sep'
        CHECK (period IN ('baseline_sep','mid_jan','reflection_jun')),
    review_date     TEXT,
    metrics_json    TEXT,
    met             INTEGER,
    notes           TEXT,
    created_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at      TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_accountability_review_workspace ON accountability_review(workspace_id);
CREATE INDEX IF NOT EXISTS idx_accountability_review_engagement ON accountability_review(engagement_id);

-- ---------------------------------------------------------------------------
-- Bridge to billing: tag delivered hours to the engagement they served so
-- program work rolls up to the engagement and into money-at-risk views.
-- Soft FK, nullable — pre-existing core coach_hours rows keep NULL. Wired
-- into the typed coach_hours UI/queries in a later slice; the column +
-- index exist now so the join target is stable.
-- ---------------------------------------------------------------------------
ALTER TABLE coach_hours ADD COLUMN engagement_id INTEGER
    REFERENCES engagement(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_coach_hours_engagement ON coach_hours(engagement_id);

-- ---------------------------------------------------------------------------
-- Catalog seed — Appendix F, MTAC #R1179 (budgets revised 2022-03-01).
-- Prices stored as integer cents. Descriptions trimmed to one sentence for
-- readability; full 300-word Appendix F copy can be backfilled in a later
-- migration. Audience A = "Lead Teachers, assistant principals, coaches,
-- principals, and other school-based instructional or administrative
-- staff"; Audience B = "New and experienced teachers and other school or
-- administrative based staff". Grade band is all bands for every line.
-- ---------------------------------------------------------------------------
INSERT INTO catalog_module
  (workspace_id, line_no, name, pillar, grade_band, audience, description,
   list_price_cents, sessions, hours_per_session, duration_weeks,
   min_participants, max_participants, instructors_per_session, kind)
VALUES
  (1, 1,  'Authentic Leadership Module', 'leadership_development',
   'Elementary, Middle, High', 'Audience A',
   'Through assessment and reflection, participants self-assess, identify three strengths and three growth areas, and draft a personal vision and core values.',
   255600, 1, 8, 1, 2, 25, 2, 'workshop'),
  (1, 2,  'Visionary Leadership Module', 'leadership_development',
   'Elementary, Middle, High', 'Audience A',
   'Become conscious of bias and inequities; use emotional intelligence to get to the heart of an issue and generate a team vision.',
   343500, 3, 4, 4, 2, 25, 2, 'workshop'),
  (1, 3,  'Servant Leadership Module', 'leadership_development',
   'Elementary, Middle, High', 'Audience A',
   'Build relationships and trust with team members and stakeholders; use the vision to set clear roles, responsibilities, and common goals.',
   255600, 1, 8, 1, 2, 25, 2, 'workshop'),
  (1, 4,  'Instructional Leadership Part 1 Module', 'leadership_development',
   'Elementary, Middle, High', 'Audience A',
   'Why assessments matter and how they drive instructional decisions; participants evaluate current assessments and create an assessment plan.',
   365500, 4, 4, 5, 2, 25, 2, 'workshop'),
  (1, 5,  'Instructional Leadership Part 2 Module', 'leadership_development',
   'Elementary, Middle, High', 'Audience A',
   'The Data-Driven Decision-Making cycle; outline an implementation action plan and SMART goals aligned to school goals.',
   365500, 4, 4, 5, 2, 25, 2, 'workshop'),
  (1, 6,  'Instructional Leadership Part 3 Module', 'leadership_development',
   'Elementary, Middle, High', 'Audience A',
   'Develop systems to achieve an instructional vision; design a PD plan enabling at least two DDDM cycles before June.',
   365500, 4, 4, 5, 2, 25, 2, 'workshop'),
  (1, 7,  'Instructional Leadership Part 4 Module', 'leadership_development',
   'Elementary, Middle, High', 'Audience A',
   'Establish weekly, monthly, and quarterly routines to assess teaching, give feedback, and build teacher-team collaboration.',
   365500, 4, 4, 4, 2, 25, 2, 'workshop'),
  (1, 8,  'Instructional Leadership Part 5 Module', 'leadership_development',
   'Elementary, Middle, High', 'Audience A',
   'Sustain the instructional vision; design a weekly schedule aligned to instructional goals with a staff feedback loop.',
   365500, 4, 4, 4, 2, 25, 2, 'workshop'),
  (1, 9,  'Transformational Leadership Part 1 Module', 'leadership_development',
   'Elementary, Middle, High', 'Audience A',
   'Build high-performing teams through alignment and empowerment; design a school governance with the leader strategically positioned.',
   299600, 3, 4, 3, 2, 25, 2, 'workshop'),
  (1, 10, 'Transformational Leadership Part 2 Module', 'leadership_development',
   'Elementary, Middle, High', 'Audience A',
   'Discover the value of the school and external community; build an action plan to grow culture and parent/community involvement.',
   299600, 3, 4, 3, 2, 25, 2, 'workshop'),
  (1, 11, 'Transformational Leadership Part 3 Module', 'leadership_development',
   'Elementary, Middle, High', 'Audience A',
   'Identify staff strengths and talent; revise schedules and systems to build capacity and leadership among staff.',
   299600, 3, 4, 4, 2, 25, 2, 'workshop'),
  (1, 12, 'Adaptive Leadership Module', 'leadership_development',
   'Elementary, Middle, High', 'Audience A',
   'Navigate challenge and crisis; revisit authentic leadership and propose a Crisis Management/Task Force team.',
   365500, 2, 8, 2, 2, 25, 2, 'workshop'),
  (1, 13, 'Self-Reflection Methods for Learning and Effectiveness Module',
   'dddm_teacher_effectiveness', 'Elementary, Middle, High', 'Audience B',
   'Gather evidence to support conclusions; collect and analyze data and determine evidence-based action steps.',
   321600, 2, 5, 1, 2, 25, 2, 'workshop'),
  (1, 14, 'Equity and Access to Drive Relationship Building Module',
   'dddm_teacher_effectiveness', 'Elementary, Middle, High', 'Audience B',
   'Establish why assessments matter and how they drive instructional decisions; create an assessment plan.',
   367000, 2, 6, 2, 2, 25, 2, 'workshop'),
  (1, 15, 'Assessment and Analysis Module', 'dddm_teacher_effectiveness',
   'Elementary, Middle, High', 'Audience B',
   'Develop data-driven systems to assess the school; launch initiatives, measure implementation, and develop an action plan with measurable outcomes.',
   388900, 4, 4, 6, 5, 50, 2, 'workshop'),
  (1, 16, 'Developing Data-Driven Practices Module', 'dddm_teacher_effectiveness',
   'Elementary, Middle, High', 'Audience B',
   'Develop productive student discourse and systems for student independence; teaching strategies aligned to a SMART goal, measured via assessments.',
   476900, 5, 4, 6, 2, 25, 2, 'workshop'),
  (1, 17, 'Leadership Coaching', 'leadership_development',
   'Elementary, Middle, High', 'Audience A',
   'Individual coaching: assessment-driven growth plan, learning goal, and action plan with coaching to ensure the goal is met.',
   299300, 4, 4, 4, 1, 5, 1, 'coaching'),
  (1, 18, 'Instructional Coaching', 'dddm_teacher_effectiveness',
   'Elementary, Middle, High', 'Audience B',
   'Individual coaching: assessment-driven growth plan, learning goal, and action plan with coaching to ensure the goal is met.',
   294900, 4, 4, 4, 1, 5, 1, 'coaching'),
  (1, 19, 'Data Coaching', 'dddm_teacher_effectiveness',
   'Elementary, Middle, High', 'Audience B',
   'Individual coaching: assessment-driven growth plan, learning goal, and action plan with coaching to ensure the goal is met.',
   353200, 5, 4, 4, 1, 5, 1, 'coaching'),
  (1, 20, 'School Assessment (Leadership)', 'leadership_development',
   'Elementary, Middle, High', 'Audience A',
   'Evaluate the school on systems, instructional focus, DDDM, and alignment to vision/goals via interviews, observations, and data analysis.',
   346100, 2, 7, 1, 30, 200, 2, 'school_assessment'),
  (1, 21, 'School Assessment (Data and Teacher Effectiveness)',
   'dddm_teacher_effectiveness', 'Elementary, Middle, High', 'Audience B',
   'Evaluate the school on data sources, analysis methods, feedback processes, inquiry/DDDM cycles, and student learning.',
   346100, 2, 7, 1, 30, 200, 2, 'school_assessment');
