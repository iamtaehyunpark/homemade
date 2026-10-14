//! OWNER: agent C.
use super::St;
use crate::error::Result;
use crate::types::StorageSummary;

#[tauri::command]
pub async fn storage_summary(state: St<'_>) -> Result<StorageSummary> {
    use crate::storage::venv_scanner::dir_size;
    let store = state.home.join("store");
    let venv_total: i64 = {
        let c = state.db.lock().unwrap();
        crate::db::queries::list_apps(&c)?.iter().map(|a| a.unmanaged_venv_bytes.max(0)).sum()
    };
    let s2 = store.clone();
    let (uv, pnpm, shared) = tauri::async_runtime::spawn_blocking(move || {
        (dir_size(&s2.join("uv_cache")), dir_size(&s2.join("pnpm_store")), dir_size(&s2.join("shared_cache")))
    }).await.map_err(|e| crate::error::Error::Msg(e.to_string()))?;
    Ok(StorageSummary {
        store_root: store.to_string_lossy().into_owned(),
        uv_cache_bytes: uv, pnpm_store_bytes: pnpm, shared_cache_bytes: shared,
        unmanaged_venv_total_bytes: venv_total as u64,
    })
}
/// Re-scan <root>/.venv and persist bytes; returns bytes.
#[tauri::command]
pub async fn scan_venv(state: St<'_>, app_id: String) -> Result<u64> {
    let root = super::artifacts::app_root(&state, &app_id)?;
    let bytes = tauri::async_runtime::spawn_blocking(move || crate::storage::venv_scanner::scan_unmanaged_venv(&root))
        .await.map_err(|e| crate::error::Error::Msg(e.to_string()))?;
    let c = state.db.lock().unwrap();
    crate::db::queries::set_app_venv_bytes(&c, &app_id, bytes as i64)?;
    Ok(bytes)
}
#[tauri::command]
pub async fn migrate_venv(state: St<'_>, app_id: String) -> Result<u64> {
    let root = super::artifacts::app_root(&state, &app_id)?;
    let st = state.inner().clone();
    let bytes = crate::storage::venv_scanner::migrate_venv(&root, &st.home, &app_id, &st).await?;
    // A uv-managed venv no longer counts as unmanaged.
    let c = state.db.lock().unwrap();
    crate::db::queries::set_app_venv_bytes(&c, &app_id, 0)?;
    Ok(bytes)
}
#[tauri::command]
pub async fn list_secret_names(state: St<'_>) -> Result<Vec<String>> { crate::storage::secrets::list_names(&state.home) }
#[tauri::command]
pub async fn set_secret(state: St<'_>, name: String, value: String) -> Result<()> { crate::storage::secrets::set(&state.home, &name, &value) }
#[tauri::command]
pub async fn delete_secret(state: St<'_>, name: String) -> Result<()> { crate::storage::secrets::delete(&state.home, &name) }
