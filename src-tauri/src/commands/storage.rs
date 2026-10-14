//! OWNER: agent C.
use super::St;
use crate::error::Result;
use crate::types::StorageSummary;

#[tauri::command]
pub async fn storage_summary(state: St<'_>) -> Result<StorageSummary> { let _ = state; todo!("agent C") }
/// Re-scan <root>/.venv and persist bytes; returns bytes.
#[tauri::command]
pub async fn scan_venv(state: St<'_>, app_id: String) -> Result<u64> { let _ = (state, app_id); todo!("agent C") }
#[tauri::command]
pub async fn migrate_venv(state: St<'_>, app_id: String) -> Result<u64> { let _ = (state, app_id); todo!("agent C") }
#[tauri::command]
pub async fn list_secret_names(state: St<'_>) -> Result<Vec<String>> { crate::storage::secrets::list_names(&state.home) }
#[tauri::command]
pub async fn set_secret(state: St<'_>, name: String, value: String) -> Result<()> { crate::storage::secrets::set(&state.home, &name, &value) }
#[tauri::command]
pub async fn delete_secret(state: St<'_>, name: String) -> Result<()> { crate::storage::secrets::delete(&state.home, &name) }
