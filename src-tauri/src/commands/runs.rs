//! OWNER: agent A (start_run/stop_run/force_run/rpc_call/get_run_log) and agent B (build_cli_command).
use super::St;
use crate::error::Result;
use crate::types::RunRecord;
use serde_json::Value;
use crate::state::PendingJob;
use std::path::PathBuf;

/// Create the run row (status 'queued_vram' if GPU and not admitted, else 'running' once spawned), build PendingJob via
/// manifest::cli::build_argv, then scheduler::submit. Returns the RunRecord as inserted.
#[tauri::command]
pub async fn start_run(state: St<'_>, app: tauri::AppHandle, app_id: String, inputs: Value) -> Result<RunRecord> {
    let st = state.inner().clone();
    let row = {
        let c = st.db.lock().unwrap();
        crate::db::queries::get_app(&c, &app_id)?
    };
    let Some(row) = row else { return crate::error::err(format!("Unknown app: {app_id}")) };
    let res_dir = super::resources_dir(&app);
    let root = PathBuf::from(&row.root_dir);
    let (manifest, _hash, _inferred) = {
        let (r, d) = (root.clone(), res_dir.clone());
        tokio::task::spawn_blocking(move || crate::manifest::infer::load_or_infer(&r, &d))
            .await
            .map_err(|e| crate::error::Error::Msg(e.to_string()))??
    };
    let argv = crate::manifest::cli::build_argv(&manifest, &inputs)?;
    let cli_command = crate::manifest::cli::display_command(&row.root_dir, &argv);
    let requires_gpu = manifest.resources.requires_gpu;
    let required_vram_mb = if requires_gpu {
        manifest.resources.vram_estimate_mb
            .or(if row.learned_peak_vram_mb > 0 { Some(row.learned_peak_vram_mb as u64) } else { None })
            .unwrap_or(2048)
    } else { 0 };
    let rec = RunRecord {
        id: uuid::Uuid::new_v4().to_string(),
        app_id: app_id.clone(),
        status: "running".into(),
        inputs_json: serde_json::to_string(&inputs)?,
        cli_command: cli_command.clone(),
        exit_code: None,
        peak_ram_mb: 0,
        peak_vram_mb: 0,
        started_at: crate::types::now_ms(),
        finished_at: None,
        log_tail: None,
    };
    {
        let c = st.db.lock().unwrap();
        crate::db::queries::insert_run(&c, &rec)?;
    }
    let job = PendingJob {
        run_id: rec.id.clone(), app_id, root_dir: root, manifest, inputs, argv, cli_command,
        requires_gpu, required_vram_mb,
    };
    crate::supervisor::scheduler::submit(st.clone(), job).await?;
    let c = st.db.lock().unwrap();
    Ok(crate::db::queries::get_run(&c, &rec.id)?.unwrap_or(rec))
}

#[tauri::command]
pub async fn stop_run(state: St<'_>, run_id: String) -> Result<()> {
    crate::supervisor::spawner::stop_run(state.inner().clone(), &run_id).await
}

/// "Run Anyway" for a queued run.
#[tauri::command]
pub async fn force_run(state: St<'_>, run_id: String) -> Result<()> {
    crate::supervisor::scheduler::force(state.inner().clone(), &run_id).await
}

#[tauri::command]
pub async fn list_runs(state: St<'_>, app_id: String, limit: Option<i64>) -> Result<Vec<RunRecord>> {
    let c = state.db.lock().unwrap();
    crate::db::queries::list_runs(&c, &app_id, limit.unwrap_or(50))
}

#[tauri::command]
pub async fn get_run(state: St<'_>, run_id: String) -> Result<Option<RunRecord>> {
    let c = state.db.lock().unwrap();
    crate::db::queries::get_run(&c, &run_id)
}

/// Live ring buffer for an active run, or log_tail lines for a finished one.
#[tauri::command]
pub async fn get_run_log(state: St<'_>, run_id: String) -> Result<Vec<String>> {
    if let Some(run) = state.active_runs.get(&run_id).map(|r| r.value().clone()) {
        return Ok(run.log.lock().unwrap().iter().cloned().collect());
    }
    let c = state.db.lock().unwrap();
    let rec = crate::db::queries::get_run(&c, &run_id)?;
    Ok(rec.and_then(|r| r.log_tail).map(|t| t.lines().map(String::from).collect()).unwrap_or_default())
}

/// rpc_worker mode. run_id optional: if None use the app's single active rpc run.
#[tauri::command]
pub async fn rpc_call(state: St<'_>, app_id: String, run_id: Option<String>, method: String, params: Value) -> Result<Value> {
    let st = state.inner().clone();
    let rid = match run_id {
        Some(r) => r,
        None => {
            let found = st.active_runs.iter()
                .filter(|e| e.value().app_id == app_id && e.value().mode == "rpc_worker")
                .map(|e| e.key().clone())
                .next();
            match found {
                Some(r) => r,
                None => return crate::error::err("No running worker for this app. Start it first."),
            }
        }
    };
    crate::supervisor::spawner::rpc_call(st, &rid, &method, params).await
}

/// Exact command string for "Copy as CLI" without running.
#[tauri::command]
pub async fn build_cli_command(state: St<'_>, app: tauri::AppHandle, app_id: String, inputs: Value) -> Result<String> {
    let (row, manifest) = crate::commands::apps::load_app(&state, &super::resources_dir(&app), &app_id)?;
    let argv = crate::manifest::cli::build_argv(&manifest, &inputs)?;
    let cwd = std::path::Path::new(&row.root_dir).join(&manifest.app.cwd);
    let cwd = if manifest.app.cwd == "." { std::path::PathBuf::from(&row.root_dir) } else { cwd };
    Ok(crate::manifest::cli::display_command(&cwd.to_string_lossy(), &argv))
}

/// Active runs (for the Running view / close guard).
#[tauri::command]
pub async fn list_active_runs(state: St<'_>) -> Result<Vec<RunRecord>> {
    let c = state.db.lock().unwrap();
    let mut out = vec![];
    for r in state.active_runs.iter() { if let Some(rec) = crate::db::queries::get_run(&c, r.key())? { out.push(rec); } }
    Ok(out)
}
