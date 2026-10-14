//! VRAM / unified-memory admission queue (spec §9). OWNER: agent C.
use crate::error::Result;
use crate::state::{AppState, PendingJob};
use std::sync::Arc;

/// Entry point used by commands::runs::start_run. Non-GPU jobs spawn immediately.
/// GPU jobs: required = vram_estimate_mb.or(learned_peak_vram_mb>0).unwrap_or(2048);
/// admit if no other GPU run is active OR free_gpu_mb >= required + 512 (free = gpu_total - gpu_used from
/// state.resources). Otherwise push to state.vram_queue, set DB status 'queued_vram', emit "run://status"
/// and "run://queued" {runId, appId, requiredVramMb, reason}. Admitted jobs call spawner::launch.
pub async fn submit(state: Arc<AppState>, job: PendingJob) -> Result<()> {
    let _ = (state, job);
    todo!("agent C")
}

/// Called by spawner on every run exit and by the 1s telemetry tick: pop queue front if it now fits.
pub async fn on_run_finished(state: Arc<AppState>) {
    let _ = state;
}

/// "Run Anyway": remove from queue and launch immediately regardless of headroom.
pub async fn force(state: Arc<AppState>, run_id: &str) -> Result<()> {
    let _ = (state, run_id);
    todo!("agent C")
}

/// Remove a queued job (used by stop_run on queued runs). Returns the job if it was queued.
pub async fn remove_queued(state: &AppState, run_id: &str) -> Option<PendingJob> {
    let mut q = state.vram_queue.lock().await;
    let pos = q.iter().position(|j| j.run_id == run_id)?;
    q.remove(pos)
}
