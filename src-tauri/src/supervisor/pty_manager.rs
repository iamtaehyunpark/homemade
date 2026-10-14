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
    /// Last keystroke from the UI (ms). Launcher notices are only typed into an agent while the user is not typing.
    pub last_input: std::sync::atomic::AtomicI64,
    /// Receives `[homemade]` run notices: every agent, and custom commands (a custom agent) — not a plain interactive shell.
    pub notices: bool,
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
    // Every agent gets Homemade's MCP server wired in (full launcher control) and the system prompt says so.
    let mcp_cfg = crate::control::mcp_config_path(&state.home).to_string_lossy().to_string();
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "homemade".into());
    let argv: Vec<String> = match agent_type {
        "claude_code" => {
            let bin = resolve("claude")?;
            let prompt = agents::system_prompt(&app.name, app_id, &root, last_failed_tail.as_deref());
            let mut v = if let Some(id) = &session_id {
                vec![bin, "--resume".into(), id.clone()]
            } else {
                let id = uuid::Uuid::new_v4().to_string();
                session_id = Some(id.clone());
                vec![bin, "--session-id".into(), id]
            };
            v.extend(["--append-system-prompt".into(), prompt, "--mcp-config".into(), mcp_cfg.clone()]);
            v
        }
        "codex" => {
            let bin = resolve("codex")?;
            let mut v = match &session_id { Some(id) => vec![bin, "resume".into(), id.clone()], None => vec![bin] };
            v.extend(["-c".into(), format!("mcp_servers.homemade.command={}", serde_json::to_string(&exe).unwrap_or_default()),
                      "-c".into(), "mcp_servers.homemade.args=[\"--mcp\"]".into()]);
            v
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
                // Interactive shell: print how to reach the launcher, then exec the login shell.
                None => vec![shell.clone(), "-lc".into(), format!("printf '%s\\n' {}; exec {} -l", shell_words::quote(crate::control::hint_line()), shell_words::quote(&shell))],
            }
        }
        other => return err(format!("unknown agent type: {other}")),
    };

    // --- Auto-checkpoint (spec §10E) + provenance before spawn ---
    let is_agent = agent_type != "custom_shell";
    if is_agent {
        let res = crate::commands::resources_dir(&state.app_handle);
        if let Ok((m, _, _)) = crate::manifest::infer::load_or_infer(&root, &res) { crate::integrations::agent_docs::ensure(&root, app_id, &m, &state.home); }
    }
    let is_git = root.join(".git").exists();
    let auto_ckpt = is_agent && is_git && {
        let cfg = crate::storage::config::load(&state.home);
        crate::storage::config::get_bool(&cfg, "git", "auto_checkpoint", true)
    };
    let started_at = now_ms();
    let head_before = if auto_ckpt && git_ops::is_dirty(&root) {
        let r = root.clone();
        let msg = format!("homemade: before {} session", agent_label(agent_type));
        tokio::task::spawn_blocking(move || git_ops::checkpoint(&r, &msg)).await.ok().and_then(|r| r.ok())
            .or_else(|| git_ops::head_sha(&root))
    } else {
        git_ops::head_sha(&root)
    };
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
    for (k, v) in crate::control::child_env(&state.home, &path_var) { cmd.env(k, v); }
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
    let log_path_w = log_path.clone();

    let info = PtyInfo { pty_id: pty_id.clone(), app_id: app_id.into(), agent_type: agent_type.into(), session_id: session_id.clone() };
    let active = Arc::new(ActivePty {
        info: info.clone(),
        writer: std::sync::Mutex::new(writer),
        master: std::sync::Mutex::new(pair.master),
        killer: std::sync::Mutex::new(child.clone_killer()),
        pid: child.process_id(),
        root: root.clone(),
        log_path: log_path.clone(),
        last_input: std::sync::atomic::AtomicI64::new(0),
        notices: agent_type != "custom_shell" || command.as_deref().map(|c| !c.trim().is_empty()).unwrap_or(false),
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
        let log_path_w = log_path_w.clone();
        std::thread::spawn(move || {
            let code = child.wait().map(|s| s.exit_code() as i64).unwrap_or(-1);
            state.active_ptys.remove(&pty_id);
            if is_agent {
                let mut title = None;
                // Find the session a new codex/antigravity PTY created (they don't take a pre-chosen id).
                let mut sid = session_id.clone();
                if sid.is_none() && (agent_type == "codex" || agent_type == "antigravity") {
                    if let Ok(found) = agents::discover(&root, &app_id) {
                        if let Some(s) = found.into_iter()
                            .filter(|s| s.agent_type == agent_type && s.updated_at >= started_at - 2000)
                            .max_by_key(|s| s.updated_at)
                        {
                            title = s.title.clone();
                            sid = Some(s.session_id);
                        }
                    }
                }
                let head_after = if auto_ckpt && git_ops::is_dirty(&root) {
                    let mut msg = format!("homemade: after {} session", agent_label(&agent_type));
                    if let Some(t) = title.as_deref().filter(|t| *t != "New session") {
                        msg.push(' ');
                        msg.push_str(&t.chars().take(60).collect::<String>());
                    }
                    git_ops::checkpoint(&root, &msg).ok().or_else(|| git_ops::head_sha(&root))
                } else {
                    git_ops::head_sha(&root)
                };
                if let Some(id) = sid {
                    let before = if resuming { None } else { head_before };
                    let rec = AgentSessionRecord {
                        session_id: id.clone(), app_id, agent_type,
                        title,
                        git_commit_before: before.clone(),
                        git_commit_after: head_after.clone(), updated_at: now_ms(),
                    };
                    if let Ok(c) = state.db.lock() { let _ = crate::db::queries::upsert_agent_session(&c, &rec); }
                    spawn_session_summary(state.clone(), id, root.clone(), log_path_w.clone(), before, head_after);
                }
            }
            state.emit("pty://exit", serde_json::json!({ "ptyId": pty_id, "exitCode": code }));
        });
    }
    Ok(info)
}

