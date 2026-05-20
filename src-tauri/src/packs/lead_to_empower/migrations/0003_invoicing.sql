-- Lead to Empower pack — invoicing document layer (LTE_INVOICING_SPEC.md).
--
-- Third pack-owned migration (meta.pack.lead-to-empower.schema_version
-- = 3). Adds the document chain that wraps the engagement_module scope
-- items into the four-document NYC DOE billing flow: Work Order
-- (vendor-issued, school-countersigned) → Purchase Order (DOE-issued,
-- received) → Sign-in Sheet (existing) → Invoice (vendor-issued,
-- school-signed). Each invoice now has typed line items pointing back
-- at the engagement_module rows they bill, with snapshot qty +
-- unit_price so post-send scope edits don't rewrite history.
--
-- Source: the COO's recorded walkthrough + the PS/MS 498 document set
-- (PO WR260363316, invoice LTE2064981) digested 2026-05-20. See
-- LTE_INVOICING_SPEC.md for full design.
--
-- ALTERs on engagement_module / invoice / coach_hours are nullable or
-- default-safe — existing rows stay intact.

-- ---------------------------------------------------------------------------
-- company_profile — the brand strip parameterized for every PDF.
-- Single row per workspace. Seeded with LTE defaults on first install;
-- the user edits via Settings → Company. Templates pull from here so
-- a sibling consultancy can swap the row and reuse every PDF.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS company_profile (
    id                                    INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id                          INTEGER NOT NULL DEFAULT 1 UNIQUE,
    name                                  TEXT NOT NULL DEFAULT 'Lead to Empower',
    legal_name                            TEXT,
    address_line_1                        TEXT,
    address_line_2                        TEXT,
    city                                  TEXT,
    state                                 TEXT,
    zip                                   TEXT,
    phone                                 TEXT,
    email                                 TEXT,
    website                               TEXT,
    ein                                   TEXT,
    nyc_doe_vendor_number                 TEXT,
    default_contract_ref                  TEXT,
    tagline                               TEXT,
    logo_path                             TEXT,
    default_invoice_signature_authority   TEXT,
    created_at                            TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at                            TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_company_profile_workspace ON company_profile(workspace_id);

-- Seed: LTE defaults from the MTAC #R1179 application package + the
-- PS 498 invoice letterhead. INSERT OR IGNORE keyed on workspace_id
-- so re-runs and pre-existing rows are safe.
INSERT OR IGNORE INTO company_profile
  (workspace_id, name, legal_name, address_line_1, city, state, zip, phone,
   email, website, ein, nyc_doe_vendor_number, default_contract_ref,
   tagline, default_invoice_signature_authority)
VALUES
  (1, 'Lead to Empower', 'Lead to Empower LLC', '533 West 141st St.',
   'New York', 'NY', '10031', '212-222-4275',
   'jacob@leadtoempower.com', 'www.leadtoempower.com',
   '82-4991893', 'LEA991893', 'QR179CF',
   'Because Leading is Not Enough',
   'Jacob Michelman');

-- ---------------------------------------------------------------------------
-- work_order — the engagement contract artifact. One per engagement.
-- Vendor-issued, school-countersigned (Q4: principal signs all school-
-- facing docs). Scope items live on engagement_module — the WO is the
-- document that wraps them with signatures + a date.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS work_order (
    id                       INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id             INTEGER NOT NULL DEFAULT 1,
    engagement_id            INTEGER NOT NULL REFERENCES engagement(id) ON DELETE CASCADE,
    contract_ref             TEXT,
    date_issued              TEXT,
    vendor_signed_at         TEXT,
    vendor_signed_by_name    TEXT,
    school_signed_at         TEXT,
    school_signed_by_name    TEXT,
    total_cents              INTEGER NOT NULL DEFAULT 0,
    pdf_path                 TEXT,
    notes                    TEXT,
    created_at               TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at               TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_work_order_workspace ON work_order(workspace_id);
CREATE INDEX IF NOT EXISTS idx_work_order_engagement ON work_order(engagement_id);
CREATE INDEX IF NOT EXISTS idx_work_order_date ON work_order(date_issued);

-- ---------------------------------------------------------------------------
-- purchase_order — the school's authorization to bill. Inbound from
-- NYC DOE (we don't author POs; Taylor uploads the PDF she receives).
-- One per engagement, typically. Carries the WR… PO number — the
-- external identifier every downstream document references.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS purchase_order (
    id                          INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id                INTEGER NOT NULL DEFAULT 1,
    engagement_id               INTEGER NOT NULL REFERENCES engagement(id) ON DELETE CASCADE,
    work_order_id               INTEGER REFERENCES work_order(id) ON DELETE SET NULL,
    po_number                   TEXT NOT NULL,
    suffix                      TEXT NOT NULL DEFAULT '01',
    tracking_number             TEXT,
    po_date                     TEXT,
    activity_start              TEXT NOT NULL,
    activity_end                TEXT NOT NULL,
    deliver_to_attention        TEXT,
    deliver_to_address          TEXT,
    deliver_to_phone            TEXT,
    special_delivery            TEXT,
    authorized_by               TEXT,
    authorized_at               TEXT,
    total_cents                 INTEGER NOT NULL DEFAULT 0,
    pdf_path                    TEXT,
    notes                       TEXT,
    created_at                  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at                  TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (workspace_id, po_number),
    CHECK (activity_start <= activity_end)
);
CREATE INDEX IF NOT EXISTS idx_purchase_order_workspace ON purchase_order(workspace_id);
CREATE INDEX IF NOT EXISTS idx_purchase_order_engagement ON purchase_order(engagement_id);
CREATE INDEX IF NOT EXISTS idx_purchase_order_wo ON purchase_order(work_order_id);
CREATE INDEX IF NOT EXISTS idx_purchase_order_number ON purchase_order(po_number);
CREATE INDEX IF NOT EXISTS idx_purchase_order_window ON purchase_order(activity_start, activity_end);

-- ---------------------------------------------------------------------------
-- invoice_line — multi-line invoice support. One row per billable line.
-- Snapshots qty + unit_price at billing time so engagement_module edits
-- after an invoice is sent don't rewrite history. The PS 498 invoice
-- (LTE2064981) has two lines (Data Coaching + Leadership Coaching);
-- the single-rate columns on `invoice` can't represent that natively.
--
-- Validator at draft→sent: line.unit_price_cents must equal
-- engagement_module.agreed_price_cents (or catalog_module.list_price_cents
-- if agreed is 0). The PS 498 invoice would refuse on Leadership being
-- billed at $5,013.30/unit when the catalog says $2,993.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS invoice_line (
    id                      INTEGER PRIMARY KEY AUTOINCREMENT,
    workspace_id            INTEGER NOT NULL DEFAULT 1,
    invoice_id              INTEGER NOT NULL REFERENCES invoice(id) ON DELETE CASCADE,
    engagement_module_id    INTEGER NOT NULL REFERENCES engagement_module(id) ON DELETE RESTRICT,
    description             TEXT NOT NULL,
    qty                     REAL NOT NULL DEFAULT 1.0,
    unit_price_cents        INTEGER NOT NULL DEFAULT 0,
    subtotal_cents          INTEGER NOT NULL DEFAULT 0,
    date_list               TEXT,
    sort_order              INTEGER NOT NULL DEFAULT 0,
    created_at              TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);
CREATE INDEX IF NOT EXISTS idx_invoice_line_workspace ON invoice_line(workspace_id);
CREATE INDEX IF NOT EXISTS idx_invoice_line_invoice ON invoice_line(invoice_id);
CREATE INDEX IF NOT EXISTS idx_invoice_line_module ON invoice_line(engagement_module_id);

-- ---------------------------------------------------------------------------
-- Additive columns on existing tables.
-- ---------------------------------------------------------------------------

-- engagement_module.qty — billable units per module. PS 498's PO line
-- "2 DAYS Data Coaching @ $3,532" stores as (module=Data Coaching,
-- agreed_price=$3,532, qty=2.0). Existing rows backfill to 1.0.
ALTER TABLE engagement_module ADD COLUMN qty REAL NOT NULL DEFAULT 1.0;

-- invoice.engagement_id — direct link to the engagement billed.
ALTER TABLE invoice ADD COLUMN engagement_id INTEGER
    REFERENCES engagement(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_invoice_engagement ON invoice(engagement_id);

-- invoice.purchase_order_id — the PO this invoice bills against.
-- Period checks reference purchase_order.activity_start/end via this FK.
ALTER TABLE invoice ADD COLUMN purchase_order_id INTEGER
    REFERENCES purchase_order(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_invoice_po ON invoice(purchase_order_id);

-- invoice.school_signed_* — the principal's countersignature on the
-- invoice itself (the "Authorized by ___ Date ___" block on the LTE
-- letterhead). Required by Q4: principal signs all school-facing docs.
ALTER TABLE invoice ADD COLUMN school_signed_at TEXT;
ALTER TABLE invoice ADD COLUMN school_signed_by_name TEXT;

-- invoice.submitted_to_polaris_at — manual marker for the Polaris
-- portal upload. Travis can surface "unsubmitted invoices" once this
-- exists; we don't have a Polaris API.
ALTER TABLE invoice ADD COLUMN submitted_to_polaris_at TEXT;

-- coach_hours.engagement_module_id — sign-in rows tagged with WHICH
-- scope item they served. Makes the per-line date_list on the invoice
-- computable, and catches the kind of cross-line misclassification
-- that hit the PS 498 invoice (4/30 Data Coaching listed as
-- Leadership). engagement_id already exists from 0001.
ALTER TABLE coach_hours ADD COLUMN engagement_module_id INTEGER
    REFERENCES engagement_module(id) ON DELETE SET NULL;
CREATE INDEX IF NOT EXISTS idx_coach_hours_engagement_module
    ON coach_hours(engagement_module_id);
