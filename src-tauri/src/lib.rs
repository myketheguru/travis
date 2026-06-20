// v0.19.7 — bump macro recursion limit for the agent-loop's
// `report_extraction` tool schema. The serde_json::json! literal at
// journal.rs:838 has grown past the default 128 with v0.19.x's pack
// memory / document classification / coach_hours / engagement
// enrichment / invoice draft fields. 512 leaves room for the next
// few extraction-field additions before another bump.
#![recursion_limit = "512"]

mod actions;
mod behavioral;
mod behavioral_cmd;
mod calendar;
mod calendar_cmd;
mod capture;
mod diagnostics;
mod cases;
mod cloud;
mod commands;
mod conversation;
mod conversation_cmd;
mod data_export;
mod data_export_cmd;
mod db;
mod documents;
mod domain;
mod interpreter;
mod steps;
mod identity_cmd_recall;
mod initiatives;
mod persona;
mod email;
mod email_cmd;
mod events;
mod feedback;
mod flags;
mod flags_cmd;
mod graph_indexer;
mod graph_inference;
mod health;
mod identity;
mod identity_cmd;
mod journal;
mod llm;
mod manager;
mod memory;
mod memory_cmd;
mod python_runtime;
mod overlay;
mod packs;
mod packs_cmd;
mod platform_cmd;
mod force_upgrade;
mod template_assets;
mod plans;
mod proactive;
mod reminders;
mod reminders_cmd;
mod secrets;
mod spine;
mod startup_error;
mod summary;
mod summary_cmd;
mod task_cmd;
mod templates;
mod telemetry;
mod tools;
mod updater_cmd;
mod workflows;
mod workspace_runtime;
mod workspaces;
mod workspaces_cmd;

// L2E pack command surface, conditionally compiled. References under
// `crate::packs::lead_to_empower::{domain_cmd, pdf_cmd}` get aliased
// here so the invoke_handler list below stays readable.
#[cfg(feature = "pack-lead-to-empower")]
use crate::packs::lead_to_empower::{domain_cmd, pdf_cmd};

