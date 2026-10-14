//! OWNER: agent B (trivial; may be finished by anyone).
use super::St;
use crate::error::Result;
use crate::types::PresetRecord;
use serde_json::Value;

#[tauri::command]
pub async fn list_presets(state: St<'_>, app_id: String) -> Result<Vec<PresetRecord>> {
    let c = state.db.lock().unwrap();
    crate::db::queries::list_presets(&c, &app_id)
}
#[tauri::command]
pub async fn save_preset(state: St<'_>, app_id: String, name: String, inputs: Value) -> Result<PresetRecord> {
    let p = PresetRecord { id: uuid::Uuid::new_v4().to_string(), app_id, name, inputs_json: inputs.to_string(), created_at: crate::types::now_ms() };
    let c = state.db.lock().unwrap();
    crate::db::queries::insert_preset(&c, &p)?;
    Ok(p)
}
#[tauri::command]
pub async fn delete_preset(state: St<'_>, preset_id: String) -> Result<()> {
    let c = state.db.lock().unwrap();
    crate::db::queries::delete_preset(&c, &preset_id)
}
