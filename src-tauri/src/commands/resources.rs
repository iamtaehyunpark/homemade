//! OWNER: agent C.
use super::St;
use crate::error::Result;
use crate::types::ResourceSnapshot;

#[tauri::command]
pub async fn resource_snapshot(state: St<'_>) -> Result<ResourceSnapshot> { Ok(state.resources.lock().unwrap().clone()) }
