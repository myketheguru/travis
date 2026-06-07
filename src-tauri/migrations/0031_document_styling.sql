-- v0.14.0 Slice 4 — cached visual styling analysis for documents.
--
-- analyze_document_styling tool sends the PDF to Claude vision with a
-- styling-extraction prompt; the JSON result (colors, fonts, table
-- structure, signature placement, layout hints) gets cached here so
-- subsequent code generations against the same sample don't re-pay
-- the vision call.

ALTER TABLE document ADD COLUMN styling_json TEXT;
ALTER TABLE document ADD COLUMN styling_analyzed_at TEXT;
