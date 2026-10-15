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
        // A new app that already carries an env dir: remember it was not ours, so deletion leaves it unticked.
        if existing.is_none() {
            for d in [".venv", "node_modules"] {
                if root.join(d).is_dir() { let _ = queries::upsert_dep(&c, &row.id, "env", d, 0, "preexisting", true); }
            }
        }
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

// ---------- clean deletion (two-level removal: remove_app hides, delete_app cleans) ----------

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanItem {
    pub key: String,
    pub label: String,
    pub detail: Option<String>,
    /// -1 = unknown
    pub bytes: i64,
    pub checked: bool,
    pub implied_by: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeletePlan {
    pub app_id: String,
    pub name: String,
    pub root_dir: String,
    pub blocked: Option<String>,
    pub items: Vec<PlanItem>,
    pub always: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteResult {
    pub freed_bytes: i64,
    pub pruned_packages: Vec<String>,
    pub trashed: bool,
}

const ALWAYS_REMOVED: &[&str] = &["run history", "presets", "artifact records"];

fn blocked_reason(state: &AppState, app_id: &str) -> Option<String> {
    let running = state.active_runs.iter().any(|r| r.value().app_id == app_id);
    let shells = state.active_ptys.iter().any(|p| p.value().info.app_id == app_id);
    (running || shells).then(|| "Stop its runs and close its shells first".to_string())
}

fn env_dir_in(root: &Path) -> Option<PathBuf> {
    [".venv", "venv", "node_modules"].iter().map(|d| root.join(d)).find(|p| p.is_dir() && !p.is_symlink())
}

fn manifest_of(root: &Path) -> Option<HomemadeManifest> {
    std::fs::read_to_string(root.join("homemade.toml")).ok().and_then(|s| HomemadeManifest::parse(&s).ok())
}

/// Names (before '@') of this app's packages that no other registered app records.
fn exclusive_packages(c: &rusqlite::Connection, app_id: &str, deps: &[queries::DepRow]) -> Vec<String> {
    let others = queries::package_names_used_by_others(c, app_id).unwrap_or_default();
    let mut names: Vec<String> = deps.iter().filter(|d| d.kind == "package")
        .map(|d| d.key.split('@').next().unwrap_or("").to_string())
        .filter(|n| !n.is_empty() && !others.contains(n)).collect();
    names.sort();
    names.dedup();
    names
}

#[tauri::command]
pub async fn delete_plan(state: St<'_>, app_id: String) -> Result<DeletePlan> {
    let st = state.inner().clone();
    tokio::task::spawn_blocking(move || delete_plan_inner(&st, &app_id)).await.map_err(|e| crate::error::Error::Msg(e.to_string()))?
}

pub fn delete_plan_inner(state: &AppState, app_id: &str) -> Result<DeletePlan> {
    let (row, deps) = {
        let c = state.db.lock().unwrap();
        let row = queries::get_app(&c, app_id)?.ok_or_else(|| crate::error::Error::Msg(format!("Unknown app: {app_id}")))?;
        let deps = queries::list_deps(&c, app_id).unwrap_or_default();
        (row, deps)
    };
    let root = PathBuf::from(&row.root_dir);
    let always: Vec<String> = ALWAYS_REMOVED.iter().map(|s| s.to_string()).collect();
    if let Some(b) = blocked_reason(state, app_id) {
        return Ok(DeletePlan { app_id: app_id.into(), name: row.name, root_dir: row.root_dir, blocked: Some(b), items: vec![], always });
    }
    let mut items = vec![];
    let size = |p: &Path| -> i64 { if p.exists() { crate::storage::venv_scanner::dir_size(p) as i64 } else { 0 } };
    items.push(PlanItem { key: "folder".into(), label: "Project folder → Trash".into(), detail: Some(row.root_dir.clone()),
        bytes: if root.is_dir() { size(&root) } else { 0 }, checked: true, implied_by: None });
    if let Some(env) = env_dir_in(&root) {
        let name = env.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let source = deps.iter().find(|d| d.kind == "env").map(|d| d.source.as_str()).unwrap_or("preexisting");
        let homemade = source == "homemade";
        items.push(PlanItem { key: "env".into(), label: name,
            detail: Some(if homemade { "installed by Homemade" } else { "existed before Homemade" }.into()),
            bytes: size(&env), checked: homemade, implied_by: Some("folder".into()) });
    }
    if root.is_dir() {
        let cfg = manifest_of(&root).map(|m| m.artifacts).unwrap_or_default();
        let scratch = crate::storage::artifact_differ::scratch_bytes(&root, &cfg) as i64;
        if scratch > 0 {
            items.push(PlanItem { key: "scratch".into(), label: "Scratch files".into(), detail: None, bytes: scratch, checked: true, implied_by: Some("folder".into()) });
        }
    }
    let sessions = state.home.join("sessions").join(app_id);
    if sessions.is_dir() {
        items.push(PlanItem { key: "sessions".into(), label: "Agent session transcripts".into(), detail: Some(sessions.to_string_lossy().into_owned()),
            bytes: size(&sessions), checked: true, implied_by: None });
    }
    let exclusive = { let c = state.db.lock().unwrap(); exclusive_packages(&c, app_id, &deps) };
    if !exclusive.is_empty() {
        let mut detail: Vec<String> = exclusive.iter().take(12).cloned().collect();
        if exclusive.len() > 12 { detail.push("…".into()); }
        items.push(PlanItem { key: "packages".into(), label: format!("{} package{} used only by this program", exclusive.len(), if exclusive.len() == 1 { "" } else { "s" }),
            detail: Some(detail.join(", ")), bytes: -1, checked: true, implied_by: None });
    }
    for d in deps.iter().filter(|d| d.kind == "model") {
        let p = state.home.join("store").join("shared_cache").join(&d.key);
        if !p.exists() { continue; }
        items.push(PlanItem { key: format!("model:{}", d.key), label: d.key.clone(),
            detail: Some("downloaded by this program; other programs may read it".into()), bytes: size(&p), checked: false, implied_by: None });
    }
    Ok(DeletePlan { app_id: app_id.into(), name: row.name, root_dir: row.root_dir, blocked: None, items, always })
}

#[tauri::command]
pub async fn delete_app(state: St<'_>, app_id: String, keys: Vec<String>) -> Result<DeleteResult> {
    delete_app_inner(state.inner(), &app_id, &keys).await
}

pub async fn delete_app_inner(state: &std::sync::Arc<AppState>, app_id: &str, keys: &[String]) -> Result<DeleteResult> {
    if let Some(b) = blocked_reason(state, app_id) { return err(b); }
    let (st, id) = (state.clone(), app_id.to_string());
    let plan = tokio::task::spawn_blocking(move || delete_plan_inner(&st, &id)).await.map_err(|e| crate::error::Error::Msg(e.to_string()))??;
    let root = PathBuf::from(&plan.root_dir);
    let has = |k: &str| keys.iter().any(|x| x == k);
    let bytes_of = |k: &str| plan.items.iter().find(|i| i.key == k).map(|i| i.bytes.max(0)).unwrap_or(0);
    let mut freed: i64 = 0;
    let mut trashed = false;

    crate::manifest::watcher::unwatch_app(app_id);
    if has("folder") && root.is_dir() {
        trash::delete(&root).map_err(|e| crate::error::Error::Msg(format!("Could not move {} to the Trash: {e}", root.display())))?;
        trashed = true;
        freed += bytes_of("folder");
    } else if root.is_dir() {
        crate::integrations::agent_docs::remove(&root);
        if has("env") {
            for d in [".venv", "venv", "node_modules"] {
                let p = root.join(d);
                if p.is_dir() && !p.is_symlink() { std::fs::remove_dir_all(&p)?; }
            }
            freed += bytes_of("env");
        }
        if has("scratch") {
            let cfg = manifest_of(&root).map(|m| m.artifacts).unwrap_or_default();
            freed += crate::storage::artifact_differ::clean_scratch(&root, &cfg) as i64;
        }
    }
    if has("sessions") {
        let p = state.home.join("sessions").join(app_id);
        if p.is_dir() { std::fs::remove_dir_all(&p)?; freed += bytes_of("sessions"); }
    }

    // Shared stores: prune packages nobody else recorded, and models the user ticked.
    let mut pruned = vec![];
    if has("packages") {
        let (manager, names) = {
            let c = state.db.lock().unwrap();
            let deps = queries::list_deps(&c, app_id).unwrap_or_default();
            (manifest_of(&root).map(|m| m.app.env_manager).unwrap_or_else(|| "uv".into()), exclusive_packages(&c, app_id, &deps))
        };
        let mut env = state.login_env.clone();
        crate::storage::env_router::inject_shared_storage_env(&mut env, &state.home, true);
        let path = env.get("PATH").cloned().unwrap_or_default();
        match manager.as_str() {
            "pnpm" => {
                if let Some(pnpm) = crate::integrations::agents::which("pnpm", &path) {
                    let ok = tokio::process::Command::new(pnpm).args(["store", "prune"]).env_clear().envs(&env)
                        .stdin(std::process::Stdio::null()).output().await.map(|o| o.status.success()).unwrap_or(false);
                    if ok { pruned = names; }
                }
            }
            _ => {
                if let Some(uv) = crate::integrations::agents::which("uv", &path) {
                    for n in names {
                        let ok = tokio::process::Command::new(&uv).args(["cache", "clean", &n]).env_clear().envs(&env)
                            .stdin(std::process::Stdio::null()).output().await.map(|o| o.status.success()).unwrap_or(false);
                        if ok { pruned.push(n); }
                    }
                }
            }
        }
    }
    let shared = state.home.join("store").join("shared_cache");
    let shared_canon = shared.canonicalize().unwrap_or(shared.clone());
    for k in keys.iter().filter_map(|k| k.strip_prefix("model:")) {
        let p = shared.join(k);
        let Ok(canon) = p.canonicalize() else { continue };
        if !canon.starts_with(&shared_canon) || canon == shared_canon { continue; }
        let b = bytes_of(&format!("model:{k}"));
        let ok = if canon.is_dir() { std::fs::remove_dir_all(&canon).is_ok() } else { std::fs::remove_file(&canon).is_ok() };
        if ok { freed += b; }
    }

    {
        let c = state.db.lock().unwrap();
        queries::delete_deps(&c, app_id)?;
        queries::delete_app(&c, app_id)?;
    }
    Ok(DeleteResult { freed_bytes: freed.max(0), pruned_packages: pruned, trashed })
}
