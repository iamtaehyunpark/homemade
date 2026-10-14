//! OWNER: agent A (start_run/stop_run/force_run/rpc_call/get_run_log) and agent B (build_cli_command).
use super::St;
use crate::error::Result;
use crate::types::RunRecord;
use serde_json::Value;

/// Create the run row (status 'queued_vram' if GPU and not admitted, else 'running' once spawned), build PendingJob via
/// manifest::cli::build_argv, then scheduler::submit. Returns the RunRecord as inserted.
#[tauri::command]
pub async fn start_run(state: St<'_>, app: tauri::AppHandle, app_id: String, inputs: Value) -> Result<RunRecord> { let _ = (state, app, app_id, inputs); todo!("agent A") }

#[tauri::command]
pub async fn stop_run(state: St<'_>, run_id: String) -> Result<()> { let _ = (state, run_id); todo!("agent A") }

/// "Run Anyway" for a queued run.
#[tauri::command]
pub async fn force_run(state: St<'_>, run_id: String) -> Result<()> { let _ = (state, run_id); todo!("agent A") }

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
pub async fn get_run_log(state: St<'_>, run_id: String) -> Result<Vec<String>> { let _ = (state, run_id); todo!("agent A") }

/// rpc_worker mode. run_id optional: if None use the app's single active rpc run.
#[tauri::command]
pub async fn rpc_call(state: St<'_>, app_id: String, run_id: Option<String>, method: String, params: Value) -> Result<Value> { let _ = (state, app_id, run_id, method, params); todo!("agent A") }

/// Exact command string for "Copy as CLI" without running.
#[tauri::command]
pub async fn build_cli_command(state: St<'_>, app: tauri::AppHandle, app_id: String, inputs: Value) -> Result<String> { let _ = (state, app, app_id, inputs); todo!("agent B") }

/// Active runs (for the Running view / close guard).
#[tauri::command]
pub async fn list_active_runs(state: St<'_>) -> Result<Vec<RunRecord>> {
    let c = state.db.lock().unwrap();
    let mut out = vec![];
    for r in state.active_runs.iter() { if let Some(rec) = crate::db::queries::get_run(&c, r.key())? { out.push(rec); } }
    Ok(out)
}
