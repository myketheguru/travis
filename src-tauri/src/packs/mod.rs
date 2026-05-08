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
//! ## Two layers of pack gating
//!
//! 1. **Compile-time** — Cargo feature `pack-<slug>` controls whether the
//!    pack module compiles into the binary. [`compiled_in_packs`] returns
//!    every pack that did. Used by distros / contributors building narrow
//!    binaries (`--no-default-features --features pack-tutoring`).
//!
//! 2. **Runtime** — `meta.pack.<slug>.enabled` (per-DB) controls whether
//!    a compiled-in pack actually participates. [`resolve_enabled_packs`]
//!    reads the flag and filters; the resolved list lives on
//!    [`crate::AppState::enabled_packs`]. Toggling requires app restart
//!    because action/tool registries are built once at startup.
//!
//! Migrations run for every compiled-in pack regardless of the runtime
//! flag — that way toggling a pack on later doesn't trigger a migration
//! that could fail. Cost is empty unused tables for disabled packs
//! (negligible in SQLite).

use sqlx::SqlitePool;

#[cfg(feature = "pack-lead-to-empower")]
pub mod lead_to_empower;

#[cfg(feature = "pack-tutoring")]
pub mod tutoring;

/// A bundled pack. All methods take `&self` so [`PackHandle`] can live behind
/// a `&'static dyn PackHandle` reference returned from [`compiled_in_packs`].
pub trait PackHandle: Send + Sync {
    /// Stable identifier — must match the `[pack].slug` field in the
    /// pack manifest. Lowercase, hyphens, no whitespace.
    fn slug(&self) -> &'static str;

    /// Human-facing name shown in UI and logs.
    fn name(&self) -> &'static str;

    /// Pack version, semver. Compared against `pack.travis_min` in the
    /// manifest at install time (currently a no-op for compiled-in packs).
    fn version(&self) -> &'static str;

    /// One-line description of the vertical this pack supports. Shown to
    /// users in the onboarding pack picker and Settings → Packs panel.
    fn description(&self) -> &'static str {
        ""
    }

    /// First-encounter state — whether this pack should be enabled by
    /// default the first time a user encounters it (no `meta.pack.<slug>.
    /// enabled` row exists yet). Existing packs that shipped before
    /// runtime selection landed return `true` to preserve their users'
    /// experience. New packs returning `false` requires the user to opt
    /// in via onboarding or Settings → Packs.
    fn default_enabled(&self) -> bool {
        false
    }

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
    /// once at startup before [`crate::AppState`] is constructed. Default:
    /// no handlers.
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

/// Every pack that's been compiled into this build. The Cargo features
/// `pack-<slug>` control which entries are present.
pub fn compiled_in_packs() -> &'static [&'static dyn PackHandle] {
    &[
        #[cfg(feature = "pack-lead-to-empower")]
        &lead_to_empower::LeadToEmpowerPack,
        #[cfg(feature = "pack-tutoring")]
        &tutoring::TutoringPack,
    ]
}

/// Format the meta key that holds a pack's runtime-enabled flag.
fn enabled_meta_key(slug: &str) -> String {
    format!("pack.{slug}.enabled")
}

/// Read a pack's `meta.pack.<slug>.enabled` flag, falling back to the
/// pack's [`PackHandle::default_enabled`] when no row exists.
pub async fn is_pack_enabled(
    pool: &SqlitePool,
    pack: &dyn PackHandle,
) -> anyhow::Result<bool> {
    let key = enabled_meta_key(pack.slug());
    let row: Option<(String,)> = sqlx::query_as("SELECT value FROM meta WHERE key = ?1")
        .bind(&key)
        .fetch_optional(pool)
        .await?;
    Ok(match row {
        Some((v,)) => v == "true",
        None => pack.default_enabled(),
    })
}

/// Write a pack's runtime-enabled flag. Idempotent. Settings → Packs and
/// the onboarding pack picker call this.
pub async fn set_pack_enabled(
    pool: &SqlitePool,
    slug: &str,
    enabled: bool,
) -> anyhow::Result<()> {
    let key = enabled_meta_key(slug);
    let value = if enabled { "true" } else { "false" };
    sqlx::query(
        "INSERT INTO meta(key, value, updated_at)
         VALUES (?1, ?2, CURRENT_TIMESTAMP)
         ON CONFLICT(key) DO UPDATE SET
            value = excluded.value,
            updated_at = CURRENT_TIMESTAMP",
    )
    .bind(&key)
    .bind(value)
    .execute(pool)
    .await?;
    Ok(())
}

/// Resolve which compiled-in packs are enabled at runtime, reading
/// `meta.pack.<slug>.enabled` for each and falling back to the pack's
/// [`PackHandle::default_enabled`] when no flag exists.
///
/// Called once at app startup. The result populates
/// [`crate::AppState::enabled_packs`]; runtime changes via
/// [`set_pack_enabled`] take effect on next launch.
pub async fn resolve_enabled_packs(
    pool: &SqlitePool,
) -> anyhow::Result<Vec<&'static dyn PackHandle>> {
    let mut out: Vec<&'static dyn PackHandle> = Vec::new();
    for pack in compiled_in_packs() {
        if is_pack_enabled(pool, *pack).await? {
            out.push(*pack);
        }
    }
    Ok(out)
}

/// Run pending migrations for every compiled-in pack — regardless of
/// runtime-enable state. Idempotent; uses `meta.pack.<slug>.schema_version`
/// to skip anything already done.
///
/// Called once from `db::Db::open` after core migrations succeed.
pub async fn run_pack_migrations(pool: &SqlitePool) -> anyhow::Result<()> {
    for pack in compiled_in_packs() {
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

/// Concatenated system-prompt fragments from the supplied packs, separated
/// by blank lines. Empty string when no fragments. Pass
/// [`crate::AppState::enabled_packs`] (or any subset) to limit which
/// packs contribute.
pub fn prompt_fragment(packs: &[&dyn PackHandle]) -> String {
    let parts: Vec<&'static str> = packs
        .iter()
        .filter_map(|p| p.prompt_fragment())
        .collect();
    if parts.is_empty() {
        String::new()
    } else {
        parts.join("\n\n")
    }
}
