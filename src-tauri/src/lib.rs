pub mod commands;
pub mod db;
pub mod error;
pub mod integrations;
pub mod intelligence;
pub mod manifest;
pub mod protocols;
pub mod state;
pub mod storage;
pub mod supervisor;
pub mod types;

use state::AppState;
use std::sync::Arc;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let home = dirs::home_dir().expect("home dir").join(".homemade");
            storage::env_router::ensure_layout(&home)?;
            let conn = db::open(&home.join("state.db"))?;
            let login_env = supervisor::spawner::load_user_login_env();
            let state = Arc::new(AppState {
                db: std::sync::Mutex::new(conn),
                home: home.clone(),
                login_env,
                active_runs: dashmap::DashMap::new(),
                active_ptys: dashmap::DashMap::new(),
                vram_queue: tokio::sync::Mutex::new(std::collections::VecDeque::new()),
                resources: std::sync::Mutex::new(types::ResourceSnapshot::default()),
                app_handle: app.handle().clone(),
            });
            app.manage(state.clone());

            // Background: telemetry + scheduler tick, file watchers for registered apps, skill install (best effort).
            supervisor::telemetry::start(state.clone());
            {
                let rows = { let c = state.db.lock().unwrap(); db::queries::list_apps(&c).unwrap_or_default() };
                for r in rows { manifest::watcher::watch_app(state.clone(), r.id.clone(), std::path::PathBuf::from(r.root_dir)); }
            }
            let res_dir = commands::resources_dir(app.handle());
            let _ = integrations::skill::install(&res_dir, &home);

            // Close guard (spec §5C).
            if let Some(win) = app.get_webview_window("main") {
                let st = state.clone();
                let handle = win.clone();
                win.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        let n = st.active_count();
                        if n > 0 {
                            api.prevent_close();
                            use tauri::Emitter;
                            let _ = handle.emit("app://confirm-exit", n);
                        }
                    }
                });
            }
            Ok(())
        })
        .register_uri_scheme_protocol("homemade-asset", |ctx, req| protocols::asset::handle(ctx, req))
        .register_uri_scheme_protocol("homemade-app", |ctx, req| {
            let state = ctx.app_handle().state::<Arc<AppState>>().inner().clone();
            let res_dir = commands::resources_dir(ctx.app_handle());
            protocols::app_ui::handle(state, res_dir, req)
        })
        .invoke_handler(tauri::generate_handler![
            commands::apps::list_apps, commands::apps::import_app, commands::apps::get_app, commands::apps::reload_manifest, commands::apps::remove_app, commands::apps::save_manifest,
            commands::runs::start_run, commands::runs::stop_run, commands::runs::force_run, commands::runs::list_runs, commands::runs::get_run, commands::runs::get_run_log, commands::runs::rpc_call, commands::runs::build_cli_command, commands::runs::list_active_runs,
            commands::presets::list_presets, commands::presets::save_preset, commands::presets::delete_preset,
            commands::artifacts::list_artifacts, commands::artifacts::clean_scratch, commands::artifacts::delete_artifact,
            commands::storage::storage_summary, commands::storage::scan_venv, commands::storage::migrate_venv, commands::storage::list_secret_names, commands::storage::set_secret, commands::storage::delete_secret,
            commands::resources::resource_snapshot,
            commands::pty::pty_spawn, commands::pty::pty_write, commands::pty::pty_resize, commands::pty::pty_kill, commands::pty::list_ptys,
            commands::agents::detect_agents, commands::agents::list_agent_sessions, commands::agents::install_skill,
            commands::git::git_status, commands::git::git_init, commands::git::git_checkpoint, commands::git::git_revert_to,
            commands::system::active_count, commands::system::confirm_quit, commands::system::reveal_in_finder, commands::system::homemade_home, commands::system::read_text_file,
        ])
        .run(tauri::generate_context!())
        .expect("error while running Homemade");
}