use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub struct AppState {
    pub db: Arc<db::Db>,
    pub http: reqwest::Client,
    pub health: Arc<health::Health>,
    pub actions: Arc<actions::ActionRegistry>,
    /// Compiled-in packs that the user has enabled at runtime
    /// (`meta.pack.<slug>.enabled`). Resolved once at startup; toggling
    /// via [`packs::set_pack_enabled`] takes effect on next launch.
    pub enabled_packs: Vec<&'static dyn packs::PackHandle>,
    /// Active workspace + visible-workspace ids. Reads on every
    /// scoped Tauri command path; writes when the user switches
    /// active workspace or toggles a workspace's `cross_visible`
    /// flag. See WORKSPACES.md.
    pub workspace: Arc<tokio::sync::RwLock<workspaces::State>>,
    /// In-process working memory — per-conversation hypothesis store
    /// (BRAIN.md Phase 4.5 #6). 30-min TTL; lost on restart by design.
    pub working_memory: memory::working::WorkingMemory,
    /// v0.14.0 — Pyodide interpreter session manager. Holds the
    /// channel registry for in-flight `run_python` calls and tracks
    /// whether the hidden interpreter window is ready.
    pub interpreter: interpreter::InterpreterState,
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,sqlx=warn")),
        )
        .init();

    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .user_agent("Travis/0.1")
        .build()
        .expect("failed to build http client");

    #[cfg(target_os = "macos")]
    let primary_shortcut = Shortcut::new(Some(Modifiers::META), Code::KeyJ);
    #[cfg(not(target_os = "macos"))]
    let primary_shortcut = Shortcut::new(Some(Modifiers::CONTROL), Code::KeyJ);

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(move |app, shortcut, event| {
                    if event.state() == ShortcutState::Pressed
                        && shortcut == &primary_shortcut
                    {
                        let _ = overlay::toggle(app);
                    }
                })
                .build(),
        )
        .on_window_event(|window, event| {
            // Overlay: hide on blur (user clicked elsewhere)
            if window.label() == "overlay" {
                if let WindowEvent::Focused(false) = event {
                    let _ = window.hide();
                }
                return;
            }
            // Main: clicking the X hides instead of quits — Travis keeps
            // running in the tray so reminders/telemetry/future proactive
            // nudges stay alive. Quit only via tray menu.
            if window.label() == "main" {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    let _ = window.hide();
                    api.prevent_close();
                }
            }
        })
        .setup(move |app| {
            let handle = app.handle().clone();
            // Resolve the app data dir up front. We need this for both the
            // database and any startup_error log we want to write.
            let data_dir = match handle.path().app_data_dir() {
                Ok(d) => d,
                Err(e) => {
                    startup_error::die(
                        None,
                        format!(
                            "Travis couldn't resolve its app data directory.\n\nDetails: {e}\n\n\
                             This is unusual — try reinstalling, or contact support."
                        ),
                    );
                }
            };
            std::fs::create_dir_all(&data_dir).ok();
            let db_path = data_dir.join("travis.db");

            // Open + migrate the SQLite DB. The most likely failure mode is
            // a sqlx migration checksum mismatch from a stale dev DB — the
            // dialog points the user at the fix.
            let db = tauri::async_runtime::block_on(async {
                db::Db::open(&db_path).await
            });
            let db = match db {
                Ok(db) => db,
                Err(e) => {
                    startup_error::die(
                        Some(&data_dir),
                        format!(
                            "Travis couldn't open its database.\n\n\
                             Details: {e}\n\n\
                             Database location:\n{}\n\n\
                             If you've run a development build of Travis on this \
                             machine before, the data directory may have stale \
                             migration checksums. Closing this dialog and deleting \
                             the directory above (or running:\n  \
                             rmdir /s /q \"{}\"\n in cmd) will let Travis start fresh.",
                            db_path.display(),
                            data_dir.display()
                        ),
                    );
                }
            };

            let db_arc = Arc::new(db);
            let health_arc = Arc::new(health::Health::new());

            // v0.20.2 — one-time migration to Travis Cloud as default
            // LLM provider. No-op when the build doesn't ship a cloud
            // key OR the user has already been migrated.
            let db_for_migrate = db_arc.clone();
            tauri::async_runtime::block_on(async move {
                if let Err(e) = db_for_migrate.migrate_to_travis_cloud_if_needed().await {
                    tracing::warn!("travis cloud migration failed (non-fatal): {e}");
                }
            });

            // Resolve which compiled-in packs the user has enabled at
            // runtime via `meta.pack.<slug>.enabled` (PACKS.md "two
            // layers of pack gating"). First-encounter packs fall back
            // to PackHandle::default_enabled.
            let enabled_packs = match tauri::async_runtime::block_on(
                packs::resolve_enabled_packs(&db_arc.pool),
            ) {
                Ok(list) => list,
                Err(e) => {
                    startup_error::die(
                        Some(&data_dir),
                        format!(
                            "Travis couldn't resolve which packs are enabled.\n\n\
                             Details: {e}"
                        ),
                    );
                }
            };

            // Build the action registry: core handlers first, then let
            // each enabled pack add its own. Disabled packs don't
            // register, so the LLM doesn't see their action kinds.
            let mut action_registry = actions::builtin_registry();
            for pack in &enabled_packs {
                pack.register_actions(&mut action_registry);
            }
            let actions_arc = Arc::new(action_registry);

            // Load workspace state — active id from
            // `meta.active_workspace_id` (default 1 for fresh DBs and
            // upgrades) and visible-id set per the asymmetric
            // isolation rule (WORKSPACES.md).
            let workspace_state = match tauri::async_runtime::block_on(
                workspaces::State::load(&db_arc.pool),
            ) {
                Ok(s) => s,
                Err(e) => {
                    startup_error::die(
                        Some(&data_dir),
                        format!(
                            "Travis couldn't resolve the active workspace.\n\n\
                             Details: {e}"
                        ),
                    );
                }
            };
            let workspace_arc = Arc::new(tokio::sync::RwLock::new(workspace_state));

            let interpreter_state = interpreter::InterpreterState::new();
            handle.manage(AppState {
                db: db_arc.clone(),
                http: http.clone(),
                health: health_arc.clone(),
                actions: actions_arc,
                enabled_packs,
                workspace: workspace_arc,
                working_memory: memory::working::WorkingMemory::new(),
                interpreter: interpreter_state.clone(),
            });

            // v0.18.0 — interpreter wiring removed. Pyodide hidden
            // window is gone; python execution is now subprocess-
            // based via `python_runtime`. No readiness signal needed
            // (process spawn is atomic) and no event routing needed
            // (subprocess stdout/stderr are read inline).
            let _ = &interpreter_state; // kept on AppState for now

            handle.global_shortcut().register(primary_shortcut)?;

            // System tray: keeps Travis alive when the main window is closed.
            // Left-click toggles the main window; menu has explicit Open/Quit.
            let tray_menu = Menu::with_items(
                &handle,
                &[
                    &MenuItem::with_id(&handle, "tray_open", "Open Travis", true, None::<&str>)?,
                    &MenuItem::with_id(&handle, "tray_overlay", "Open quick capture (Ctrl+J)", true, None::<&str>)?,
                    &MenuItem::with_id(&handle, "tray_quit", "Quit Travis", true, None::<&str>)?,
                ],
            )?;
            let tray_icon = match handle.default_window_icon().cloned() {
                Some(i) => i,
                None => {
                    startup_error::die(
                        Some(&data_dir),
                        "Travis couldn't load its window icon. The build is corrupted — try \
                         reinstalling.",
                    );
                }
            };
            let _tray = TrayIconBuilder::with_id("travis-tray")
                .icon(tray_icon)
                .tooltip("Travis")
                .menu(&tray_menu)
                .menu_on_left_click(false)
                .on_menu_event(move |app, event| {
                    match event.id.as_ref() {
                        "tray_open" => {
                            if let Some(w) = app.get_webview_window("main") {
                                let _ = w.show();
                                let _ = w.unminimize();
                                let _ = w.set_focus();
                            }
                        }
                        "tray_overlay" => {
                            let _ = overlay::toggle(app);
                        }
                        "tray_quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(w) = app.get_webview_window("main") {
                            // Toggle: if visible+focused, hide. Else show+focus.
                            if w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false) {
                                let _ = w.hide();
                            } else {
                                let _ = w.show();
                                let _ = w.unminimize();
                                let _ = w.set_focus();
                            }
                        }
                    }
                })
                .build(&handle)?;

            reminders::scheduler::spawn(handle.clone(), db_arc.clone());
            telemetry::sender::spawn(handle.clone(), db_arc.clone(), http.clone());
            proactive::spawn(handle.clone(), db_arc.clone(), http.clone(), health_arc.clone());
            graph_indexer::spawn(db_arc.clone());

            // User model derivation (BRAIN.md capability #3a). Daily
            // background pass that summarises capture patterns into
            // user_profile.derived_model_json. Persona block consumes
            // it so Travis adapts timing + length without being told.
            {
                let pool = db_arc.pool.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(120)).await;
                    loop {
                        match crate::persona::user_model::refresh(&pool).await {
                            Ok(Some(m)) => tracing::info!(
                                "user model: refreshed ({} captures, peak {})",
                                m.capture_count,
                                m.peak_window
                            ),
                            Ok(None) => {}
                            Err(e) => tracing::warn!("user model refresh failed: {e}"),
                        }
                        // Daily. Active hours / cadence are stable
                        // signals; no need for higher cadence.
                        tokio::time::sleep(std::time::Duration::from_secs(24 * 60 * 60)).await;
                    }
                });
            }

            // Entity personality slots (BRAIN.md capability #3b).
            // Weekly background pass that for each frequently-mentioned
            // person entity derives contact-window + style hints from
            // mention timing and snippets. Writes into
            // entity.attributes_json under a "personality" key.
            {
                let pool = db_arc.pool.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_secs(180)).await;
                    loop {
                        let n = crate::persona::entity_model::run_tick(&pool).await;
                        if n > 0 {
                            tracing::info!("entity personality: updated {n} entit{}",
                                if n == 1 { "y" } else { "ies" });
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(6 * 60 * 60)).await;
                    }
                });
            }

            // Memory consolidation tick (BRAIN.md Phase 4.5 #3).
            // Periodic pass that turns each entity's event cloud into
            // a stable summary claim — keeps retrieval from getting
            // noisier as usage accumulates. Fires every 30 minutes;
            // each tick processes at most 25 stale entities.
            {
                let pool = db_arc.pool.clone();
                tauri::async_runtime::spawn(async move {
                    // Wait 60s on startup so we don't compete with
                    // first-boot migrations or extraction warmup.
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    loop {
                        let n = crate::memory::consolidate::run_tick(&pool).await;
                        if n > 0 {
                            tracing::info!("memory consolidate: refreshed {n} entit{}",
                                if n == 1 { "y" } else { "ies" });
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(30 * 60)).await;
                    }
                });
            }

            // Auto-close idle conversations: fire once at startup, then
            // daily. Cheap UPDATE; no side effects beyond status change.
            {
                let pool = db_arc.pool.clone();
                tauri::async_runtime::spawn(async move {
                    // One pass right away — covers the case where the
                    // app was closed during the would-be tick.
                    match crate::conversation::auto_close_idle(&pool).await {
                        Ok(n) if n > 0 => tracing::info!("auto-closed {n} idle conversations"),
                        Ok(_) => {}
                        Err(e) => tracing::warn!("auto-close idle conversations: {e}"),
                    }
                    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(86_400));
                    ticker.tick().await; // skip the immediate first tick
                    loop {
                        ticker.tick().await;
                        match crate::conversation::auto_close_idle(&pool).await {
                            Ok(n) if n > 0 => tracing::info!("auto-closed {n} idle conversations"),
                            Ok(_) => {}
                            Err(e) => tracing::warn!("auto-close idle conversations: {e}"),
                        }
                    }
                });
            }

            // v0.16.3 — memory decay tick. Exponentially decays unpinned
            // claim relevance with a 180-day half-life. Archives claims
            // below the relevance floor (0.05). Runs daily; one pass at
            // startup to cover gaps from when the app was closed.
            {
                let pool = db_arc.pool.clone();
                tauri::async_runtime::spawn(async move {
                    const HALF_LIFE_DAYS: f64 = 180.0;
                    const ARCHIVE_FLOOR: f64 = 0.05;
                    // Delay the first pass so it doesn't race startup.
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(86_400));
                    // Immediate first tick fires; that's our startup pass.
                    loop {
                        ticker.tick().await;
                        match crate::memory::claims::decay_all(&pool, HALF_LIFE_DAYS).await {
                            Ok(n) => {
                                if n > 0 {
                                    tracing::info!("memory decay: updated {n} claim(s)");
                                }
                            }
                            Err(e) => tracing::warn!("memory decay failed: {e}"),
                        }
                        match crate::memory::claims::archive_low_relevance(&pool, ARCHIVE_FLOOR)
                            .await
                        {
                            Ok(n) => {
                                if n > 0 {
                                    tracing::info!(
                                        "memory decay: archived {n} claim(s) below {}",
                                        ARCHIVE_FLOOR
                                    );
                                }
                            }
                            Err(e) => tracing::warn!("memory archive failed: {e}"),
                        }
                    }
                });
            }

            let pool = db_arc.pool.clone();
            let http_clone = http.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = flags::refresh(&pool, &http_clone).await {
                    tracing::warn!("startup flags refresh failed: {e}");
                }
            });

            // Step cleanup: any step that was 'running' at app exit
            // (crashed or force-killed mid-execution) gets marked
            // 'cancelled' so the chat UI doesn't show eternal spinners.
            // Runs once at startup, non-blocking.
            {
                let pool = db_arc.pool.clone();
                tauri::async_runtime::spawn(async move {
                    match crate::steps::cmd::mark_orphans_cancelled(&pool).await {
                        Ok(n) if n > 0 => {
                            tracing::info!("step cleanup: marked {n} orphans as cancelled")
                        }
                        Ok(_) => {}
                        Err(e) => tracing::warn!("step cleanup failed: {e}"),
                    }
                });
            }

            // Auto-update poll. Tauri's updater plugin doesn't poll by
            // default — without this loop the only path that ever
            // checks the endpoint is Settings → "Check for updates".
            // First check fires ~60s after startup (give the app room
            // to settle), then every 4 hours. When a new version is
            // detected we emit `update-available` for the frontend AND
            // fire a system notification (once per version per session
            // so we don't spam between polls).
            {
                let handle = handle.clone();
                tauri::async_runtime::spawn(async move {
                    use tauri::Emitter;
                    use tauri_plugin_notification::NotificationExt;
                    let mut last_notified: Option<String> = None;
                    tokio::time::sleep(std::time::Duration::from_secs(60)).await;
                    loop {
                        match crate::updater_cmd::silent_check(&handle).await {
                            Ok(Some(info)) => {
                                tracing::info!(
                                    "update available: {} (current {})",
                                    info.version,
                                    env!("CARGO_PKG_VERSION"),
                                );
                                let _ = handle.emit("update-available", &info);
                                // Native notification, once per session per version.
                                if last_notified.as_deref() != Some(info.version.as_str()) {
                                    let _ = handle
                                        .notification()
                                        .builder()
                                        .title("Travis update available")
                                        .body(format!(
                                            "Version {} is ready. Open Settings to install.",
                                            info.version
                                        ))
                                        .show();
                                    last_notified = Some(info.version.clone());
                                }
                            }
                            Ok(None) => {}
                            Err(e) => tracing::warn!("auto-update check failed: {e}"),
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(4 * 60 * 60)).await;
                    }
                });
            }

            // Emit app_start telemetry — metadata only.
            let pool2 = db_arc.pool.clone();
            tauri::async_runtime::spawn(async move {
                telemetry::emit(
                    &pool2,
                    "app_start",
                    serde_json::json!({
                        "version": env!("CARGO_PKG_VERSION"),
                        "platform": std::env::consts::OS,
                    }),
                )
                .await;
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            cloud::cmd::cloud_status,
            cloud::cmd::cloud_sign_in_with_google,
            cloud::cmd::cloud_sign_out,
            cloud::cmd::cloud_policy,
            cloud::cmd::cloud_record_byok,
            cloud::cmd::cloud_has_token,
            cloud::cmd::cloud_migration_status,
            cloud::cmd::cloud_migration_upload,
            cloud::cmd::cloud_migration_start_fresh,
            cloud::cmd::cloud_migration_skip,
            commands::app_status,
            commands::complete_onboarding,
            commands::update_profile,
            commands::get_user_profile,
            commands::has_api_key,
            commands::set_api_key,
            commands::get_shell_enabled,
            commands::set_shell_enabled,
            commands::get_proactive_config,
            commands::set_proactive_enabled,
            commands::set_proactive_schedule,
            commands::test_provider,
            commands::chat,
            overlay::toggle_overlay,
            overlay::hide_overlay,
            #[cfg(feature = "pack-lead-to-empower")] domain_cmd::db_stats,
            #[cfg(feature = "pack-lead-to-empower")] domain_cmd::list_coaches,
            #[cfg(feature = "pack-lead-to-empower")] domain_cmd::upsert_coach,
            #[cfg(feature = "pack-lead-to-empower")] domain_cmd::delete_coach,
            #[cfg(feature = "pack-lead-to-empower")] domain_cmd::list_schools,
            #[cfg(feature = "pack-lead-to-empower")] domain_cmd::upsert_school,
            #[cfg(feature = "pack-lead-to-empower")] domain_cmd::delete_school,
            #[cfg(feature = "pack-lead-to-empower")] domain_cmd::list_coach_hours,
            #[cfg(feature = "pack-lead-to-empower")] domain_cmd::log_coach_hours,
            #[cfg(feature = "pack-lead-to-empower")] domain_cmd::delete_coach_hours,
            #[cfg(feature = "pack-lead-to-empower")] domain_cmd::list_signing_sheets,
            #[cfg(feature = "pack-lead-to-empower")] domain_cmd::upsert_signing_sheet,
            #[cfg(feature = "pack-lead-to-empower")] domain_cmd::delete_signing_sheet,
            #[cfg(feature = "pack-lead-to-empower")] domain_cmd::list_invoices,
            #[cfg(feature = "pack-lead-to-empower")] domain_cmd::upsert_invoice,
            #[cfg(feature = "pack-lead-to-empower")] domain_cmd::transition_invoice,
            #[cfg(feature = "pack-lead-to-empower")] domain_cmd::delete_invoice,
            #[cfg(feature = "pack-lead-to-empower")] crate::packs::lead_to_empower::detail_cmd::lte_school_detail,
            #[cfg(feature = "pack-lead-to-empower")] crate::packs::lead_to_empower::detail_cmd::lte_engagement_detail,
            #[cfg(feature = "pack-lead-to-empower")] crate::packs::lead_to_empower::detail_cmd::lte_coach_detail,
            task_cmd::list_tasks,
            task_cmd::upsert_task,
            task_cmd::set_task_status,
            task_cmd::delete_task,
            journal::journal_ingest,
            journal::list_journal_entries,
            memory_cmd::index_all_journal_entries,
            memory_cmd::ask_travis,
            reminders_cmd::list_reminders,
            reminders_cmd::create_reminder,
            reminders_cmd::dismiss_reminder,
            reminders_cmd::delete_reminder,
            behavioral_cmd::list_events,
            behavioral_cmd::list_patterns,
            behavioral_cmd::detect_patterns,
            behavioral_cmd::dismiss_pattern,
            summary_cmd::list_summaries,
            summary_cmd::generate_daily_summary,
            summary_cmd::generate_weekly_summary,
            data_export_cmd::export_data,
            data_export_cmd::reveal_export,
            documents::cmd::ingest_document,
            documents::cmd::list_documents,
            documents::cmd::get_document,
            documents::cmd::get_document_path,
            documents::cmd::link_document,
            documents::cmd::set_document_kind,
            documents::cmd::delete_document,
            documents::cmd::extract_document,
            documents::cmd::update_document_extraction,
            documents::cmd::preview_document,
            documents::cmd::reveal_document_in_folder,
            documents::cmd::download_document,
            documents::cmd::analyze_document_styling,
            cases::cmd::list_open_cases,
            cases::cmd::open_case,
            cases::cmd::close_case,
            cases::cmd::list_case_artifacts,
            cases::cmd::case_for_conversation,
            templates::cmd::save_pack_template,
            templates::cmd::list_pack_templates,
            templates::cmd::delete_pack_template,
            interpreter::cmd::run_python,
            steps::cmd::list_steps,
            workflows::cmd::get_active_workflow,
            identity_cmd::list_entities,
            identity_cmd_recall::recall_entity,
            identity_cmd::get_profile_blurb,
            #[cfg(feature = "pack-lead-to-empower")] pdf_cmd::export_invoice_pdf,
            #[cfg(feature = "pack-lead-to-empower")] pdf_cmd::export_invoice_pdf_preview,
            #[cfg(feature = "pack-lead-to-empower")] pdf_cmd::export_work_order_pdf,
            #[cfg(feature = "pack-lead-to-empower")] pdf_cmd::export_sign_in_sheet_pdf,
            email_cmd::get_smtp_config,
            email_cmd::set_smtp_config,
            email_cmd::list_emails_sent,
            #[cfg(feature = "pack-lead-to-empower")] email_cmd::send_invoice_email,
            email_cmd::send_email_gmail,
            email_cmd::send_email_outlook,
            flags_cmd::refresh_flags,
            flags_cmd::get_flags,
            flags_cmd::get_flag,
            flags_cmd::set_flags_url,
            platform_cmd::platform_info,
            force_upgrade::check_force_upgrade,
            force_upgrade::quit_app,
            template_assets::list_template_assets,
            template_assets::find_template_assets,
            template_assets::request_template_extraction,
            template_assets::set_template_asset_label,
            plans::plan_create_cmd,
            plans::plan_status_cmd,
            plans::plan_active_cmd,
            packs_cmd::list_packs,
            packs_cmd::set_pack_enabled,
            packs_cmd::pack_schemas,
            packs_cmd::pack_table_list,
            packs_cmd::pack_table_get,
            packs_cmd::pack_table_upsert,
            packs_cmd::pack_table_delete,
            packs_cmd::pack_alerts,
            packs_cmd::pack_valves,
            packs_cmd::set_pack_valve,
            packs_cmd::reset_pack_valve,
            workspaces_cmd::list_workspaces,
            workspaces_cmd::get_active_workspace,
            workspaces_cmd::set_active_workspace,
            workspaces_cmd::create_workspace,
            workspaces_cmd::update_workspace,
            workspaces_cmd::archive_workspace,
            workspaces_cmd::unarchive_workspace,
            updater_cmd::check_for_update,
            updater_cmd::install_update,
            health::health_status,
            health::health_set_online,
            health::health_dismiss,
            feedback::list_feedback,
            feedback::ack_feedback,
            feedback::delete_feedback,
            conversation_cmd::list_conversations,
            conversation_cmd::list_conversations_for_switcher,
            conversation_cmd::get_thread,
            conversation_cmd::active_conversation,
            conversation_cmd::load_more_messages,
            conversation_cmd::resolve_conversation,
            conversation_cmd::append_user_message,
            diagnostics::list_recent_errors,
            diagnostics::clear_error_log,
            conversation_cmd::delete_message_and_after,
            actions::list_proposed_actions,
            actions::confirm_action,
            actions::decline_action,
            calendar_cmd::calendar_status,
            calendar_cmd::calendar_connect_google,
            calendar_cmd::calendar_disconnect_google,
            calendar_cmd::microsoft_status,
            calendar_cmd::microsoft_connect,
            calendar_cmd::microsoft_disconnect,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
