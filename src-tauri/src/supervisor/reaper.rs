//! 3-stage SIGINT -> SIGTERM -> SIGKILL process-group reaper (spec §5B). OWNER: agent A.
use nix::sys::signal::{killpg, Signal};
use nix::unistd::Pid;
use tokio::time::{sleep, Duration};

pub fn group_alive(pgid: i32) -> bool {
    killpg(Pid::from_raw(pgid), None).is_ok()
}

pub async fn terminate_process_group(pgid: i32, graceful_ms: u64, kill_ms: u64) {
    let target = Pid::from_raw(pgid);
    let _ = killpg(target, Signal::SIGINT);
    sleep(Duration::from_millis(graceful_ms)).await;
    if !group_alive(pgid) { return; }
    let _ = killpg(target, Signal::SIGTERM);
    sleep(Duration::from_millis(kill_ms)).await;
    if !group_alive(pgid) { return; }
    let _ = killpg(target, Signal::SIGKILL);
}

/// Startup sweep: runs the previous session left as running/stopping/queued whose process group is
/// still alive are terminated (SIGTERM, then SIGKILL after 2 s) and recorded as 'killed'.
/// Returns (reaped, recorded) counts. Called before any new run can start.
pub async fn reap_orphans(state: &std::sync::Arc<crate::state::AppState>) -> (usize, usize) {
    let rows = { let c = state.db.lock().unwrap(); crate::db::queries::list_unfinished_runs(&c).unwrap_or_default() };
    let (mut reaped, mut recorded) = (0usize, 0usize);
    for (run_id, app_id, pgid, port) in rows {
        let mut note = String::from("[homemade] run was still marked active when Homemade restarted");
        if let Some(pg) = pgid.filter(|p| *p > 1) {
            if group_alive(pg) {
                terminate_process_group(pg, 1000, 2000).await;
                reaped += 1;
                note = format!("[homemade] orphaned process group {pg} from a previous session was stopped at startup{}",
                    port.map(|p| format!(" (it held port {p})")).unwrap_or_default());
            }
        }
        recorded += 1;
        let c = state.db.lock().unwrap();
        let prev = crate::db::queries::get_run(&c, &run_id).ok().flatten();
        let tail = format!("{}\n{note}", prev.as_ref().and_then(|r| r.log_tail.clone()).unwrap_or_default()).trim().to_string();
        let _ = crate::db::queries::finish_run(&c, &run_id, "killed", None,
            prev.as_ref().map(|r| r.peak_ram_mb).unwrap_or(0), prev.as_ref().map(|r| r.peak_vram_mb).unwrap_or(0),
            crate::types::now_ms(), &tail);
        let _ = app_id;
    }
    (reaped, recorded)
}

/// Last-resort synchronous kill of every active process group (used on app exit).
pub fn kill_all_now(state: &crate::state::AppState) {
    for e in state.active_runs.iter() {
        let pg = e.value().pgid;
        if pg > 1 { let _ = killpg(Pid::from_raw(pg), Signal::SIGKILL); }
    }
}
