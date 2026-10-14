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
