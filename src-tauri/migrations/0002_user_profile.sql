CREATE TABLE IF NOT EXISTS user_profile (
    id           INTEGER PRIMARY KEY CHECK (id = 1),
    name         TEXT NOT NULL,
    role         TEXT NOT NULL,
    org          TEXT NOT NULL,
    llm_provider TEXT NOT NULL,
    ollama_url   TEXT,
    model        TEXT,
    created_at   TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at   TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

UPDATE meta SET value = '2' WHERE key = 'schema_version';
