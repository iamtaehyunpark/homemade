//! portable-pty sessions for coding agents and interactive shells (spec §10B). OWNER: agent D.
use crate::error::{err, Result};
use crate::integrations::{agents, git_ops};
use crate::state::AppState;
use crate::types::{now_ms, AgentSessionRecord, PtyInfo};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;

pub struct ActivePty {
    pub info: PtyInfo,
    pub writer: std::sync::Mutex<Box<dyn std::io::Write + Send>>,
    pub master: std::sync::Mutex<Box<dyn portable_pty::MasterPty + Send>>,
    /// Killer handle; the Child itself is owned by the wait thread.
    pub killer: std::sync::Mutex<Box<dyn portable_pty::ChildKiller + Send + Sync>>,
    pub pid: Option<u32>,
    pub root: PathBuf,
    /// Transcript log at ~/.homemade/sessions/<app_id>/<pty_id>.log
    pub log_path: PathBuf,
}

/// Spawn an agent/shell in a PTY with cwd = app root and env = state.login_env (+TERM=xterm-256color, COLORTERM=truecolor).
/// agent_type: "claude_code" | "codex" | "antigravity" | "custom_shell".
/// Reader thread: emit "pty://data" {ptyId, data: String (lossy utf8)} and append to log_path.
pub async fn spawn(state: Arc<AppState>, app_id: &str, agent_type: &str, session_id: Option<String>, command: Option<String>, cols: u16, rows: u16) -> Result<PtyInfo> {
    // --- DB lookups (lock dropped before anything else) ---
    let (app, last_failed_tail) = {
        let c = state.db.lock().unwrap();
        let app = crate::db::queries::get_app(&c, app_id)?.ok_or("unknown app")?;
        let tail = crate::db::queries::list_runs(&c, app_id, 1)?
            .into_iter()
            .next()
            .filter(|r| r.status == "failed")
            .and_then(|r| r.log_tail);
        (app, tail)
    };
    let root = PathBuf::from(&app.root_dir);
    if !root.is_dir() {
        return err(format!("app root does not exist: {}", root.display()));
    }
    let path_var = state.login_env.get("PATH").cloned().unwrap_or_else(|| std::env::var("PATH").unwrap_or_default());
    let resolve = |bin: &str| -> Result<String> {
        agents::which(bin, &path_var)
            .map(|p| p.to_string_lossy().to_string())
            .ok_or_else(|| crate::error::Error::Msg(format!("`{bin}` not found on PATH")))
    };

    // --- Build argv ---
    let mut session_id = session_id.filter(|s| !s.trim().is_empty());
    let resuming = session_id.is_some();
    let argv: Vec<String> = match agent_type {
        "claude_code" => {
            let bin = resolve("claude")?;
            if let Some(id) = &session_id {
                vec![bin, "--resume".into(), id.clone()]
            } else {
                let id = uuid::Uuid::new_v4().to_string();
                session_id = Some(id.clone());
                let prompt = agents::system_prompt(&app.name, &root, last_failed_tail.as_deref());
                vec![bin, "--session-id".into(), id, "--append-system-prompt".into(), prompt]
            }
        }
        "codex" => {
            let bin = resolve("codex")?;
            match &session_id { Some(id) => vec![bin, "resume".into(), id.clone()], None => vec![bin] }
        }
        "antigravity" => {
            let bin = resolve("agy")?;
            match &session_id {
                Some(id) => vec![bin, "--conversation".into(), id.clone()],
                None => vec![bin, "--add-dir".into(), root.to_string_lossy().to_string()],
            }
        }
        "custom_shell" => {
            let shell = state.login_env.get("SHELL").cloned().or_else(|| std::env::var("SHELL").ok()).unwrap_or_else(|| "/bin/sh".into());
            match command.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
                Some(c) => vec![shell, "-lc".into(), c.to_string()],
                None => vec![shell, "-l".into()],
            }
        }
        other => return err(format!("unknown agent type: {other}")),
    };

    // --- Record session provenance before spawn ---
    let head_before = git_ops::head_sha(&root);
    if let Some(id) = &session_id {
        let rec = AgentSessionRecord {
            session_id: id.clone(), app_id: app_id.into(), agent_type: agent_type.into(),
            title: if resuming { None } else { Some("New session".into()) },
            git_commit_before: if resuming { None } else { head_before.clone() },
            git_commit_after: None, updated_at: now_ms(),
        };
        let c = state.db.lock().unwrap();
        crate::db::queries::upsert_agent_session(&c, &rec)?;
    }

    // --- PTY ---
    let pty_id = uuid::Uuid::new_v4().to_string();
    let size = PtySize { rows: rows.max(2), cols: cols.max(10), pixel_width: 0, pixel_height: 0 };
    let pair = native_pty_system().openpty(size).map_err(|e| crate::error::Error::Msg(format!("openpty: {e}")))?;
    let mut cmd = CommandBuilder::new(&argv[0]);
    cmd.args(&argv[1..]);
    cmd.cwd(&root);
    cmd.env_clear();
    for (k, v) in &state.login_env { cmd.env(k, v); }
    cmd.env("PATH", &path_var);
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("HOMEMADE_APP_ID", app_id);
    cmd.env("HOMEMADE_ROOT", &root);
    let mut child = pair.slave.spawn_command(cmd).map_err(|e| crate::error::Error::Msg(format!("spawn {}: {e}", argv[0])))?;
    drop(pair.slave);
    let reader = pair.master.try_clone_reader().map_err(|e| crate::error::Error::Msg(e.to_string()))?;
    let writer = pair.master.take_writer().map_err(|e| crate::error::Error::Msg(e.to_string()))?;

    let log_dir = state.home.join("sessions").join(app_id);
    let _ = std::fs::create_dir_all(&log_dir);
    let log_path = log_dir.join(format!("{pty_id}.log"));

    let info = PtyInfo { pty_id: pty_id.clone(), app_id: app_id.into(), agent_type: agent_type.into(), session_id: session_id.clone() };
    let active = Arc::new(ActivePty {
        info: info.clone(),
        writer: std::sync::Mutex::new(writer),
        master: std::sync::Mutex::new(pair.master),
        killer: std::sync::Mutex::new(child.clone_killer()),
        pid: child.process_id(),
        root: root.clone(),
        log_path: log_path.clone(),
    });
    state.active_ptys.insert(pty_id.clone(), active);

    // --- Reader thread ---
    {
        let state = state.clone();
        let pty_id = pty_id.clone();
        std::thread::spawn(move || read_loop(state, pty_id, reader, log_path));
    }
    // --- Wait thread ---
    {
        let state = state.clone();
        let pty_id = pty_id.clone();
        let agent_type = agent_type.to_string();
        let app_id = app_id.to_string();
        std::thread::spawn(move || {
            let code = child.wait().map(|s| s.exit_code() as i64).unwrap_or(-1);
            state.active_ptys.remove(&pty_id);
            if let Some(id) = session_id {
                let rec = AgentSessionRecord {
                    session_id: id, app_id, agent_type, title: None, git_commit_before: None,
                    git_commit_after: git_ops::head_sha(&root), updated_at: now_ms(),
                };
                if let Ok(c) = state.db.lock() { let _ = crate::db::queries::upsert_agent_session(&c, &rec); }
            }
            state.emit("pty://exit", serde_json::json!({ "ptyId": pty_id, "exitCode": code }));
        });
    }
    Ok(info)
}

