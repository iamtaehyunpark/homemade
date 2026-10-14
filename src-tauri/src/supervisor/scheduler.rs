//! VRAM / unified-memory admission queue (spec §9). OWNER: agent C.
use crate::error::Result;
use crate::state::{AppState, PendingJob};
use std::sync::Arc;

const MARGIN_MB: u64 = 512;

fn required_mb(job: &PendingJob) -> u64 {
    if job.required_vram_mb > 0 { job.required_vram_mb } else { 2048 }
}

/// (admissible, reason-if-not)
fn admissible(state: &AppState, job: &PendingJob) -> (bool, String) {
    if !job.requires_gpu { return (true, String::new()); }
    let active_gpu = state.active_runs.iter().filter(|r| r.requires_gpu).count();
    if active_gpu == 0 { return (true, String::new()); }
    let (total, used) = {
        let r = state.resources.lock().unwrap();
        (r.gpu_total_mb, r.gpu_used_mb)
    };
    let req = required_mb(job);
    if total == 0 {
        return (false, format!("Waiting for {active_gpu} running GPU job(s) to finish (no GPU telemetry; needs {req} MB)"));
    }
    let free = total.saturating_sub(used);
    if free >= req + MARGIN_MB { (true, String::new()) } else {
        (false, format!("Waiting for {req} MB GPU memory ({free} MB free)"))
    }
}

fn emit_status(state: &AppState, run_id: &str, status: &str) {
    let rec = {
        let c = state.db.lock().unwrap();
        let _ = crate::db::queries::set_run_status(&c, run_id, status);
        crate::db::queries::get_run(&c, run_id).ok().flatten()
    };
    if let Some(rec) = rec { state.emit("run://status", rec); }
}

/// Entry point used by commands::runs::start_run. Non-GPU jobs spawn immediately.
/// GPU jobs: required = vram_estimate_mb.or(learned_peak_vram_mb>0).unwrap_or(2048);
/// admit if no other GPU run is active OR free_gpu_mb >= required + 512 (free = gpu_total - gpu_used from
/// state.resources). Otherwise push to state.vram_queue, set DB status 'queued_vram', emit "run://status"
/// and "run://queued" {runId, appId, requiredVramMb, reason}. Admitted jobs call spawner::launch.
pub async fn submit(state: Arc<AppState>, job: PendingJob) -> Result<()> {
    // Respect FIFO: if others are already waiting, a GPU job queues behind them.
    let queue_nonempty = job.requires_gpu && !state.vram_queue.lock().await.is_empty();
    let (ok, reason) = admissible(&state, &job);
    if ok && !queue_nonempty {
        return crate::supervisor::spawner::launch(state, job).await;
    }
    let reason = if reason.is_empty() { "Waiting behind earlier queued GPU jobs".to_string() } else { reason };
    let (run_id, app_id, req) = (job.run_id.clone(), job.app_id.clone(), required_mb(&job));
    state.vram_queue.lock().await.push_back(job);
    emit_status(&state, &run_id, "queued_vram");
    state.emit("run://queued", serde_json::json!({"runId": run_id, "appId": app_id, "requiredVramMb": req, "reason": reason}));
    Ok(())
}

/// Called by spawner on every run exit and by the 1s telemetry tick: pop queue front if it now fits.
pub async fn on_run_finished(state: Arc<AppState>) {
    {
        let job = {
            let mut q = state.vram_queue.lock().await;
            match q.front() {
                Some(front) if admissible(&state, front).0 => q.pop_front(),
                _ => None,
            }
        };
        let Some(job) = job else { return };
        let run_id = job.run_id.clone();
        emit_status(&state, &run_id, "running");
        if let Err(e) = crate::supervisor::spawner::launch(state.clone(), job).await {
            eprintln!("[scheduler] launch of queued run {run_id} failed: {e}");
            let c = state.db.lock().unwrap();
            let _ = crate::db::queries::finish_run(&c, &run_id, "failed", None, 0, 0, crate::types::now_ms(), &e.to_string());
            let rec = crate::db::queries::get_run(&c, &run_id).ok().flatten();
            drop(c);
            if let Some(rec) = rec { state.emit("run://status", rec); }
        }
        // Admit at most one per call; headroom is re-evaluated on the next tick.
    }
}

/// "Run Anyway": remove from queue and launch immediately regardless of headroom.
pub async fn force(state: Arc<AppState>, run_id: &str) -> Result<()> {
    let Some(job) = remove_queued(&state, run_id).await else {
        return crate::error::err(format!("Run {run_id} is not queued"));
    };
    emit_status(&state, run_id, "running");
    crate::supervisor::spawner::launch(state, job).await
}

/// Remove a queued job (used by stop_run on queued runs). Returns the job if it was queued.
pub async fn remove_queued(state: &AppState, run_id: &str) -> Option<PendingJob> {
    let mut q = state.vram_queue.lock().await;
    let pos = q.iter().position(|j| j.run_id == run_id)?;
    q.remove(pos)
}
