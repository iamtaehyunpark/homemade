//! Login-env hydration and process-group spawning + run lifecycle. OWNER: agent A.
use crate::error::Result;
use crate::state::{AppState, PendingJob};
use std::collections::HashMap;
use std::sync::Arc;

/// Spec §5A: run `$SHELL -l -c 'env -0'` with a 5s timeout; merge over std::env::vars();
/// force PYTHONUNBUFFERED=1, FORCE_COLOR=1, PYTHONIOENCODING=utf-8. Called once at startup.
pub fn load_user_login_env() -> HashMap<String, String> {
    let mut m: HashMap<String, String> = std::env::vars().collect();
    m.insert("PYTHONUNBUFFERED".into(), "1".into());
    m.insert("FORCE_COLOR".into(), "1".into());
    m // agent A: replace with real implementation
}

/// Spawn an admitted job (called by scheduler, never directly by commands).
/// Steps: build env = login_env + storage::env_router::inject_shared_storage_env + manifest.environment.vars
/// (+ "$DATA_DIR"-style alias expansion is NOT required in v1) + storage::secrets::resolve(manifest.environment.secrets)
/// + HOMEMADE_RUN_ID / HOMEMADE_APP_ID / HOMEMADE_OUTPUT_DIR (absolute). cwd = root_dir/manifest.app.cwd.
/// pre_exec: setsid(); Linux: prctl(PR_SET_PDEATHSIG, SIGTERM); Linux + max_ram_mb: setrlimit(RLIMIT_AS).
/// stdin/stdout/stderr piped. Insert ActiveRun into state.active_runs, set run status 'running' in DB,
/// emit "run://status" with the RunRecord. Spawn tokio tasks reading stdout/stderr line by line through
/// stdout_parser::handle_line. Take a storage::artifact_differ::Snapshot before spawn; on exit diff it,
/// insert artifacts, emit "run://artifact" per output artifact. On exit: status = killed if stop_requested,
/// success if code==0 else failed; finish_run in DB (log_tail = last 200 lines), update_app_learned_peaks,
/// remove from active_runs, emit "run://status", then call scheduler::on_run_finished(state).
pub async fn launch(state: Arc<AppState>, job: PendingJob) -> Result<()> {
    let _ = (state, job);
    todo!("agent A")
}

/// Stop: set stop_requested, DB status 'stopping' + emit run://status, then reaper::terminate_process_group.
/// If the run is still queued (not in active_runs), remove it from vram_queue and finish as 'killed'.
pub async fn stop_run(state: Arc<AppState>, run_id: &str) -> Result<()> {
    let _ = (state, run_id);
    todo!("agent A")
}

/// rpc_worker mode: write `{"id","method","params"}\n` to child stdin, await the matching
/// `@homemade:rpc_response` via ActiveRun.rpc_pending (30s timeout => Err). Err if worker not ready.
pub async fn rpc_call(state: Arc<AppState>, run_id: &str, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
    let _ = (state, run_id, method, params);
    todo!("agent A")
}

/// Stop every active run and PTY (used by the quit flow).
pub async fn stop_all(state: Arc<AppState>) {
    let _ = state;
    // agent A: iterate active_runs -> stop_run; active_ptys -> pty_manager::kill
}
