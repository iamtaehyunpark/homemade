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
pub async fn clean_scratch(state: St<'_>, app: tauri::AppHandle, app_id: String) -> Result<u64> {
    let root = app_root(&state, &app_id)?;
    let (manifest, _, _) = crate::manifest::infer::load_or_infer(&root, &super::resources_dir(&app))?;
    let cfg = manifest.artifacts.clone();
    let root2 = root.clone();
    let freed = tauri::async_runtime::spawn_blocking(move || crate::storage::artifact_differ::clean_scratch(&root2, &cfg))
        .await.map_err(|e| crate::error::Error::Msg(e.to_string()))?;
    // Remove scratch rows whose files are gone (or all scratch rows: they were matched by the same globs).
    let c = state.db.lock().unwrap();
    crate::db::queries::delete_artifacts_by_kind(&c, &app_id, "scratch")?;
    Ok(freed)
}

pub(crate) fn app_root(state: &crate::state::AppState, app_id: &str) -> Result<std::path::PathBuf> {
    let c = state.db.lock().unwrap();
    match crate::db::queries::get_app(&c, app_id)? {
        Some(a) => Ok(std::path::PathBuf::from(a.root_dir)),
        None => crate::error::err(format!("Unknown app: {app_id}")),
    }
}
/// Remove a single artifact row (and the file if delete_file).
#[tauri::command]
pub async fn delete_artifact(state: St<'_>, app_id: String, file_path: String, delete_file: bool) -> Result<()> {
    if delete_file {
        match std::fs::remove_file(&file_path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    let c = state.db.lock().unwrap();
    crate::db::queries::delete_artifact_by_path(&c, &app_id, &file_path)
}
