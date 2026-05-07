//! Pack registry — the extension surface for vertical packs.
//!
//! See PACKS.md for the full format spec. A pack contributes:
//! - migrations against the core SQLite DB
//! - tools that the LLM can invoke
//! - action handlers (mapped through actions::dispatch)
//! - a system-prompt fragment
//! - declared entity kinds (so identity::record_mention accepts them)
//! - declared action kinds (registered with the action dispatcher)
//!
//! For v0.2, packs are compiled in. Their migrations are bundled at compile
//! time as `&'static [PackMigration]`. User-installable packs (drag-and-drop
//! `.zip` → run) come in Phase 2 of the roadmap.

use sqlx::SqlitePool;

/// A bundled pack. All methods take `&self` so [`PackHandle`] can live behind
/// a `&'static dyn PackHandle` reference returned from [`enabled_packs`].
pub trait PackHandle: Send + Sync {
    /// Stable identifier — must match the `[pack].slug` field in the
    /// pack manifest. Lowercase, hyphens, no whitespace.
    fn slug(&self) -> &'static str;

    /// Human-facing name shown in UI and logs.
    fn name(&self) -> &'static str;

    /// Pack version, semver. Compared against `pack.travis_min` in the
    /// manifest at install time (currently a no-op for compiled-in packs).
    fn version(&self) -> &'static str;

    /// Migrations the pack contributes, in apply order. Numbering is
    /// independent of core's `_sqlx_migrations`; tracked per-pack in
    /// `meta.pack.<slug>.schema_version`.
    fn migrations(&self) -> &'static [PackMigration] {
        &[]
    }

    /// Optional system-prompt fragment. Concatenated into Travis's system
    /// prompt according to the manifest's `system_prompt.mode` (append by
    /// default — currently the only supported mode in v0.2).
    fn prompt_fragment(&self) -> Option<&'static str> {
        None
    }

    /// Entity kinds this pack declares. Used to allow
    /// [`crate::identity::record_mention`] for these kinds, and to populate
    /// the journal extraction schema with the right buckets.
    fn entity_kinds(&self) -> &'static [&'static str] {
        &[]
    }

    /// Action kinds this pack registers handlers for. The pack's
    /// [`PackHandle::register_actions`] implementation must install handlers
    /// matching these names with the action dispatcher.
    fn action_kinds(&self) -> &'static [&'static str] {
        &[]
    }

    /// Add this pack's LLM tools to the read-only registry. Called once
    /// at startup from `lib.rs` after the core tool list is built.
    /// Default: no tools.
    fn register_tools(&self, registry: &mut crate::tools::ToolRegistry) {
        let _ = registry;
    }

    /// Add this pack's action handlers to the action registry. Called
    /// once at startup before [`AppState`] is constructed. Default: no
    /// handlers.
    fn register_actions(&self, registry: &mut crate::actions::ActionRegistry) {
        let _ = registry;
    }
}

/// A single SQL migration file, bundled into the binary at compile time.
#[derive(Debug, Clone)]
pub struct PackMigration {
    /// Display name for logs, e.g. "0001_init".
    pub name: &'static str,
    /// Raw SQL. Will be executed verbatim against the core DB.
    pub sql: &'static str,
}

/// Packs compiled into this build. For v0.2 this is empty — the L2E pack
/// is added in step 8 of the refactor (see PACKS_AUDIT.md).
pub fn enabled_packs() -> &'static [&'static dyn PackHandle] {
    &[]
}

/// Run pending migrations for every enabled pack. Idempotent — uses
/// `meta.pack.<slug>.schema_version` to track the highest-applied number
/// and skips anything already done.
///
/// Called once from `db::Db::open` after core migrations succeed.
pub async fn run_pack_migrations(pool: &SqlitePool) -> anyhow::Result<()> {
    for pack in enabled_packs() {
        run_one_pack(pool, *pack).await?;
    }
    Ok(())
}

async fn run_one_pack(pool: &SqlitePool, pack: &dyn PackHandle) -> anyhow::Result<()> {
    let key = format!("pack.{}.schema_version", pack.slug());
    let current = current_version(pool, &key).await?;

    let mut highest = current;
    for (i, m) in pack.migrations().iter().enumerate() {
        let n = (i as i64) + 1;
        if n <= current {
            continue;
        }
        sqlx::query(m.sql)
            .execute(pool)
            .await
            .map_err(|e| anyhow::anyhow!("pack {} migration {}: {e}", pack.slug(), m.name))?;
        tracing::info!(
            "pack {}: applied migration {} ({})",
            pack.slug(),
            n,
            m.name
        );
        highest = n;
    }

    if highest > current {
        sqlx::query(
            "INSERT INTO meta(key, value, updated_at)
             VALUES (?1, ?2, CURRENT_TIMESTAMP)
             ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updated_at = CURRENT_TIMESTAMP",
        )
        .bind(&key)
        .bind(highest.to_string())
        .execute(pool)
        .await?;
    }

    Ok(())
}

async fn current_version(pool: &SqlitePool, key: &str) -> anyhow::Result<i64> {
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM meta WHERE key = ?1")
        .bind(key)
        .fetch_optional(pool)
        .await?;
    Ok(row.and_then(|(v,)| v.parse::<i64>().ok()).unwrap_or(0))
}

/// True when a pack with the given slug is enabled in this build. Used by
/// the frontend gating logic (`app_status` exposes this) to show or hide
/// pack-supplied UI tabs.
pub fn is_enabled(slug: &str) -> bool {
    enabled_packs().iter().any(|p| p.slug() == slug)
}
