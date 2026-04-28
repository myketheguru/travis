use std::path::Path;

use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

pub struct Db {
    pub pool: SqlitePool,
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct UserProfile {
    pub name: String,
    pub role: String,
    pub org: String,
    pub llm_provider: String,
    pub ollama_url: Option<String>,
    pub model: Option<String>,
    /// Free-form description of what the user/org does, who they serve,
    /// and what activities Travis should pay attention to. Embedded
    /// verbatim in system prompts when present. Optional.
    pub context_blurb: Option<String>,
    /// Optional voice/tone guidance, e.g. "warm and direct", "formal".
    pub communication_style: Option<String>,
}

impl UserProfile {
    /// First name for friendly addressing. Falls back to the full name
    /// when there's no whitespace to split on.
    pub fn first_name(&self) -> &str {
        self.name
            .split_whitespace()
            .next()
            .unwrap_or(&self.name)
    }

    /// Templated user-context block for embedding in LLM system prompts.
    /// Always opens with the structured "name / role / org" line; the
    /// optional blurb and communication style are appended only when set.
    /// Other modules should use this everywhere they previously hardcoded
    /// user identity, so the app works for any deployment.
    pub fn context_block(&self) -> String {
        let mut out = format!(
            "The user is {}. They are {} at {}.",
            self.name.trim(),
            self.role.trim(),
            self.org.trim(),
        );
        if let Some(blurb) = self.context_blurb.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
            out.push_str("\n\nContext about their work (use this to make examples + language relevant — never invent details beyond what's stated):\n");
            out.push_str(blurb);
        }
        if let Some(style) = self
            .communication_style
            .as_ref()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            out.push_str("\n\nPreferred voice: ");
            out.push_str(style);
            out.push('.');
        }
        out
    }
}

impl Db {
    pub async fn open(path: &Path) -> anyhow::Result<Self> {
        let opts = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .foreign_keys(true);

        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(opts)
            .await?;

        sqlx::migrate!("./migrations").run(&pool).await?;

        Ok(Self { pool })
    }

    pub async fn meta(&self, key: &str) -> anyhow::Result<Option<String>> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT value FROM meta WHERE key = ?1")
                .bind(key)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|r| r.0))
    }

    pub async fn set_meta(&self, key: &str, value: &str) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO meta(key, value, updated_at) VALUES (?1, ?2, CURRENT_TIMESTAMP)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn upsert_user_profile(&self, p: &UserProfile) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO user_profile (id, name, role, org, llm_provider, ollama_url, model,
                                       context_blurb, communication_style, updated_at)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, CURRENT_TIMESTAMP)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                role = excluded.role,
                org = excluded.org,
                llm_provider = excluded.llm_provider,
                ollama_url = excluded.ollama_url,
                model = excluded.model,
                context_blurb = excluded.context_blurb,
                communication_style = excluded.communication_style,
                updated_at = CURRENT_TIMESTAMP",
        )
        .bind(&p.name)
        .bind(&p.role)
        .bind(&p.org)
        .bind(&p.llm_provider)
        .bind(&p.ollama_url)
        .bind(&p.model)
        .bind(&p.context_blurb)
        .bind(&p.communication_style)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn user_profile(&self) -> anyhow::Result<Option<UserProfile>> {
        let row: Option<(
            String,
            String,
            String,
            String,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
        )> = sqlx::query_as(
            "SELECT name, role, org, llm_provider, ollama_url, model,
                    context_blurb, communication_style
             FROM user_profile WHERE id = 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(
            |(
                name,
                role,
                org,
                llm_provider,
                ollama_url,
                model,
                context_blurb,
                communication_style,
            )| UserProfile {
                name,
                role,
                org,
                llm_provider,
                ollama_url,
                model,
                context_blurb,
                communication_style,
            },
        ))
    }
}
