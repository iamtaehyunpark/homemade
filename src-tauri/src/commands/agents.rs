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
pub async fn list_agent_sessions(state: St<'_>, app_id: String) -> Result<Vec<AgentSessionRecord>> {
    let root = {
        let c = state.db.lock().unwrap();
        std::path::PathBuf::from(crate::db::queries::get_app(&c, &app_id)?.ok_or("unknown app")?.root_dir)
    };
    let found = tokio::task::spawn_blocking({
        let (root, app_id) = (root.clone(), app_id.clone());
        move || crate::integrations::agents::discover(&root, &app_id)
    })
    .await
    .map_err(|e| crate::error::Error::Msg(e.to_string()))??;
    let c = state.db.lock().unwrap();
    crate::integrations::agents::merge_with_db(&c, &app_id, &found)
}
/// Install the skill; returns installed path.
#[tauri::command]
pub async fn install_skill(state: St<'_>, app: tauri::AppHandle) -> Result<String> {
    crate::integrations::skill::install(&super::resources_dir(&app), &state.home)
}