/// After an agent session that changed the repo: ask the assistor for a one-line title when the session has none yet.
fn spawn_session_summary(state: Arc<AppState>, session_id: String, root: PathBuf, log_path: PathBuf, before: Option<String>, after: Option<String>) {
    tauri::async_runtime::spawn(async move {
        let current = {
            let Ok(c) = state.db.lock() else { return };
            c.query_row("SELECT title FROM agent_sessions WHERE session_id = ?1", rusqlite::params![session_id], |r| r.get::<_, Option<String>>(0)).ok().flatten()
        };
        if current.as_deref().map(|t| !t.trim().is_empty() && t != "New session").unwrap_or(false) { return; }
        let git = |args: &[&str]| std::process::Command::new("git").args(args).current_dir(&root).env("GIT_TERMINAL_PROMPT", "0").output().ok()
            .filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).to_string()).unwrap_or_default();
        let diff = match (&before, &after) {
            (Some(b), Some(a)) if b != a => git(&["diff", "--stat", &format!("{b}..{a}")]),
            _ if git_ops::is_dirty(&root) => git(&["diff", "--stat", "HEAD"]),
            _ => String::new(),
        };
        if diff.trim().is_empty() { return; }
        let diff: String = diff.lines().take(60).collect::<Vec<_>>().join("\n");
        let transcript = std::fs::read(&log_path).map(|b| strip_ansi(&String::from_utf8_lossy(&b))).unwrap_or_default();
        let lines: Vec<&str> = transcript.lines().map(str::trim_end).filter(|l| !l.trim().is_empty()).collect();
        let tail = lines[lines.len().saturating_sub(60)..].join("\n");
        let Some(title) = state.assistor.summarize_agent_session(&diff, &tail).await else { return };
        if let Ok(c) = state.db.lock() {
            let _ = c.execute("UPDATE agent_sessions SET title = ?2 WHERE session_id = ?1", rusqlite::params![session_id, title]);
        }
        state.emit("pty://session_summary", serde_json::json!({ "sessionId": session_id, "title": title }));
    });
}

/// Remove ANSI CSI/OSC escape sequences and carriage returns from a terminal transcript.
fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '\x1b' => match it.next() {
                Some('[') => { while let Some(&n) = it.peek() { it.next(); if ('@'..='~').contains(&n) { break; } } }
                Some(']') => { while let Some(n) = it.next() { if n == '\x07' { break; } if n == '\x1b' { it.next(); break; } } }
                _ => {}
            },
            '\r' => {}
            c if c.is_control() && c != '\n' && c != '\t' => {}
            c => out.push(c),
        }
    }
    out
}

fn agent_label(agent_type: &str) -> &str {
    match agent_type { "claude_code" => "claude", "antigravity" => "agy", other => other }
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
    p.last_input.store(now_ms(), std::sync::atomic::Ordering::Relaxed);
    let mut w = p.writer.lock().unwrap();
    w.write_all(data.as_bytes())?;
    w.flush()?;
    Ok(())
}

/// Type a launcher notice into an agent session as if the user had sent it (Claude Code / Codex queue it while busy).
/// Only when the user has not typed for `idle_ms`; otherwise waits up to 30 s for a pause, then gives up (the agent can
/// still ask `app_activity`). Plain interactive shells never get notices (see `ActivePty.notices`).
pub fn inject_notice(state: Arc<AppState>, app_id: &str, text: String) {
    let cfg = crate::storage::config::load(&state.home);
    if !crate::storage::config::get_bool(&cfg, "agents", "notify_runs", true) { return; }
    let targets: Vec<Arc<ActivePty>> = state.active_ptys.iter()
        .filter(|e| e.value().info.app_id == app_id && e.value().notices)
        .map(|e| e.value().clone()).collect();
    if targets.is_empty() { return; }
    let line = format!("{}\r", text.replace(['\r', '\n'], " ⏎ "));
    for p in targets {
        let (st, line) = (state.clone(), line.clone());
        tauri::async_runtime::spawn(async move {
            let deadline = now_ms() + 30_000;
            loop {
                if !st.active_ptys.contains_key(&p.info.pty_id) { return; }
                let idle = now_ms() - p.last_input.load(std::sync::atomic::Ordering::Relaxed);
                if idle >= 3_000 { break; }
                if now_ms() > deadline { return; }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            let mut w = p.writer.lock().unwrap();
            let _ = w.write_all(line.as_bytes()).and_then(|_| w.flush());
        });
    }
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
