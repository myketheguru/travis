use std::path::Path;

use serde::Serialize;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

pub struct Db {
    pub pool: SqlitePool,
}

/// Meta keys reserved for per-install state — never sync these to the cloud.
/// Used by [`Db::set_meta`] to decide whether to enqueue a `settings.set`
/// change event.
fn is_internal_meta_key(key: &str) -> bool {
    key.starts_with("cloud_")
        || key.starts_with("sync_")
        || key.starts_with("travis_cloud_")
        || key.starts_with("previous_llm_")
        || key == "onboarded"
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
    /// Derived user-activity model — JSON written by the user_model
    /// background pass (BRAIN.md capability #3). Consumed by the
    /// persona block so Travis adapts timing + length without
    /// being told. Shape: src/persona/user_model.rs::UserModel.
    pub derived_model_json: Option<String>,
    /// Timestamp the derived_model was last refreshed.
    pub derived_model_at: Option<String>,
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
        crate::packs::run_pack_migrations(&pool).await?;

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

    /// Write a meta value and, for user-meaningful keys, enqueue a
    /// `settings.set` change so the cloud (and other devices) see it
    /// on the next sync. Keys with internal-flag prefixes (`cloud_`,
    /// `sync_`, `travis_cloud_`) are excluded from sync — those are
    /// per-install state that should NOT roam.
    ///
    /// The insert + outbox enqueue run in a single transaction so we
    /// can never end up with a local value that didn't queue (or an
    /// outbox row whose underlying value got rolled back).
    pub async fn set_meta(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO meta(key, value, updated_at) VALUES (?1, ?2, CURRENT_TIMESTAMP)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = CURRENT_TIMESTAMP",
        )
        .bind(key)
        .bind(value)
        .execute(&mut *tx)
        .await?;
        if !is_internal_meta_key(key) {
            let payload = serde_json::json!({ "key": key, "value": value }).to_string();
            sqlx::query(
                "INSERT INTO sync_outbox (kind, payload) VALUES ('settings.set', ?1)",
            )
            .bind(payload)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// Same as `set_meta` but does NOT enqueue. Used by the sync engine
    /// when applying a pulled remote event — we already know the cloud
    /// has it, so re-enqueueing would create a write loop.
    pub async fn set_meta_from_remote(&self, key: &str, value: &str) -> anyhow::Result<()> {
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
        let mut tx = self.pool.begin().await?;
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
        .execute(&mut *tx)
        .await?;
        // v2 Phase 2.3 — enqueue profile.set so the cloud (and other
        // devices) see the change on the next sync cycle. We omit the
        // llm_provider / ollama_url / model fields from the synced
        // payload — those are per-install (BYOK vs hosted) and never
        // roam.
        let payload = serde_json::json!({
            "name": p.name,
            "role": p.role,
            "org": p.org,
            "contextBlurb": p.context_blurb,
            "communicationStyle": p.communication_style,
        })
        .to_string();
        sqlx::query("INSERT INTO sync_outbox (kind, payload) VALUES ('profile.set', ?1)")
            .bind(payload)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(())
    }

    /// v2 Phase 2.3 — apply a pulled `profile.set` event. Bypasses the
    /// outbox so we don't re-emit our own writes. Only the synced
    /// fields are touched; llm_provider / ollama_url / model stay
    /// whatever the local install had.
    pub async fn upsert_user_profile_from_remote(
        &self,
        name: &str,
        role: &str,
        org: &str,
        context_blurb: Option<&str>,
        communication_style: Option<&str>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "INSERT INTO user_profile (id, name, role, org, llm_provider, ollama_url, model,
                                       context_blurb, communication_style, updated_at)
             VALUES (1, ?1, ?2, ?3, COALESCE((SELECT llm_provider FROM user_profile WHERE id=1), 'travis_cloud'),
                     (SELECT ollama_url FROM user_profile WHERE id=1),
                     (SELECT model FROM user_profile WHERE id=1),
                     ?4, ?5, CURRENT_TIMESTAMP)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                role = excluded.role,
                org = excluded.org,
                context_blurb = excluded.context_blurb,
                communication_style = excluded.communication_style,
                updated_at = CURRENT_TIMESTAMP",
        )
        .bind(name)
        .bind(role)
        .bind(org)
        .bind(context_blurb)
        .bind(communication_style)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// v0.20.2 — migrate any existing profile to default Travis Cloud
    /// IF this build shipped with a cloud key AND the user hasn't
    /// already been migrated (meta flag `travis_cloud_migrated_v020`).
    /// Their previous provider + model are preserved in meta keys so
    /// Settings can offer "switch back to <previous>" easily.
    ///
    /// Called once on app startup from setup(). Idempotent.
    pub async fn migrate_to_travis_cloud_if_needed(&self) -> anyhow::Result<()> {
        if !crate::llm::travis_cloud_available() {
            return Ok(());
        }
        let already: Option<(String,)> = sqlx::query_as(
            "SELECT value FROM meta WHERE key = 'travis_cloud_migrated_v020'",
        )
        .fetch_optional(&self.pool)
        .await?;
        if already.is_some() {
            return Ok(());
        }
        if let Some(profile) = self.user_profile().await? {
            if profile.llm_provider != "travis_cloud" {
                // Stash previous values so Settings can show "previously
                // <provider>" + "switch back" affordance.
                let _ = sqlx::query(
                    "INSERT INTO meta(key, value, updated_at)
                     VALUES (?1, ?2, CURRENT_TIMESTAMP)
                     ON CONFLICT(key) DO UPDATE SET
                       value = excluded.value,
                       updated_at = CURRENT_TIMESTAMP",
                )
                .bind("previous_llm_provider")
                .bind(&profile.llm_provider)
                .execute(&self.pool)
                .await;
                if let Some(model) = &profile.model {
                    let _ = sqlx::query(
                        "INSERT INTO meta(key, value, updated_at)
                         VALUES (?1, ?2, CURRENT_TIMESTAMP)
                         ON CONFLICT(key) DO UPDATE SET
                           value = excluded.value,
                           updated_at = CURRENT_TIMESTAMP",
                    )
                    .bind("previous_model")
                    .bind(model)
                    .execute(&self.pool)
                    .await;
                }
                // Flip the live provider to travis_cloud.
                sqlx::query(
                    "UPDATE user_profile SET llm_provider = 'travis_cloud', updated_at = CURRENT_TIMESTAMP WHERE id = 1",
                )
                .execute(&self.pool)
                .await?;
                tracing::info!(
                    "migrated user profile to travis_cloud (previous = {})",
                    profile.llm_provider
                );
            }
        }
        sqlx::query(
            "INSERT INTO meta(key, value, updated_at)
             VALUES ('travis_cloud_migrated_v020', '1', CURRENT_TIMESTAMP)
             ON CONFLICT(key) DO NOTHING",
        )
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
            Option<String>,
            Option<String>,
        )> = sqlx::query_as(
            "SELECT name, role, org, llm_provider, ollama_url, model,
                    context_blurb, communication_style,
                    derived_model_json, derived_model_at
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
                derived_model_json,
                derived_model_at,
            )| UserProfile {
                name,
                role,
                org,
                llm_provider,
                ollama_url,
                model,
                context_blurb,
                communication_style,
                derived_model_json,
                derived_model_at,
            },
        ))
    }
}
