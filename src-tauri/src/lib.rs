mod actions;
mod behavioral;
mod behavioral_cmd;
mod calendar;
mod calendar_cmd;
mod commands;
mod conversation;
mod conversation_cmd;
mod db;
mod domain;
mod domain_cmd;
mod email;
mod email_cmd;
mod feedback;
mod flags;
mod flags_cmd;
mod health;
mod identity;
mod identity_cmd;
mod journal;
mod llm;
mod memory;
mod memory_cmd;
mod overlay;
mod pdf;
mod pdf_cmd;
mod proactive;
mod reminders;
mod reminders_cmd;
mod secrets;
mod startup_error;
mod summary;
mod summary_cmd;
mod telemetry;
mod tools;
mod updater_cmd;

use std::sync::Arc;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

pub struct AppState {
    pub db: Arc<db::Db>,
    pub http: reqwest::Client,
    pub health: Arc<health::Health>,
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
            handle.manage(AppState {
                db: db_arc.clone(),
                http: http.clone(),
                health: health_arc.clone(),
            });

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

            let pool = db_arc.pool.clone();
            let http_clone = http.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = flags::refresh(&pool, &http_clone).await {
                    tracing::warn!("startup flags refresh failed: {e}");
                }
            });

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
            commands::test_provider,
            commands::chat,
            overlay::toggle_overlay,
            overlay::hide_overlay,
            domain_cmd::db_stats,
            domain_cmd::list_coaches,
            domain_cmd::upsert_coach,
            domain_cmd::delete_coach,
            domain_cmd::list_schools,
            domain_cmd::upsert_school,
            domain_cmd::delete_school,
            domain_cmd::list_coach_hours,
            domain_cmd::log_coach_hours,
            domain_cmd::delete_coach_hours,
            domain_cmd::list_signing_sheets,
            domain_cmd::upsert_signing_sheet,
            domain_cmd::delete_signing_sheet,
            domain_cmd::list_invoices,
            domain_cmd::upsert_invoice,
            domain_cmd::transition_invoice,
            domain_cmd::delete_invoice,
            domain_cmd::list_tasks,
            domain_cmd::upsert_task,
            domain_cmd::set_task_status,
            domain_cmd::delete_task,
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
            identity_cmd::list_entities,
            identity_cmd::get_profile_blurb,
            pdf_cmd::export_invoice_pdf,
            pdf_cmd::export_invoice_pdf_preview,
            email_cmd::get_smtp_config,
            email_cmd::set_smtp_config,
            email_cmd::list_emails_sent,
            email_cmd::send_invoice_email,
            email_cmd::send_email_gmail,
            email_cmd::send_email_outlook,
            flags_cmd::refresh_flags,
            flags_cmd::get_flags,
            flags_cmd::get_flag,
            flags_cmd::set_flags_url,
            updater_cmd::check_for_update,
            updater_cmd::install_update,
            health::health_status,
            health::health_set_online,
            health::health_dismiss,
            feedback::list_feedback,
            feedback::ack_feedback,
            feedback::delete_feedback,
            conversation_cmd::list_conversations,
            conversation_cmd::get_thread,
            conversation_cmd::active_conversation,
            conversation_cmd::resolve_conversation,
            conversation_cmd::append_user_message,
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
