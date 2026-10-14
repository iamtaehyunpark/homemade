//! OWNER: agent D.
use super::St;
use crate::error::Result;
use crate::types::GitStatus;

fn root(state: &St<'_>, app_id: &str) -> Result<std::path::PathBuf> {
    let c = state.db.lock().unwrap();
    let a = crate::db::queries::get_app(&c, app_id)?.ok_or("unknown app")?;
    Ok(std::path::PathBuf::from(a.root_dir))
}
#[tauri::command]
pub async fn git_status(state: St<'_>, app_id: String) -> Result<GitStatus> { crate::integrations::git_ops::status(&root(&state, &app_id)?) }
#[tauri::command]
pub async fn git_init(state: St<'_>, app_id: String) -> Result<GitStatus> { let r = root(&state, &app_id)?; crate::integrations::git_ops::init(&r)?; crate::integrations::git_ops::status(&r) }
#[tauri::command]
pub async fn git_checkpoint(state: St<'_>, app_id: String, message: String) -> Result<String> { crate::integrations::git_ops::checkpoint(&root(&state, &app_id)?, &message) }
#[tauri::command]
pub async fn git_revert_to(state: St<'_>, app_id: String, sha: String) -> Result<GitStatus> { let r = root(&state, &app_id)?; crate::integrations::git_ops::revert_to(&r, &sha)?; crate::integrations::git_ops::status(&r) }
