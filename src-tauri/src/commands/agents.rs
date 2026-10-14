//! OWNER: agent D.
use super::St;
use crate::error::Result;
use crate::types::{AgentSessionRecord, DetectedAgent};

#[tauri::command]
pub async fn detect_agents(state: St<'_>) -> Result<Vec<DetectedAgent>> {
    Ok(crate::integrations::agents::detect(state.login_env.get("PATH").map(|s| s.as_str()).unwrap_or("")))
}
/// Discovery + DB merge.
#[tauri::command]
pub async fn list_agent_sessions(state: St<'_>, app_id: String) -> Result<Vec<AgentSessionRecord>> { let _ = (state, app_id); todo!("agent D") }
/// Install the skill; returns installed path.
#[tauri::command]
pub async fn install_skill(state: St<'_>, app: tauri::AppHandle) -> Result<String> {
    crate::integrations::skill::install(&super::resources_dir(&app), &state.home)
}
