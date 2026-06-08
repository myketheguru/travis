-- v0.16.3 — Memory decay + pinning columns on the claim table.
--
-- The AutoMem research synthesis (v0.15.x) flagged this: without a
-- forgetting policy or relevance decay, recall quality monotonically
-- degrades as the store grows. More candidates → more near-misses →
-- noisier reranking. AutoMem ships forgetting *off by default*; we're
-- shipping it on.
--
-- Two new columns:
--   relevance_score — float in [0.0, 1.0]. Defaults to 1.0 (fresh).
--                     A background scheduler decays unpinned claims
--                     exponentially: ~0.5% per day, ~180-day half-life.
--   pinned          — 0 or 1. Pinned claims never decay. Used for
--                     user-confirmed claims, durable facts the user
--                     explicitly told Travis to remember.
--
-- Recall queries (memory::claims) can use relevance_score as a weight
-- when ranking candidates. Stale claims naturally sink; fresh + pinned
-- claims surface first.

ALTER TABLE claim ADD COLUMN relevance_score REAL NOT NULL DEFAULT 1.0;
ALTER TABLE claim ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;

CREATE INDEX IF NOT EXISTS idx_claim_relevance
    ON claim(relevance_score DESC)
    WHERE superseded_at IS NULL;
