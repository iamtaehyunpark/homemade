//! `@homemade:` directive state machine (spec §6A). OWNER: agent A.
use crate::state::{ActiveRun, AppState};
use std::sync::Arc;

pub const RING_CAPACITY: usize = 5000;

/// Called for every line from stdout (is_stderr=false) or stderr (is_stderr=true).
/// Always: push into run.log ring (cap RING_CAPACITY) and emit "run://stdout" | "run://stderr" {runId, line}.
/// If line starts with "@homemade:" parse the directive (do NOT emit it as stdout):
///   progress <int> "<msg>"         -> emit "run://progress" {runId, percent, message}
///   artifact "<abs path>"          -> index into DB (kind=output, mime via mime_guess, size via metadata), emit "run://artifact" ArtifactRecord
///   metric <json object>           -> emit "run://metric" {runId, data}
///   port <u16>                     -> set run.port, emit "run://port" {runId, port}
///   worker_ready                   -> set run.worker_ready, emit "run://worker_ready" {runId}
///   rpc_response <json>            -> look up json.id in run.rpc_pending, send the whole json
/// Unknown directives are logged as stdout.
pub fn handle_line(state: &Arc<AppState>, run: &Arc<ActiveRun>, line: &str, is_stderr: bool) {
    let _ = (state, run, line, is_stderr);
    todo!("agent A")
}

/// Last `n` lines of the ring buffer joined by '\n'.
pub fn tail(run: &ActiveRun, n: usize) -> String {
    let log = run.log.lock().unwrap();
    let skip = log.len().saturating_sub(n);
    log.iter().skip(skip).cloned().collect::<Vec<_>>().join("\n")
}
