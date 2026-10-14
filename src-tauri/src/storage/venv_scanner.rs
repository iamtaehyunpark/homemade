//! Unmanaged .venv detection and 1-click migration (spec §8B). OWNER: agent C.
use crate::error::Result;
use std::path::Path;

/// Physical bytes of `<root>/.venv` or `<root>/venv` (0 if none). Count st_blocks*512, dedupe by inode.
pub fn scan_unmanaged_venv(root: &Path) -> u64 { let _ = root; 0 }

/// Migration per spec: capture python version, `pip freeze > requirements.homemade.lock` (if no pyproject/requirements),
/// rm -rf .venv, `uv venv .venv --python X.Y`, `uv pip install -r <lock or requirements.txt>` with UV_CACHE_DIR/UV_LINK_MODE env.
/// Returns bytes after migration. Emit "storage://progress" {appId, message} lines while running.
pub async fn migrate_venv(root: &Path, homemade_home: &Path, app_id: &str, state: &crate::state::AppState) -> Result<u64> {
    let _ = (root, homemade_home, app_id, state);
    todo!("agent C")
}

/// Sizes of the store subdirectories for StorageSummary.
pub fn dir_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path).into_iter().filter_map(|e| e.ok()).filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum()
}
