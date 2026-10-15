//! Control socket: the running GUI exposes every launcher operation over a local Unix socket so the CLI
//! (`homemade <cmd>`) and the MCP server (`homemade --mcp`) are thin clients of the same supervisor the UI uses.
//!
//! Protocol: newline-delimited JSON. Request `{"id":1,"token":"…","method":"start_run","params":{…}}`,
//! response `{"id":1,"result":…}` or `{"id":1,"error":"…"}`.
//! Socket: `<tmp>/homemade-<hash of home>.sock`; `<home>/control.json` = `{token, socket, pid}` (mode 0600, rewritten at every GUI
//! start) so only this user can drive the launcher.
pub mod client;
pub mod cli;

use crate::state::AppState;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;

/// Unix socket paths are limited to ~104 bytes, so the socket lives in the temp dir under a short hash of the home path;
/// `control.json` in the home records where it is.
pub fn socket_path(home: &Path) -> PathBuf {
    use sha2::Digest;
    let h = hex::encode(sha2::Sha256::digest(home.to_string_lossy().as_bytes()));
    std::env::temp_dir().join(format!("homemade-{}.sock", &h[..10]))
}
pub fn token_path(home: &Path) -> PathBuf { home.join("control.json") }

/// Start serving. Called once from Tauri setup.
pub fn start(state: Arc<AppState>) {
    let home = state.home.clone();
    let token = uuid::Uuid::new_v4().to_string();
    let sock = socket_path(&home);
    let info = serde_json::json!({ "token": token, "socket": sock, "pid": std::process::id() }).to_string();
    if let Err(e) = write_token(&token_path(&home), &info) { eprintln!("[control] token: {e}"); return; }
    let _ = std::fs::remove_file(&sock);
    install_cli_shim(&home);
    tauri::async_runtime::spawn(async move {
        // bind() needs the Tokio reactor, so it happens inside the runtime.
        let listener = match UnixListener::bind(&sock) { Ok(l) => l, Err(e) => { eprintln!("[control] bind {}: {e}", sock.display()); return; } };
        #[cfg(unix)]
        { use std::os::unix::fs::PermissionsExt; let _ = std::fs::set_permissions(&sock, std::fs::Permissions::from_mode(0o600)); }
        loop {
            let Ok((stream, _)) = listener.accept().await else { break };
            let st = state.clone();
            let tok = token.clone();
            tauri::async_runtime::spawn(async move {
                let (r, mut w) = stream.into_split();
                let mut lines = BufReader::new(r).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    if line.trim().is_empty() { continue; }
                    let req: Value = match serde_json::from_str(&line) { Ok(v) => v, Err(e) => { let _ = w.write_all(format!("{}\n", json!({"id": null, "error": format!("parse error: {e}")})).as_bytes()).await; continue; } };
                    let id = req.get("id").cloned().unwrap_or(Value::Null);
                    let resp = if req.get("token").and_then(|t| t.as_str()) != Some(tok.as_str()) {
                        json!({ "id": id, "error": "bad token" })
                    } else {
                        let method = req.get("method").and_then(|m| m.as_str()).unwrap_or("").to_string();
                        let params = req.get("params").cloned().unwrap_or(json!({}));
                        match dispatch(&st, &method, &params).await {
                            Ok(v) => json!({ "id": id, "result": v }),
                            Err(e) => json!({ "id": id, "error": e }),
                        }
                    };
                    if w.write_all(format!("{resp}\n").as_bytes()).await.is_err() { break; }
                }
            });
        }
    });
}

fn write_token(p: &Path, token: &str) -> std::io::Result<()> {
    std::fs::write(p, token)?;
    #[cfg(unix)]
    { use std::os::unix::fs::PermissionsExt; std::fs::set_permissions(p, std::fs::Permissions::from_mode(0o600))?; }
    Ok(())
}

