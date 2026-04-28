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
            "INSERT INTO user_profile (id, name, role, org, llm_provider, ollama_url, model, updated_at)
             VALUES (1, ?1, ?2, ?3, ?4, ?5, ?6, CURRENT_TIMESTAMP)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                role = excluded.role,
                org = excluded.org,
                llm_provider = excluded.llm_provider,
                ollama_url = excluded.ollama_url,
                model = excluded.model,
                updated_at = CURRENT_TIMESTAMP",
        )
        .bind(&p.name)
        .bind(&p.role)
        .bind(&p.org)
        .bind(&p.llm_provider)
        .bind(&p.ollama_url)
        .bind(&p.model)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn user_profile(&self) -> anyhow::Result<Option<UserProfile>> {
        let row: Option<(String, String, String, String, Option<String>, Option<String>)> =
            sqlx::query_as(
                "SELECT name, role, org, llm_provider, ollama_url, model
                 FROM user_profile WHERE id = 1",
            )
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|(name, role, org, llm_provider, ollama_url, model)| UserProfile {
            name,
            role,
            org,
            llm_provider,
            ollama_url,
            model,
        }))
    }
}
