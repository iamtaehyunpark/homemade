//! OWNER: agent B.
use super::St;
use crate::db::queries::{self, AppRow};
use crate::error::{err, Result};
use crate::manifest::infer::{load_or_infer, slugify, write_manifest};
use crate::manifest::schema::HomemadeManifest;
use crate::state::AppState;
use crate::types::AppRecord;
use std::path::{Path, PathBuf};

fn to_record(row: &AppRow, mut manifest: HomemadeManifest) -> AppRecord {
    let root = Path::new(&row.root_dir);
    let has_custom_ui = manifest.ui.entry.as_ref().map(|e| root.join(e).is_file()).unwrap_or(false);
    manifest.app.id = row.id.clone();
    AppRecord {
        id: row.id.clone(), name: row.name.clone(), root_dir: row.root_dir.clone(), manifest_hash: row.manifest_hash.clone(),
        execution_mode: row.execution_mode.clone(), unmanaged_venv_bytes: row.unmanaged_venv_bytes,
        learned_peak_ram_mb: row.learned_peak_ram_mb, learned_peak_vram_mb: row.learned_peak_vram_mb,
        created_at: row.created_at, last_run_at: row.last_run_at,
        manifest, has_git: root.join(".git").exists(), has_custom_ui,
    }
}

/// Load the DB row and the (parsed or inferred) manifest of a registered app. Shared by other command modules.
pub fn load_app(state: &AppState, resources_dir: &Path, app_id: &str) -> Result<(AppRow, HomemadeManifest)> {
    let row = {
        let c = state.db.lock().unwrap();
        queries::get_app(&c, app_id)?
    };
    let Some(row) = row else { return err(format!("Unknown app: {app_id}")) };
    let root = Path::new(&row.root_dir);
    if !root.is_dir() { return err(format!("App folder not found: {}", row.root_dir)); }
    let (mut m, _, _) = load_or_infer(root, resources_dir)?;
    m.app.id = row.id.clone();
    Ok((row, m))
}

/// All registered apps with manifests loaded. Missing folders are skipped (row kept).
#[tauri::command]
pub async fn list_apps(state: St<'_>) -> Result<Vec<AppRecord>> {
    let rows = { let c = state.db.lock().unwrap(); queries::list_apps(&c)? };
    let res = super::resources_dir(&state.app_handle);
    let mut out = vec![];
    for row in rows {
        let root = Path::new(&row.root_dir);
        if !root.is_dir() { continue; }
        match load_or_infer(root, &res) {
            Ok((m, _, _)) => out.push(to_record(&row, m)),
            Err(e) => eprintln!("[apps] skipping {}: {e}", row.id),
        }
    }
    Ok(out)
}

/// Import a folder (or a single script file: use its parent dir as root).
#[tauri::command]
pub async fn import_app(state: St<'_>, app: tauri::AppHandle, path: String) -> Result<AppRecord> {
    let p = PathBuf::from(&path);
    let p = p.canonicalize().map_err(|e| crate::error::Error::Msg(format!("Cannot open {path}: {e}")))?;
    let root = if p.is_file() { p.parent().map(|x| x.to_path_buf()).unwrap_or(p) } else { p };
    let root_str = root.to_string_lossy().to_string();
    let res = super::resources_dir(&app);
    let (mut m, mut hash, inferred) = load_or_infer(&root, &res)?;
    if inferred {
        write_manifest(&root, &m)?;
        if let Ok(b) = std::fs::read(root.join("homemade.toml")) { hash = crate::manifest::infer::sha256_hex(&b); }
    }
    let venv_bytes = crate::storage::venv_scanner::scan_unmanaged_venv(&root) as i64;
    let row = {
        let c = state.db.lock().unwrap();
        let existing = queries::get_app_by_root(&c, &root_str)?;
        let (id, created_at, peak_ram, peak_vram, last_run) = match &existing {
            Some(r) => (r.id.clone(), r.created_at, r.learned_peak_ram_mb, r.learned_peak_vram_mb, r.last_run_at),
            None => {
                let base = slugify(&m.app.id);
                let mut id = base.clone();
                let mut n = 2;
                while let Some(o) = queries::get_app(&c, &id)? {
                    if o.root_dir == root_str { break; }
                    id = format!("{base}-{n}");
                    n += 1;
                }
                (id, crate::types::now_ms(), 0, 0, None)
            }
        };
        let row = AppRow {
            id, name: m.app.name.clone(), root_dir: root_str.clone(), manifest_hash: hash,
            execution_mode: m.app.mode.as_str().to_string(), unmanaged_venv_bytes: venv_bytes,
            learned_peak_ram_mb: peak_ram, learned_peak_vram_mb: peak_vram, created_at, last_run_at: last_run,
        };
        queries::upsert_app(&c, &row)?;
        row
    };
    m.app.id = row.id.clone();
    crate::manifest::watcher::watch_app(state.inner().clone(), row.id.clone(), root);
    Ok(to_record(&row, m))
}