fn read_loop(state: Arc<AppState>, pty_id: String, mut reader: Box<dyn Read + Send>, log_path: PathBuf) {
    let mut log = std::fs::OpenOptions::new().create(true).append(true).open(&log_path).ok();
    let mut buf = [0u8; 8192];
    let mut pending: Vec<u8> = Vec::new();
    loop {
        let n = match reader.read(&mut buf) { Ok(0) | Err(_) => break, Ok(n) => n };
        if let Some(f) = log.as_mut() { let _ = f.write_all(&buf[..n]); }
        pending.extend_from_slice(&buf[..n]);
        // Emit only complete UTF-8; carry an incomplete trailing sequence to the next chunk.
        let cut = match std::str::from_utf8(&pending) {
            Ok(_) => pending.len(),
            Err(e) if e.error_len().is_none() => e.valid_up_to(),
            Err(_) => pending.len(),
        };
        if cut == 0 { continue; }
        let data = String::from_utf8_lossy(&pending[..cut]).to_string();
        pending.drain(..cut);
        state.emit("pty://data", serde_json::json!({ "ptyId": pty_id, "data": data }));
    }
    if !pending.is_empty() {
        let data = String::from_utf8_lossy(&pending).to_string();
        state.emit("pty://data", serde_json::json!({ "ptyId": pty_id, "data": data }));
    }
}

fn get(state: &AppState, pty_id: &str) -> Result<Arc<ActivePty>> {
    state.active_ptys.get(pty_id).map(|p| p.clone()).ok_or_else(|| crate::error::Error::Msg(format!("no such pty: {pty_id}")))
}

pub fn write(state: &AppState, pty_id: &str, data: &str) -> Result<()> {
    let p = get(state, pty_id)?;
    let mut w = p.writer.lock().unwrap();
    w.write_all(data.as_bytes())?;
    w.flush()?;
    Ok(())
}

pub fn resize(state: &AppState, pty_id: &str, cols: u16, rows: u16) -> Result<()> {
    let p = get(state, pty_id)?;
    p.master
        .lock()
        .unwrap()
        .resize(PtySize { rows: rows.max(2), cols: cols.max(10), pixel_width: 0, pixel_height: 0 })
        .map_err(|e| crate::error::Error::Msg(e.to_string()))?;
    Ok(())
}

/// SIGHUP/kill the child, remove from active_ptys.
pub fn kill(state: &AppState, pty_id: &str) -> Result<()> {
    let Some((_, p)) = state.active_ptys.remove(pty_id) else { return Ok(()) };
    #[cfg(unix)]
    if let Some(pid) = p.pid {
        // Hang up the whole foreground job (agents spawn subprocesses).
        unsafe {
            let pgid = libc::getpgid(pid as i32);
            if pgid > 0 { libc::killpg(pgid, libc::SIGHUP); } else { libc::kill(pid as i32, libc::SIGHUP); }
        }
    }
    let _ = p.killer.lock().unwrap().kill();
    Ok(())
}
