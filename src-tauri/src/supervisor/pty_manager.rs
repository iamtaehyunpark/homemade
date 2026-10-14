//! portable-pty sessions for coding agents and interactive shells (spec §10B). OWNER: agent D.
use crate::error::Result;
use crate::state::AppState;
use crate::types::PtyInfo;
use std::sync::Arc;

pub struct ActivePty {
    pub info: PtyInfo,
    pub writer: std::sync::Mutex<Box<dyn std::io::Write + Send>>,
    pub master: std::sync::Mutex<Box<dyn portable_pty::MasterPty + Send>>,
    pub child: std::sync::Mutex<Box<dyn portable_pty::Child + Send + Sync>>,
    /// Transcript log at ~/.homemade/sessions/<app_id>/<pty_id>.log
    pub log_path: std::path::PathBuf,
}

/// Spawn an agent/shell in a PTY with cwd = app root and env = state.login_env (+TERM=xterm-256color, COLORTERM=truecolor).
/// agent_type: "claude_code" | "codex" | "antigravity" | "custom_shell".
/// - claude_code: resume => `claude --resume <id>`; new => generate uuid, `claude --session-id <uuid> --append-system-prompt <integrations::agents::system_prompt(app)>`
/// - codex: resume => `codex resume <id>`; new => `codex`
/// - antigravity: resume => `agy --conversation <id>`; new => `agy --add-dir <root>`
/// - custom_shell: `$SHELL -l` (or `command` if given)
/// Before spawn: git_ops::head_sha(root) -> upsert agent_sessions(git_commit_before). On exit: git_commit_after, emit "pty://exit" {ptyId, exitCode}.
/// Reader thread: emit "pty://data" {ptyId, data: String (lossy utf8)} and append to log_path.
pub async fn spawn(state: Arc<AppState>, app_id: &str, agent_type: &str, session_id: Option<String>, command: Option<String>, cols: u16, rows: u16) -> Result<PtyInfo> {
    let _ = (state, app_id, agent_type, session_id, command, cols, rows);
    todo!("agent D")
}
pub fn write(state: &AppState, pty_id: &str, data: &str) -> Result<()> { let _ = (state, pty_id, data); todo!("agent D") }
pub fn resize(state: &AppState, pty_id: &str, cols: u16, rows: u16) -> Result<()> { let _ = (state, pty_id, cols, rows); todo!("agent D") }
/// SIGHUP/kill the child, remove from active_ptys.
pub fn kill(state: &AppState, pty_id: &str) -> Result<()> { let _ = (state, pty_id); todo!("agent D") }
