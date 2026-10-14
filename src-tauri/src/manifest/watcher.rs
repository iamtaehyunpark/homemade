//! notify-based file watcher per registered app for hot reload. OWNER: agent B.
//! Watch `<root>/homemade.toml` and the ui entry's directory (debounced 300ms). On change emit:
//!   - "app://changed"   { appId }  when homemade.toml changed (frontend re-fetches get_app)
//!   - "app://ui_reload" { appId }  when anything under the ui dir changed (AppSandbox reloads iframe)
use crate::state::AppState;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex, OnceLock};
use std::time::Duration;

static WATCHERS: OnceLock<Mutex<HashMap<String, RecommendedWatcher>>> = OnceLock::new();
fn watchers() -> &'static Mutex<HashMap<String, RecommendedWatcher>> { WATCHERS.get_or_init(|| Mutex::new(HashMap::new())) }

const IGNORED: &[&str] = &[".git", ".venv", "venv", "node_modules", "outputs", "__pycache__"];

fn ui_dir(root: &Path) -> PathBuf {
    let entry = std::fs::read_to_string(root.join("homemade.toml")).ok()
        .and_then(|s| crate::manifest::schema::HomemadeManifest::parse(&s).ok())
        .and_then(|m| m.ui.entry);
    match entry {
        Some(e) => root.join(e).parent().map(|p| p.to_path_buf()).unwrap_or_else(|| root.to_path_buf()),
        None => root.join("ui"),
    }
}

pub fn watch_app(state: Arc<AppState>, app_id: String, root_dir: PathBuf) {
    unwatch_app(&app_id);
    let root = root_dir.canonicalize().unwrap_or(root_dir);
    let (tx, rx) = mpsc::channel::<notify::Event>();
    let mut watcher = match notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if let Ok(ev) = res { let _ = tx.send(ev); }
    }) {
        Ok(w) => w,
        Err(e) => { eprintln!("[watcher] {app_id}: {e}"); return; }
    };
    if let Err(e) = watcher.watch(&root, RecursiveMode::Recursive) {
        eprintln!("[watcher] {app_id}: {e}");
        return;
    }
    let id = app_id.clone();
    let root2 = root.clone();
    std::thread::spawn(move || {
        let mut ui = ui_dir(&root2);
        // Loop ends when the watcher (and thus tx) is dropped.
        while let Ok(first) = rx.recv() {
            let mut evs = vec![first];
            while let Ok(ev) = rx.recv_timeout(Duration::from_millis(300)) { evs.push(ev); }
            let (mut changed, mut ui_changed) = (false, false);
            for ev in &evs {
                if matches!(ev.kind, notify::EventKind::Access(_)) { continue; }
                for p in &ev.paths {
                    let rel = p.strip_prefix(&root2).unwrap_or(p);
                    if rel.components().any(|c| IGNORED.contains(&c.as_os_str().to_string_lossy().as_ref())) { continue; }
                    if rel == Path::new("homemade.toml") { changed = true; }
                    if p.starts_with(&ui) { ui_changed = true; }
                }
            }
            if changed {
                ui = ui_dir(&root2);
                state.emit("app://changed", serde_json::json!({ "appId": id }));
            }
            if ui_changed { state.emit("app://ui_reload", serde_json::json!({ "appId": id })); }
        }
    });
    watchers().lock().unwrap().insert(app_id, watcher);
}

pub fn unwatch_app(app_id: &str) { watchers().lock().unwrap().remove(app_id); }