/// `<home>/bin/homemade`: a shim that execs this binary, so `export PATH="$HOME/.homemade/bin:$PATH"` gives a `homemade`
/// command — and every process Homemade spawns gets that dir on PATH (see `child_env`). Also `<home>/mcp.json`, a ready-made
/// MCP server config (`claude --mcp-config <home>/mcp.json`).
fn install_cli_shim(home: &Path) {
    let Ok(exe) = std::env::current_exe() else { return };
    let dir = home.join("bin");
    if std::fs::create_dir_all(&dir).is_err() { return; }
    let shim = dir.join("homemade");
    let body = format!("#!/bin/sh\n# Installed by Homemade. Runs the launcher's CLI (see `homemade help`).\nexec \"{}\" \"$@\"\n", exe.display());
    if std::fs::read_to_string(&shim).ok().as_deref() != Some(body.as_str()) && std::fs::write(&shim, body).is_ok() {
        #[cfg(unix)]
        { use std::os::unix::fs::PermissionsExt; let _ = std::fs::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)); }
    }
    let mcp = serde_json::json!({ "mcpServers": { "homemade": { "command": exe, "args": ["--mcp"] } } });
    let _ = std::fs::write(mcp_config_path(home), serde_json::to_string_pretty(&mcp).unwrap_or_default());
}

pub fn mcp_config_path(home: &Path) -> PathBuf { home.join("mcp.json") }

/// Environment every child of Homemade (program runs, agent terminals, shells) receives so it can always find the launcher:
/// `<home>/bin` first on PATH (→ `homemade` command), plus explicit pointers for scripts and agents.
pub fn child_env(home: &Path, path_var: &str) -> Vec<(String, String)> {
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "homemade".into());
    let bin = home.join("bin").to_string_lossy().into_owned();
    let path = if path_var.split(':').any(|p| p == bin) { path_var.to_string() } else if path_var.is_empty() { bin.clone() } else { format!("{bin}:{path_var}") };
    vec![
        ("PATH".into(), path),
        ("HOMEMADE_HOME".into(), home.to_string_lossy().into_owned()),
        ("HOMEMADE_CLI".into(), exe.clone()),
        ("HOMEMADE_MCP_COMMAND".into(), format!("{exe} --mcp")),
        ("HOMEMADE_MCP_CONFIG".into(), mcp_config_path(home).to_string_lossy().into_owned()),
        ("HOMEMADE_CONTROL".into(), token_path(home).to_string_lossy().into_owned()),
    ]
}

/// One line for interactive shells / agents: how to drive the launcher from here.
pub fn hint_line() -> &'static str {
    "Homemade: this shell can drive the launcher — `homemade help` (CLI) or the MCP server `$HOMEMADE_MCP_COMMAND` (config: $HOMEMADE_MCP_CONFIG)."
}

fn s<'a>(p: &'a Value, k: &str) -> Result<&'a str, String> {
    p.get(k).and_then(|v| v.as_str()).filter(|v| !v.is_empty()).ok_or_else(|| format!("missing required parameter `{k}`"))
}
fn opt_s(p: &Value, k: &str) -> Option<String> { p.get(k).and_then(|v| v.as_str()).filter(|v| !v.is_empty()).map(String::from) }
fn b(p: &Value, k: &str) -> bool { p.get(k).and_then(|v| v.as_bool()).unwrap_or(false) }
fn j<T: serde::Serialize>(v: T) -> Result<Value, String> { serde_json::to_value(v).map_err(|e| e.to_string()) }
fn e(x: crate::error::Error) -> String { x.to_string() }

/// Every method the socket understands. Names mirror the Tauri command surface (CONTRACT.md) plus `status`, `wait_run`,
/// `open_app`, `close_window`, `show_view`.
pub const METHODS: &[&str] = &[
    "status", "list_apps", "get_app", "import_app", "create_project", "remove_app", "reload_manifest", "save_manifest", "draft_manifest", "apply_manifest",
    "start_run", "stop_run", "force_run", "wait_run", "get_run", "list_runs", "list_active_runs", "get_run_log", "rpc_call", "build_cli_command",
    "list_artifacts", "list_presets", "save_preset", "delete_preset",
    "git_status", "git_init", "git_checkpoint", "git_revert_to",
    "resource_snapshot", "list_listening_ports", "kill_port_owner", "app_process_info", "explain_run", "model_status",
    "list_agent_sessions", "storage_summary", "open_app", "close_window", "show_view", "app_activity",
    "app_usage", "app_processes", "signal_run", "write_stdin", "capture",
];

