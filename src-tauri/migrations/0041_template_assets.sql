-- v0.20.2+ Tier 4 — global, deduped, categorized binary asset library
-- extracted from sample/template PDFs.
--
-- Three tables, separating concerns:
--
-- 1. `template_extraction` — per-document extraction state. One row per
--    sample doc, tracks extraction lifecycle.
-- 2. `template_asset` — the actual binary asset. Content-addressed by
--    SHA-256; one row per unique image bytes. Carries kind ('logo',
--    'header_banner', 'signature', 'watermark', 'page_render',
--    'embedded_image') and a human display_name for LLM grounding.
-- 3. `template_asset_source` — N:M link asset ↔ source doc. The same
--    L2E logo extracted from invoice-sample-A and invoice-sample-B
--    lands as ONE asset row with TWO source rows. Future docs can
--    reference the asset by kind+display_name without needing the
--    original sample attached.
--
-- Pairs with `analyze_document_styling` (cached visual JSON). That tool
-- describes the layout; this one supplies the literal pixels.

CREATE TABLE IF NOT EXISTS template_extraction (
  document_id INTEGER PRIMARY KEY REFERENCES document(id) ON DELETE CASCADE,
  -- 'pending' | 'extracting' | 'ready' | 'failed'
  status TEXT NOT NULL DEFAULT 'pending',
  page_count INTEGER NOT NULL DEFAULT 0,
  image_count INTEGER NOT NULL DEFAULT 0,
  -- Per-doc manifest the LLM sees: pages + fonts + colors + the list
  -- of asset ids extracted from this doc with their bbox positions.
  manifest_json TEXT,
  error TEXT,
  extracted_at TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_template_extraction_status
  ON template_extraction(status);

CREATE TABLE IF NOT EXISTS template_asset (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  workspace_id INTEGER NOT NULL DEFAULT 1,
  -- SHA-256 of the image bytes. UNIQUE so the same logo extracted
  -- from twenty samples is one row, not twenty.
  content_hash TEXT NOT NULL UNIQUE,
  -- Absolute path under <app_data>/template_assets/<hash[:2]>/<hash>.png
  abs_path TEXT NOT NULL,
  -- 'logo' | 'header_banner' | 'signature' | 'watermark'
  --   | 'page_render' | 'embedded_image' | 'unknown'
  kind TEXT NOT NULL DEFAULT 'embedded_image',
  -- Human-friendly label for LLM grounding. Examples:
  --   "L2E logo (round)"
  --   "Header banner – navy + tan"
  --   "Signature block – Jacob Michelman"
  --   "Invoice page 1 render @ 300 DPI"
  -- Set heuristically at extraction time; can be refined later by a
  -- vision classification job or by the user.
  display_name TEXT NOT NULL DEFAULT '',
  width_px  INTEGER,
  height_px INTEGER,
  mime_type TEXT NOT NULL DEFAULT 'image/png',
  size_bytes INTEGER,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_template_asset_kind
  ON template_asset(workspace_id, kind);

CREATE INDEX IF NOT EXISTS idx_template_asset_name
  ON template_asset(workspace_id, display_name);

-- N:M asset ↔ source document. The same asset can come from many
-- samples; a single sample can yield many assets. Page + bbox here
-- so the LLM can ask "show me the logo's bounding box on the original
-- invoice" and replicate position when generating a new doc.
CREATE TABLE IF NOT EXISTS template_asset_source (
  asset_id INTEGER NOT NULL REFERENCES template_asset(id) ON DELETE CASCADE,
  document_id INTEGER NOT NULL REFERENCES document(id) ON DELETE CASCADE,
  page INTEGER NOT NULL DEFAULT 0,
  -- "[x0, top, x1, bottom]" in PDF points. NULL for page_render kind.
  bbox_pts TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  PRIMARY KEY (asset_id, document_id, page)
);

CREATE INDEX IF NOT EXISTS idx_template_asset_source_doc
  ON template_asset_source(document_id);
