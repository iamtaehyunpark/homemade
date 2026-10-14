//! 1s poller: sysinfo CPU/RAM, per-run RSS (sum over process group), GPU memory. OWNER: agent C.
//! Linux: nvml-wrapper (memory_info + running_compute_processes mapped to run PIDs/descendants).
//! macOS: gpu_total = 75% of hw.memsize; gpu_used = "In use system memory" parsed from
//!   `ioreg -r -d 1 -w 0 -c IOAccelerator` (PerformanceStatistics); gpu_label = "GPU memory headroom".
//! Each tick: update ActiveRun.peak_ram_mb/peak_vram_mb (AtomicU64 fetch_max), write state.resources,
//! emit "resources://tick" ResourceSnapshot (include queue from state.vram_queue), then scheduler::on_run_finished(state).
use crate::state::AppState;
use std::sync::Arc;

pub fn start(state: Arc<AppState>) {
    let _ = state;
    // agent C: tauri::async_runtime::spawn(async move { loop { ...; sleep(1s) } })
}
