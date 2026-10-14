//! OWNER: agent C.
use super::St;
use crate::error::Result;
use crate::types::ArtifactRecord;

#[tauri::command]
pub async fn list_artifacts(state: St<'_>, app_id: String, run_id: Option<String>, kind: Option<String>) -> Result<Vec<ArtifactRecord>> {
    let c = state.db.lock().unwrap();
    crate::db::queries::list_artifacts(&c, &app_id, run_id.as_deref(), kind.as_deref())
}
/// Delete scratch files + rows. Returns bytes freed.
#[tauri::command]
pub async fn clean_scratch(state: St<'_>, app: tauri::AppHandle, app_id: String) -> Result<u64> { let _ = (state, app, app_id); todo!("agent C") }
/// Remove a single artifact row (and the file if delete_file).
#[tauri::command]
pub async fn delete_artifact(state: St<'_>, app_id: String, file_path: String, delete_file: bool) -> Result<()> { let _ = (state, app_id, file_path, delete_file); todo!("agent C") }