#[tauri::command]
pub async fn get_app(state: St<'_>, app: tauri::AppHandle, app_id: String) -> Result<AppRecord> {
    let (row, m) = load_app(&state, &super::resources_dir(&app), &app_id)?;
    Ok(to_record(&row, m))
}

/// Re-read homemade.toml from disk (after agent edits) and update hash/mode.
#[tauri::command]
pub async fn reload_manifest(state: St<'_>, app: tauri::AppHandle, app_id: String) -> Result<AppRecord> {
    let (mut row, _) = load_app(&state, &super::resources_dir(&app), &app_id)?;
    let (m, hash, _) = load_or_infer(Path::new(&row.root_dir), &super::resources_dir(&app))?;
    row.manifest_hash = hash;
    row.execution_mode = m.app.mode.as_str().to_string();
    row.name = m.app.name.clone();
    { let c = state.db.lock().unwrap(); queries::upsert_app(&c, &row)?; }
    Ok(to_record(&row, m))
}

/// Unregister. Never deletes source files.
#[tauri::command]
pub async fn remove_app(state: St<'_>, app_id: String, purge_scratch: bool, delete_venv: bool) -> Result<()> {
    let row = { let c = state.db.lock().unwrap(); queries::get_app(&c, &app_id)? };
    crate::manifest::watcher::unwatch_app(&app_id);
    if let Some(row) = &row {
        let root = Path::new(&row.root_dir);
        if purge_scratch && root.is_dir() {
            let cfg = std::fs::read_to_string(root.join("homemade.toml")).ok()
                .and_then(|s| HomemadeManifest::parse(&s).ok()).map(|m| m.artifacts).unwrap_or_default();
            crate::storage::artifact_differ::clean_scratch(root, &cfg);
        }
        if delete_venv {
            for d in [".venv", "venv"] {
                let p = root.join(d);
                if p.is_dir() && !p.is_symlink() { std::fs::remove_dir_all(&p)?; }
            }
        }
    }
    let c = state.db.lock().unwrap();
    queries::delete_app(&c, &app_id)?;
    Ok(())
}

/// Write the given manifest back to <root>/homemade.toml (used by the in-app manifest editor).
#[tauri::command]
pub async fn save_manifest(state: St<'_>, app: tauri::AppHandle, app_id: String, toml_text: String) -> Result<AppRecord> {
    HomemadeManifest::parse(&toml_text).map_err(|e| crate::error::Error::Msg(format!("Invalid homemade.toml: {e}")))?;
    let row = { let c = state.db.lock().unwrap(); queries::get_app(&c, &app_id)? };
    let Some(row) = row else { return err(format!("Unknown app: {app_id}")) };
    std::fs::write(Path::new(&row.root_dir).join("homemade.toml"), &toml_text)?;
    reload_manifest(state, app, app_id).await
}
