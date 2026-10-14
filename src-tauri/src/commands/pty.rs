//! OWNER: agent D.
use super::St;
use crate::error::Result;
use crate::types::PtyInfo;

#[tauri::command]
pub async fn pty_spawn(state: St<'_>, app_id: String, agent_type: String, session_id: Option<String>, command: Option<String>, cols: u16, rows: u16) -> Result<PtyInfo> {
    crate::supervisor::pty_manager::spawn(state.inner().clone(), &app_id, &agent_type, session_id, command, cols, rows).await
}
#[tauri::command]
pub async fn pty_write(state: St<'_>, pty_id: String, data: String) -> Result<()> { crate::supervisor::pty_manager::write(&state, &pty_id, &data) }
#[tauri::command]
pub async fn pty_resize(state: St<'_>, pty_id: String, cols: u16, rows: u16) -> Result<()> { crate::supervisor::pty_manager::resize(&state, &pty_id, cols, rows) }
#[tauri::command]
pub async fn pty_kill(state: St<'_>, pty_id: String) -> Result<()> { crate::supervisor::pty_manager::kill(&state, &pty_id) }
#[tauri::command]
pub async fn list_ptys(state: St<'_>) -> Result<Vec<PtyInfo>> { Ok(state.active_ptys.iter().map(|p| p.info.clone()).collect()) }
