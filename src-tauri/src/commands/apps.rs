//! OWNER: agent B.
use super::St;
use crate::error::Result;
use crate::types::AppRecord;

/// All registered apps with manifests loaded (manifest::infer::load_or_infer per app; if a folder vanished, still return it with manifest from DB-less fallback? No: skip missing folders but keep the row).
#[tauri::command]
pub async fn list_apps(state: St<'_>) -> Result<Vec<AppRecord>> { let _ = state; todo!("agent B") }

/// Import a folder (or a single script file: use its parent dir as root). Loads/infers manifest, writes homemade.toml if inferred,
/// id = manifest.app.id (slugified, de-duplicated with -2 suffix if another root uses it), scans unmanaged venv bytes,
/// upserts, starts manifest::watcher::watch_app, returns the record.
#[tauri::command]
pub async fn import_app(state: St<'_>, app: tauri::AppHandle, path: String) -> Result<AppRecord> { let _ = (state, app, path); todo!("agent B") }

#[tauri::command]
pub async fn get_app(state: St<'_>, app: tauri::AppHandle, app_id: String) -> Result<AppRecord> { let _ = (state, app, app_id); todo!("agent B") }

/// Re-read homemade.toml from disk (after agent edits) and update hash/mode.
#[tauri::command]
pub async fn reload_manifest(state: St<'_>, app: tauri::AppHandle, app_id: String) -> Result<AppRecord> { let _ = (state, app, app_id); todo!("agent B") }

/// Unregister. If purge_scratch: artifact_differ::clean_scratch. If delete_venv: rm -rf <root>/.venv. Never deletes source files.
#[tauri::command]
pub async fn remove_app(state: St<'_>, app_id: String, purge_scratch: bool, delete_venv: bool) -> Result<()> { let _ = (state, app_id, purge_scratch, delete_venv); todo!("agent B") }

/// Write the given manifest back to <root>/homemade.toml (used by the in-app manifest editor).
#[tauri::command]
pub async fn save_manifest(state: St<'_>, app: tauri::AppHandle, app_id: String, toml_text: String) -> Result<AppRecord> { let _ = (state, app, app_id, toml_text); todo!("agent B") }
