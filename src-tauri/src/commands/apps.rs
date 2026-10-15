//! OWNER: agent B.
use super::St;
use crate::db::queries::{self, AppRow};
use crate::error::{err, Result};
use crate::manifest::infer::{load_or_infer, slugify, write_manifest};
use crate::manifest::schema::HomemadeManifest;
use crate::state::AppState;
use crate::types::AppRecord;
use std::path::{Path, PathBuf};

pub fn to_record(row: &AppRow, mut manifest: HomemadeManifest) -> AppRecord {
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
pub async fn list_apps(state: St<'_>) -> Result<Vec<AppRecord>> { list_apps_inner(&state) }

pub fn list_apps_inner(state: &AppState) -> Result<Vec<AppRecord>> {
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
    import_path(state.inner(), &super::resources_dir(&app), &path).await
}

/// Create a project without choosing a language or running any code.
/// An empty exec keeps Run disabled until the user or agent configures it.
#[tauri::command]
pub async fn create_project(state: St<'_>, app: tauri::AppHandle, parent_dir: Option<String>, name: String) -> Result<AppRecord> {
    let parent = resolve_parent_dir(&state.home, parent_dir.as_deref())?;
    let root = create_project_folder(&parent, &name)?;
    import_path(state.inner(), &super::resources_dir(&app), &root.to_string_lossy()).await
        .map_err(|e| crate::error::Error::Msg(format!("Created {} but could not add it to Homemade: {e}", root.display())))
}

/// Empty/missing parent → the default projects dir (`~/homemade` unless `[projects] dir` is configured), created on demand.
pub fn resolve_parent_dir(home: &Path, parent_dir: Option<&str>) -> Result<PathBuf> {
    match parent_dir.map(str::trim).filter(|s| !s.is_empty()) {
        Some(p) => Ok(PathBuf::from(p)),
        None => { let d = crate::storage::config::projects_dir(home); std::fs::create_dir_all(&d)?; Ok(d) }
    }
}

/// The default projects dir as a string (for the create dialog's prefilled location).
#[tauri::command]
pub async fn default_projects_dir(state: St<'_>) -> Result<String> { Ok(crate::storage::config::projects_dir(&state.home).to_string_lossy().into_owned()) }

pub fn create_project_folder(parent: &Path, name: &str) -> Result<PathBuf> {
    let name = name.trim();
    if name.is_empty() || name == "." || name == ".." || name.chars().any(|c| c == '/' || c == '\\' || c == ':' || c.is_control()) {
        return err("Enter a folder name without slashes or special characters");
    }
    let parent = parent.canonicalize()?;
    if !parent.is_dir() { return err("Choose an existing folder as the project location"); }
    let root = parent.join(name);
    // create_dir is atomic and never reuses or overwrites an existing folder.
    std::fs::create_dir(&root).map_err(|e| crate::error::Error::Msg(format!("Cannot create {}: {e}", root.display())))?;
    let manifest = crate::manifest::schema::HomemadeManifest {
        app: crate::manifest::schema::AppConfig {
            id: slugify(name), name: name.to_string(), version: "0.1.0".into(), description: None,
            mode: crate::manifest::schema::ExecutionMode::Cli, exec: String::new(), cwd: ".".into(), env_manager: "system".into(),
        },
        ui: crate::manifest::schema::UiConfig { entry: None, default_layout: "tab".into() },
        resources: Default::default(), environment: Default::default(), artifacts: Default::default(), inputs: Default::default(),
    };
    if let Err(e) = write_manifest(&root, &manifest) {
        // Only remove our folder if it is still empty.
        let _ = std::fs::remove_dir(&root);
        return Err(e);
    }
    Ok(root)
}

/// Body of `import_app`, shared with `apply_manifest`: infer/write manifest, upsert the row, start the watcher.
pub async fn import_path(state: &std::sync::Arc<AppState>, res: &Path, path: &str) -> Result<AppRecord> {
    let p = PathBuf::from(path);
    let p = p.canonicalize().map_err(|e| crate::error::Error::Msg(format!("Cannot open {path}: {e}")))?;
    let root = if p.is_file() { p.parent().map(|x| x.to_path_buf()).unwrap_or(p) } else { p };
    let root_str = root.to_string_lossy().to_string();
    let (mut m, mut hash, inferred) = load_or_infer(&root, res)?;
    if inferred {
        // The inspector found nothing but the `extra` fallback: ask the local model (if available) to read the code.
        let only_extra = m.inputs.len() == 1 && m.inputs.get("extra").map(|i| i.field_type == "args").unwrap_or(false);
        if only_extra && state.model.status().await.available {
            if let Some(src) = crate::manifest::infer::find_entry_script(&root).and_then(|s| std::fs::read_to_string(s).ok()) {
                if let Some(mut inputs) = state.assistor.infer_schema_from_raw_code(&src).await {
                    inputs.entry("extra".into()).or_insert_with(|| m.inputs["extra"].clone());
                    m.inputs = inputs;
                }
            }
        }
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
    crate::integrations::agent_docs::ensure(&root, &row.id, &m, &state.home);
    crate::manifest::watcher::watch_app(state.clone(), row.id.clone(), root);
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
    reload_manifest_inner(&state, &super::resources_dir(&app), &app_id)
}

pub fn reload_manifest_inner(state: &AppState, res: &Path, app_id: &str) -> Result<AppRecord> {
    let (mut row, _) = load_app(state, res, app_id)?;
    let (m, hash, _) = load_or_infer(Path::new(&row.root_dir), res)?;
    row.manifest_hash = hash;
    row.execution_mode = m.app.mode.as_str().to_string();
    row.name = m.app.name.clone();
    { let c = state.db.lock().unwrap(); queries::upsert_app(&c, &row)?; }
    crate::integrations::agent_docs::ensure(Path::new(&row.root_dir), &row.id, &m, &state.home);
    Ok(to_record(&row, m))
}

/// Unregister. Never deletes source files.
#[tauri::command]
pub async fn remove_app(state: St<'_>, app_id: String, purge_scratch: bool, delete_venv: bool) -> Result<()> {
    remove_app_inner(&state, &app_id, purge_scratch, delete_venv)
}

pub fn remove_app_inner(state: &AppState, app_id: &str, purge_scratch: bool, delete_venv: bool) -> Result<()> {
    let app_id = app_id.to_string();
    let row = { let c = state.db.lock().unwrap(); queries::get_app(&c, &app_id)? };
    crate::manifest::watcher::unwatch_app(&app_id);
    if let Some(r) = &row { crate::integrations::agent_docs::remove(Path::new(&r.root_dir)); }
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
    reload_manifest_inner(&state, &super::resources_dir(&app), &app_id)
}

#[cfg(test)]
mod project_tests {
    use super::*;

    #[test]
    fn creates_blank_project_and_refuses_overwrite() {
        let parent = std::env::temp_dir().join(format!("homemade-project-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&parent).unwrap();
        let root = create_project_folder(&parent, "My project").unwrap();
        let bytes = std::fs::read_to_string(root.join("homemade.toml")).unwrap();
        let manifest = HomemadeManifest::parse(&bytes).unwrap();
        assert_eq!(manifest.app.name, "My project");
        assert!(manifest.app.exec.is_empty());
        assert_eq!(manifest.ui.default_layout, "tab");
        std::fs::write(root.join("keep.txt"), "existing content").unwrap();
        assert!(create_project_folder(&parent, "My project").is_err());
        assert_eq!(std::fs::read_to_string(root.join("keep.txt")).unwrap(), "existing content");
        assert_eq!(std::fs::read_to_string(root.join("homemade.toml")).unwrap(), bytes);
        std::fs::remove_dir_all(parent).unwrap();
    }

    #[test]
    fn rejects_names_that_escape_the_chosen_folder() {
        for name in ["", " ", ".", "..", "../outside", "/tmp/outside", "nested/folder", "nested\\folder", "bad:name", "bad\nname"] {
            assert!(create_project_folder(&std::env::temp_dir(), name).is_err(), "accepted {name:?}");
        }
    }
}