pub async fn dispatch(st: &Arc<AppState>, method: &str, p: &Value) -> Result<Value, String> {
    let res = crate::commands::resources_dir(&st.app_handle);
    let db = |f: &dyn Fn(&rusqlite::Connection) -> crate::error::Result<Value>| -> Result<Value, String> {
        let c = st.db.lock().unwrap(); f(&c).map_err(e)
    };
    match method {
        "status" => Ok(json!({
            "ok": true, "version": env!("CARGO_PKG_VERSION"), "pid": std::process::id(), "home": st.home,
            "active_runs": st.active_runs.len(), "active_terminals": st.active_ptys.len(),
            "model": st.model.status().await,
        })),
        "list_apps" => j(crate::commands::apps::list_apps_inner(st).map_err(e)?),
        "get_app" => {
            let (row, m) = crate::commands::apps::load_app(st, &res, s(p, "app_id")?).map_err(e)?;
            j(crate::commands::apps::to_record(&row, m))
        }
        "import_app" => {
            let rec = crate::commands::apps::import_path(st, &res, s(p, "path")?).await.map_err(e)?;
            st.emit("app://changed", json!({ "appId": rec.id }));
            j(rec)
        }
        // Blank project: new folder + manifest with an empty exec (Run stays disabled until exec is set), registered.
        "create_project" => {
            let parent = crate::commands::apps::resolve_parent_dir(&st.home, opt_s(p, "parent_dir").as_deref()).map_err(e)?;
            let root = crate::commands::apps::create_project_folder(&parent, s(p, "name")?).map_err(e)?;
            let rec = crate::commands::apps::import_path(st, &res, &root.to_string_lossy()).await.map_err(e)?;
            st.emit("app://changed", json!({ "appId": rec.id }));
            j(rec)
        }
        "remove_app" => {
            crate::commands::apps::remove_app_inner(st, s(p, "app_id")?, b(p, "purge_scratch"), b(p, "delete_venv")).map_err(e)?;
            st.emit("app://removed", json!({ "appId": s(p, "app_id")? }));
            Ok(json!({ "ok": true }))
        }
        "reload_manifest" => {
            let rec = crate::commands::apps::reload_manifest_inner(st, &res, s(p, "app_id")?).map_err(e)?;
            st.emit("app://changed", json!({ "appId": rec.id })); j(rec)
        }
        "save_manifest" => {
            let id = s(p, "app_id")?; let toml = s(p, "toml")?;
            crate::manifest::schema::HomemadeManifest::parse(toml).map_err(|x| format!("Invalid homemade.toml: {x}"))?;
            let row = db(&|c| crate::db::queries::get_app(c, id).map(|r| json!(r.map(|r| r.root_dir))))?;
            let Some(root) = row.as_str().map(String::from) else { return Err(format!("Unknown app: {id}")) };
            std::fs::write(Path::new(&root).join("homemade.toml"), toml).map_err(|x| x.to_string())?;
            let rec = crate::commands::apps::reload_manifest_inner(st, &res, id).map_err(e)?;
            st.emit("app://changed", json!({ "appId": rec.id })); j(rec)
        }
        "draft_manifest" => {
            let root = folder_of(s(p, "path")?)?;
            j(crate::intelligence::draft::draft_manifest(&st.model, &root, &res).await.map_err(e)?)
        }
        "apply_manifest" => j(crate::commands::model::apply_manifest_inner(st, s(p, "path")?, s(p, "toml")?).await.map_err(e)?),
        "start_run" => {
            let inputs = p.get("inputs").cloned().unwrap_or(json!({}));
            let rec = crate::commands::runs::start_run_inner(st.clone(), res, s(p, "app_id")?.to_string(), inputs).await.map_err(e)?;
            st.emit("run://status", &rec);
            j(rec)
        }
        "stop_run" => { crate::supervisor::spawner::stop_run(st.clone(), s(p, "run_id")?).await.map_err(e)?; Ok(json!({ "ok": true })) }
        "force_run" => { crate::supervisor::scheduler::force(st.clone(), s(p, "run_id")?).await.map_err(e)?; Ok(json!({ "ok": true })) }
        "get_run" => db(&|c| crate::db::queries::get_run(c, s(p, "run_id").unwrap_or("")).map(|r| json!(r))),
        "wait_run" => {
            let id = s(p, "run_id")?.to_string();
            let timeout = p.get("timeout_ms").and_then(|v| v.as_u64()).unwrap_or(600_000);
            let deadline = std::time::Instant::now() + std::time::Duration::from_millis(timeout);
            loop {
                let rec = { let c = st.db.lock().unwrap(); crate::db::queries::get_run(&c, &id).map_err(e)? };
                let Some(rec) = rec else { return Err(format!("Unknown run: {id}")) };
                let terminal = !matches!(rec.status.as_str(), "running" | "queued_vram" | "stopping");
                if terminal || !st.active_runs.contains_key(&id) && rec.status == "running" && std::time::Instant::now() > deadline { return j(rec); }
                if std::time::Instant::now() > deadline { return Err(format!("timeout after {timeout} ms; run {id} is still {}", rec.status)); }
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            }
        }
        "list_runs" => { let limit = p.get("limit").and_then(|v| v.as_i64()).unwrap_or(50); db(&|c| crate::db::queries::list_runs(c, s(p, "app_id").unwrap_or(""), limit).map(|r| json!(r))) }
        "list_active_runs" => {
            let c = st.db.lock().unwrap();
            let mut out = vec![];
            for r in st.active_runs.iter() { if let Some(rec) = crate::db::queries::get_run(&c, r.key()).map_err(e)? { out.push(rec); } }
            j(out)
        }
        "get_run_log" => {
            // Plain call → array of lines. With `offset` → {lines (from offset), next (offset for the next poll), total, active}:
            // agents/CLIs tail a run incrementally without re-reading.
            let id = s(p, "run_id")?;
            let active = st.active_runs.get(id).map(|r| r.value().clone());
            let lines: Vec<String> = match &active {
                Some(run) => run.log.lock().unwrap().iter().cloned().collect(),
                None => { let c = st.db.lock().unwrap(); crate::db::queries::get_run(&c, id).map_err(e)?.and_then(|r| r.log_tail).map(|t| t.lines().map(String::from).collect()).unwrap_or_default() }
            };
            match p.get("offset").and_then(|v| v.as_u64()) {
                None => Ok(json!(lines)),
                Some(off) => {
                    // The live buffer is a 5000-line ring: offsets count lines *emitted*, so clamp when the ring has rolled.
                    let total = lines.len() as u64;
                    let start = off.min(total) as usize;
                    Ok(json!({ "lines": &lines[start..], "next": total, "total": total, "active": active.is_some() }))
                }
            }
        }
        // Per-program usage: current sample + up to 5 min of 1 Hz history for this app's runs.
        "app_usage" => {
            let id = s(p, "app_id")?;
            let secs = p.get("seconds").and_then(|v| v.as_u64()).unwrap_or(60) as usize;
            let h = st.usage_history.lock().unwrap();
            let samples: Vec<Value> = h.iter().rev().take(secs).filter_map(|snap| {
                let u = snap.per_run.iter().find(|u| u.app_id == id)?;
                Some(json!({ "t": snap.t, "runId": u.run_id, "cpuPct": u.cpu_pct, "ramMb": u.ram_mb, "vramMb": u.vram_mb, "systemCpuPct": snap.cpu_pct, "systemRamUsedMb": snap.ram_used_mb, "gpuUsedMb": snap.gpu_used_mb }))
            }).collect::<Vec<_>>().into_iter().rev().collect();
            let current = st.resources.lock().unwrap().per_run.iter().find(|u| u.app_id == id).cloned();
            let peak = st.active_runs.iter().find(|r| r.value().app_id == id).map(|r| json!({ "ramMb": r.peak_ram_mb.load(std::sync::atomic::Ordering::Relaxed), "vramMb": r.peak_vram_mb.load(std::sync::atomic::Ordering::Relaxed) }));
            Ok(json!({ "appId": id, "current": current, "peak": peak, "samples": samples, "intervalMs": 1000 }))
        }
        // The whole process tree of the app's run (pgid), with per-process CPU/RSS from sysinfo.
        "app_processes" => {
            let id = s(p, "app_id")?;
            let Some(run) = st.active_runs.iter().find(|r| r.value().app_id == id).map(|r| r.value().clone()) else { return Ok(json!({ "appId": id, "runId": null, "processes": [] })) };
            let pgid = run.pgid;
            let procs = tokio::task::spawn_blocking(move || {
                use sysinfo::{ProcessesToUpdate, System};
                let mut sys = System::new();
                sys.refresh_processes(ProcessesToUpdate::All, true);
                let mut out = vec![];
                for (pid, pr) in sys.processes() {
                    let raw = pid.as_u32() as i32;
                    let gid = unsafe { libc::getpgid(raw) };
                    if gid != pgid && raw != pgid { continue; }
                    out.push(json!({ "pid": raw, "parentPid": pr.parent().map(|x| x.as_u32()), "name": pr.name().to_string_lossy(),
                        "cmd": pr.cmd().iter().map(|c| c.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" "),
                        "cpuPct": pr.cpu_usage(), "rssMb": pr.memory() / (1024 * 1024), "status": format!("{:?}", pr.status()), "startTime": pr.start_time() }));
                }
                out.sort_by_key(|p| p["pid"].as_i64());
                out
            }).await.map_err(|x| x.to_string())?;
            Ok(json!({ "appId": id, "runId": run.run_id, "pid": run.pid, "pgid": pgid, "port": *run.port.lock().unwrap(), "processes": procs }))
        }
        // Raw control: a POSIX signal to the run's process group (or just the leader with `leader_only`).
        "signal_run" => {
            let id = s(p, "run_id")?;
            let run = st.active_runs.get(id).map(|r| r.value().clone()).ok_or_else(|| format!("run {id} is not active"))?;
            let name = p.get("signal").and_then(|v| v.as_str()).unwrap_or("TERM").trim_start_matches("SIG").to_uppercase();
            let sig = match name.as_str() { "INT" => libc::SIGINT, "TERM" => libc::SIGTERM, "KILL" => libc::SIGKILL, "HUP" => libc::SIGHUP, "USR1" => libc::SIGUSR1, "USR2" => libc::SIGUSR2, "STOP" => libc::SIGSTOP, "CONT" => libc::SIGCONT, "QUIT" => libc::SIGQUIT, o => return Err(format!("unsupported signal {o}")) };
            if matches!(sig, libc::SIGTERM | libc::SIGKILL | libc::SIGINT) { run.stop_requested.store(true, std::sync::atomic::Ordering::SeqCst); }
            let rc = unsafe { if b(p, "leader_only") { libc::kill(run.pid, sig) } else { libc::killpg(run.pgid, sig) } };
            if rc != 0 { return Err(format!("kill failed: {}", std::io::Error::last_os_error())); }
            Ok(json!({ "ok": true, "signal": name, "pid": run.pid, "pgid": run.pgid }))
        }
        // Raw control: write to the program's stdin (every run has a piped stdin; add a newline yourself if the program reads lines).
        "write_stdin" => {
            let id = s(p, "run_id")?;
            let run = st.active_runs.get(id).map(|r| r.value().clone()).ok_or_else(|| format!("run {id} is not active"))?;
            let data = p.get("data").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let mut g = run.stdin.lock().await;
            let Some(w) = g.as_mut() else { return Err("stdin is closed for this run".into()) };
            use tokio::io::AsyncWriteExt;
            w.write_all(data.as_bytes()).await.map_err(|x| x.to_string())?;
            w.flush().await.map_err(|x| x.to_string())?;
            if b(p, "close") { *g = None; }
            Ok(json!({ "ok": true, "bytes": data.len() }))
        }
        // Screenshot of the program's window (its solo window if open, else the main window) → PNG path. macOS only.
        "capture" => {
            let app_id = opt_s(p, "app_id");
            let label = app_id.as_deref().map(|id| format!("app-{}", id.chars().map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect::<String>()));
            use tauri::Manager;
            let (win, which) = match label.as_deref().and_then(|l| st.app_handle.get_webview_window(l)) {
                Some(w) => (w, "solo-window"),
                None => (st.app_handle.get_webview_window("main").ok_or("no main window")?, "main-window"),
            };
            let path = opt_s(p, "path").map(PathBuf::from).unwrap_or_else(|| {
                let dir = st.home.join("captures"); let _ = std::fs::create_dir_all(&dir);
                dir.join(format!("{}-{}.png", app_id.as_deref().unwrap_or("homemade"), chrono::Local::now().format("%Y%m%d-%H%M%S")))
            });
            #[cfg(target_os = "macos")]
            {
                let ns = win.ns_window().map_err(|x| x.to_string())?;
                let num: isize = unsafe { objc2::msg_send![ns as *mut objc2::runtime::AnyObject, windowNumber] };
                let out = tokio::process::Command::new("screencapture").args(["-x", "-o", "-l", &num.to_string()]).arg(&path).output().await.map_err(|x| x.to_string())?;
                if !out.status.success() || !path.is_file() { return Err(format!("screencapture failed: {}", String::from_utf8_lossy(&out.stderr))); }
            }
            #[cfg(not(target_os = "macos"))]
            { let _ = &win; return Err("capture is only implemented on macOS".into()); }
            #[allow(unreachable_code)]
            Ok(json!({ "path": path, "window": which, "appId": app_id }))
        }
        "rpc_call" => {
            let app_id = s(p, "app_id")?;
            let rid = match opt_s(p, "run_id") {
                Some(r) => r,
                None => st.active_runs.iter().filter(|x| x.value().app_id == app_id && x.value().mode == "rpc_worker").map(|x| x.key().clone()).next()
                    .ok_or("No running worker for this app. Start it first.")?,
            };
            crate::supervisor::spawner::rpc_call(st.clone(), &rid, s(p, "method")?, p.get("params").cloned().unwrap_or(json!({}))).await.map_err(e)
        }
        "build_cli_command" => {
            let (row, manifest) = crate::commands::apps::load_app(st, &res, s(p, "app_id")?).map_err(e)?;
            let argv = crate::manifest::cli::build_argv(&manifest, &p.get("inputs").cloned().unwrap_or(json!({}))).map_err(e)?;
            let cwd = if manifest.app.cwd == "." { PathBuf::from(&row.root_dir) } else { Path::new(&row.root_dir).join(&manifest.app.cwd) };
            Ok(json!(crate::manifest::cli::display_command(&cwd.to_string_lossy(), &argv)))
        }
        "list_artifacts" => db(&|c| crate::db::queries::list_artifacts(c, s(p, "app_id").unwrap_or(""), opt_s(p, "run_id").as_deref(), opt_s(p, "kind").as_deref()).map(|r| json!(r))),
        "list_presets" => db(&|c| crate::db::queries::list_presets(c, s(p, "app_id").unwrap_or("")).map(|r| json!(r))),
        "save_preset" => {
            let pr = crate::types::PresetRecord { id: uuid::Uuid::new_v4().to_string(), app_id: s(p, "app_id")?.into(), name: s(p, "name")?.into(),
                inputs_json: p.get("inputs").cloned().unwrap_or(json!({})).to_string(), created_at: crate::types::now_ms() };
            db(&|c| crate::db::queries::insert_preset(c, &pr).map(|_| json!(pr)))
        }
        "delete_preset" => db(&|c| crate::db::queries::delete_preset(c, s(p, "preset_id").unwrap_or("")).map(|_| json!({ "ok": true }))),
        "git_status" | "git_init" | "git_checkpoint" | "git_revert_to" => {
            let root = crate::commands::artifacts::app_root(st, s(p, "app_id")?).map_err(e)?;
            use crate::integrations::git_ops as g;
            match method {
                "git_init" => { g::init(&root).map_err(e)?; }
                "git_checkpoint" => { let sha = g::checkpoint(&root, p.get("message").and_then(|v| v.as_str()).unwrap_or("Checkpoint")).map_err(e)?; return Ok(json!({ "sha": sha })); }
                "git_revert_to" => { g::revert_to(&root, s(p, "sha")?).map_err(e)?; }
                _ => {}
            }
            j(g::status(&root).map_err(e)?)
        }
        "resource_snapshot" => j(st.resources.lock().unwrap().clone()),
        "list_listening_ports" => j(crate::commands::system::list_listening_ports_inner(st).await.map_err(e)?),
        "kill_port_owner" => { let pid = p.get("pid").and_then(|v| v.as_i64()).ok_or("missing `pid`")? as i32; crate::commands::system::kill_port_owner_inner(st.clone(), pid).await.map_err(e)?; Ok(json!({ "ok": true })) }
        "app_process_info" => {
            let id = s(p, "app_id")?;
            Ok(st.active_runs.iter().find(|x| x.value().app_id == id).map(|x| { let r = x.value(); json!({ "runId": r.run_id, "pid": r.pid, "pgid": r.pgid, "port": *r.port.lock().unwrap(), "ready": r.webapp_ready.load(std::sync::atomic::Ordering::Relaxed) }) }).unwrap_or(Value::Null))
        }
        "explain_run" => j(crate::commands::model::explain_run_inner(st, s(p, "run_id")?).await.map_err(e)?),
        "model_status" => j(st.model.status().await),
        // Everything an agent needs to know about what its app is doing right now, in one call.
        "app_activity" => {
            let id = s(p, "app_id")?;
            let n = p.get("lines").and_then(|v| v.as_u64()).unwrap_or(40) as usize;
            let current = st.active_runs.iter().find(|x| x.value().app_id == id).map(|x| x.value().clone());
            let (history, artifacts) = { let c = st.db.lock().unwrap();
                (crate::db::queries::list_runs(&c, id, 5).map_err(e)?, crate::db::queries::list_artifacts(&c, id, None, Some("output")).map_err(e)?) };
            let cur = current.as_ref().map(|r| json!({
                "runId": r.run_id, "pid": r.pid, "port": *r.port.lock().unwrap(), "ready": r.webapp_ready.load(std::sync::atomic::Ordering::Relaxed),
                "startedAt": r.started_at, "elapsedMs": crate::types::now_ms() - r.started_at, "mode": r.mode,
                "peakRamMb": r.peak_ram_mb.load(std::sync::atomic::Ordering::Relaxed),
                "logTail": crate::supervisor::stdout_parser::tail(r, n).lines().collect::<Vec<_>>(),
            }));
            let last = history.iter().find(|r| !matches!(r.status.as_str(), "running" | "queued_vram" | "stopping"));
            let usage = st.resources.lock().unwrap().per_run.iter().find(|u| u.app_id == id).cloned();
            let agents: Vec<_> = st.active_ptys.iter().filter(|x| x.value().info.app_id == id).map(|x| x.value().info.clone()).collect();
            Ok(json!({
                "appId": id, "state": if current.is_some() { "running" } else { "idle" },
                "current": cur, "usage": usage, "lastFinished": last, "recentRuns": history, "latestOutputs": artifacts.iter().take(10).collect::<Vec<_>>(),
                "openAgentSessions": agents,
            }))
        }
        "list_agent_sessions" => {
            let id = s(p, "app_id")?.to_string();
            let root = crate::commands::artifacts::app_root(st, &id).map_err(e)?;
            let found = crate::integrations::agents::discover(&root, &id).map_err(e)?;
            db(&|c| crate::integrations::agents::merge_with_db(c, &id, &found).map(|r| json!(r)))
        }
        "storage_summary" => {
            use crate::storage::venv_scanner::dir_size;
            let store = st.home.join("store");
            Ok(json!({ "storeRoot": store, "uvCacheBytes": dir_size(&store.join("uv_cache")), "pnpmStoreBytes": dir_size(&store.join("pnpm_store")), "sharedCacheBytes": dir_size(&store.join("shared_cache")) }))
        }
        // UI control: the main window listens and acts (src/lib/useTauriEvents.ts).
        "open_app" => { let id = s(p, "app_id")?; crate::commands::apps::load_app(st, &res, id).map_err(e)?; st.emit("control://open", json!({ "appId": id, "window": b(p, "window") })); Ok(json!({ "ok": true })) }
        "close_window" => { st.emit("control://close-window", json!({ "appId": s(p, "app_id")? })); Ok(json!({ "ok": true })) }
        "show_view" => { let v = s(p, "view")?; if !["workbench", "launch", "monitor", "settings"].contains(&v) { return Err("view must be workbench|launch|monitor|settings".into()); } st.emit("control://view", json!({ "view": v })); Ok(json!({ "ok": true })) }
        other => Err(format!("unknown method: {other} (known: {})", METHODS.join(", "))),
    }
}

fn folder_of(path: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(path).canonicalize().map_err(|e| format!("Cannot open {path}: {e}"))?;
    Ok(if p.is_file() { p.parent().map(Path::to_path_buf).unwrap_or(p) } else { p })
}
