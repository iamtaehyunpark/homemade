//! notify-based file watcher per registered app for hot reload. OWNER: agent B.
//! Watch `<root>/homemade.toml` and `<root>/ui/**` (debounced 300ms). On change emit:
//!   - "app://changed"   { appId }  when homemade.toml changed (frontend re-fetches get_app)
//!   - "app://ui_reload" { appId }  when anything under ui/ changed (AppSandbox reloads iframe)
use crate::state::AppState;
use std::sync::Arc;

pub fn watch_app(state: Arc<AppState>, app_id: String, root_dir: std::path::PathBuf) {
    let _ = (state, app_id, root_dir);
    // todo!("agent B") -- must not panic when called; no-op until implemented.
}
pub fn unwatch_app(app_id: &str) { let _ = app_id; }
