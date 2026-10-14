pub mod commands;
pub mod control;
pub mod db;
pub mod error;
pub mod integrations;
pub mod intelligence;
#[cfg(target_os = "macos")]
pub mod macos_quit;
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
        .plugin(tauri_plugin_drag::init())
        .setup(|app| {
            let home = storage::config::homemade_home();
            storage::env_router::ensure_layout(&home)?;
            let conn = db::open(&home.join("state.db"))?;
            let login_env = supervisor::spawner::load_user_login_env();
            let res_dir = commands::resources_dir(app.handle());
            let model = Arc::new(intelligence::local_model::LocalModel::new(&res_dir, &storage::config::load(&home)));
            let assistor = intelligence::assistor::make(model.clone());
            let state = Arc::new(AppState {
                db: std::sync::Mutex::new(conn),
                home: home.clone(),
                login_env,
                active_runs: dashmap::DashMap::new(),
                active_ptys: dashmap::DashMap::new(),
                vram_queue: tokio::sync::Mutex::new(std::collections::VecDeque::new()),
                resources: std::sync::Mutex::new(types::ResourceSnapshot::default()),
                usage_history: std::sync::Mutex::new(std::collections::VecDeque::with_capacity(300)),
                app_handle: app.handle().clone(),
                model,
                assistor,
            });
            app.manage(state.clone());
            control::start(state.clone());
            #[cfg(target_os = "macos")]
            macos_quit::install(state.clone());

            // Reap anything a previous session left running (e.g. after a crash or a forced quit), then tell the UI.
            {
                let st = state.clone();
                tauri::async_runtime::spawn(async move {
                    let (reaped, recorded) = supervisor::reaper::reap_orphans(&st).await;
                    if recorded > 0 {
                        tokio::time::sleep(std::time::Duration::from_millis(1500)).await; // let the webview subscribe
                        let msg = if reaped > 0 {
                            format!("Stopped {reaped} program{} left running by the previous session", if reaped == 1 { "" } else { "s" })
                        } else {
                            format!("Marked {recorded} run{} from the previous session as killed", if recorded == 1 { "" } else { "s" })
                        };
                        st.emit("app://notice", serde_json::json!({ "message": msg }));
                    }
                });
            }
            // Background: telemetry + scheduler tick, file watchers for registered apps, skill install (best effort).
            supervisor::telemetry::start(state.clone());
            {
                let rows = { let c = state.db.lock().unwrap(); db::queries::list_apps(&c).unwrap_or_default() };
                for r in rows { manifest::watcher::watch_app(state.clone(), r.id.clone(), std::path::PathBuf::from(r.root_dir)); }
            }
            // Local model availability, once, in the background (model://status).
            {
                let st = state.clone();
                tauri::async_runtime::spawn(async move {
                    let s = st.model.status().await;
                    tokio::time::sleep(std::time::Duration::from_millis(1500)).await; // let the webview subscribe
                    st.emit("model://status", serde_json::json!({ "available": s.available, "reason": s.reason }));
                });
            }
            let _ = integrations::skill::install(&res_dir, &home);

            // Headless smoke hook: HOMEMADE_SMOKE=<app_id> [HOMEMADE_SMOKE_INPUTS='{"name":"x"}'] runs one job and exits.
            if let Ok(app_id) = std::env::var("HOMEMADE_SMOKE") {
                let st = state.clone();
                let res_dir = res_dir.clone();
                let inputs: serde_json::Value = std::env::var("HOMEMADE_SMOKE_INPUTS").ok()
                    .and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(serde_json::json!({}));
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    eprintln!("[smoke] starting {app_id} with {inputs}");
                    let rec = match commands::runs::start_run_inner(st.clone(), res_dir, app_id.clone(), inputs).await {
                        Ok(r) => r,
                        Err(e) => { eprintln!("[smoke] start failed: {e}"); st.app_handle.exit(2); return; }
                    };
                    eprintln!("[smoke] run {} cmd: {}", rec.id, rec.cli_command);
                    for _ in 0..120 {
                        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        let r = { let c = st.db.lock().unwrap(); db::queries::get_run(&c, &rec.id).ok().flatten() };
                        if let Some(r) = r {
                            if matches!(r.status.as_str(), "success" | "failed" | "killed") {
                                let arts = { let c = st.db.lock().unwrap(); db::queries::list_artifacts(&c, &app_id, Some(&rec.id), None).unwrap_or_default() };
                                eprintln!("[smoke] status={} exit={:?} peak_ram_mb={}\n[smoke] log tail:\n{}\n[smoke] artifacts: {:?}",
                                    r.status, r.exit_code, r.peak_ram_mb, r.log_tail.unwrap_or_default(),
                                    arts.iter().map(|a| format!("{} ({}, {} bytes)", a.file_path, a.kind, a.file_size_bytes)).collect::<Vec<_>>());
                                st.app_handle.exit(if r.status == "success" { 0 } else { 1 });
                                return;
                            }
                        }
                    }
                    eprintln!("[smoke] timeout"); st.app_handle.exit(3);
                });
            }

            // E2E harness: HOMEMADE_E2E=<script.js> is evaluated inside the main webview once window.__hm exists.
            if let Ok(script_path) = std::env::var("HOMEMADE_E2E") {
                let handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let js = match std::fs::read_to_string(&script_path) {
                        Ok(s) => s,
                        Err(e) => { eprintln!("[e2e] cannot read script: {e}"); handle.exit(8); return; }
                    };
                    let wrapped = format!(
                        "if (window.__hm && !window.__e2e_started) {{ window.__e2e_started = true; (async () => {{ try {{ {js} }} catch (e) {{ await window.__hm.report('SCRIPT ERROR: ' + (e && e.stack || e)); await window.__hm.done(9); }} }})(); }}"
                    );
                    for i in 0..120 {
                        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                        if let Some(w) = handle.get_webview_window("main") {
                            if let Err(e) = w.eval(&wrapped) { eprintln!("[e2e] eval failed ({i}): {e}"); }
                        }
                    }
                });
            }

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
            commands::storage::storage_summary, commands::storage::storage_refs, commands::system::app_process_info, commands::system::app_activity, commands::storage::scan_venv, commands::storage::migrate_venv, commands::storage::list_secret_names, commands::storage::set_secret, commands::storage::delete_secret,
            commands::resources::resource_snapshot,
            commands::pty::pty_spawn, commands::pty::pty_write, commands::pty::pty_resize, commands::pty::pty_kill, commands::pty::list_ptys,
            commands::agents::detect_agents, commands::agents::list_agent_sessions, commands::agents::install_skill,
            commands::git::git_status, commands::git::git_init, commands::git::git_checkpoint, commands::git::git_revert_to,
            commands::system::active_count, commands::system::confirm_quit, commands::system::list_listening_ports, commands::system::kill_port_owner, commands::system::reveal_in_finder, commands::system::homemade_home, commands::system::read_text_file, commands::system::e2e_report, commands::system::e2e_done,
            commands::model::model_status, commands::model::draft_manifest, commands::model::apply_manifest, commands::model::explain_run, commands::model::model_complete,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Homemade")
        .run(|app, event| match event {
            // Cmd+Q / Quit menu / last window closed: same guard as the window close button.
            // `code` is Some only for programmatic `app.exit(code)` (confirm_quit), which must pass.
            tauri::RunEvent::ExitRequested { api, code, .. } if code.is_none() => {
                let st = app.state::<Arc<AppState>>();
                let n = st.active_count();
                if n > 0 {
                    api.prevent_exit();
                    use tauri::Emitter;
                    let _ = app.emit("app://confirm-exit", n);
                    if let Some(w) = app.get_webview_window("main") { let _ = w.show(); let _ = w.set_focus(); }
                }
            }
            // Safety net: whatever is still alive at the very end gets SIGKILLed so no orphan keeps a port.
            tauri::RunEvent::Exit => {
                let st = app.state::<Arc<AppState>>();
                supervisor::reaper::kill_all_now(&st);
                let ptys: Vec<String> = st.active_ptys.iter().map(|e| e.key().clone()).collect();
                for id in ptys { let _ = supervisor::pty_manager::kill(&st, &id); }
            }
            _ => {}
        });
}
