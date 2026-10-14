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
pub mod model;

use crate::state::AppState;
use std::sync::Arc;
pub type St<'a> = tauri::State<'a, Arc<AppState>>;

pub fn resources_dir(app: &tauri::AppHandle) -> std::path::PathBuf {
    use tauri::Manager;
    app.path().resource_dir().map(|p| p.join("resources")).unwrap_or_else(|_| std::path::PathBuf::from("resources"))
}

/// Resources dir without an AppHandle (used by `homemade --mcp`): `<exe>/../Resources/resources` in the macOS bundle,
/// `<exe dir>/resources` in dev (tauri-build copies them next to target/debug/homemade), else the source tree.
pub fn resources_dir_from_exe() -> std::path::PathBuf {
    if let Ok(exe) = std::env::current_exe() {
        let exe = exe.canonicalize().unwrap_or(exe);
        if let Some(d) = exe.parent() {
            for c in [d.join("../Resources/resources"), d.join("resources")] {
                if c.join("SKILL.md").is_file() { return c.canonicalize().unwrap_or(c); }
            }
        }
    }
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources")
}
