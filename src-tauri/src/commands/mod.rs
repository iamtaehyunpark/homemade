//! Tauri command surface. Names here are the frontend contract (see CONTRACT.md). Keep signatures stable.
pub mod apps;
pub mod runs;
pub mod presets;
pub mod artifacts;
pub mod storage;
pub mod resources;
pub mod pty;
pub mod agents;
pub mod git;
pub mod system;

use crate::state::AppState;
use std::sync::Arc;
pub type St<'a> = tauri::State<'a, Arc<AppState>>;

pub fn resources_dir(app: &tauri::AppHandle) -> std::path::PathBuf {
    use tauri::Manager;
    app.path().resource_dir().map(|p| p.join("resources")).unwrap_or_else(|_| std::path::PathBuf::from("resources"))
}
