//! `@homemade:` directive state machine (spec §6A). OWNER: agent A.
use crate::state::{ActiveRun, AppState};
use crate::types::{now_ms, ArtifactRecord};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
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
    let line = line.trim_end_matches(['\r', '\n']);
    if !is_stderr {
        if let Some(rest) = line.trim_start().strip_prefix("@homemade:") {
            // Directive lines are never stored in the ring nor emitted as stdout.
            handle_directive(state, run, rest);
            return;
        }
    }
    {
        let mut log = run.log.lock().unwrap();
        log.push_back(if is_stderr { format!("[stderr] {line}") } else { line.to_string() });
        while log.len() > RING_CAPACITY { log.pop_front(); }
    }
    let ev = if is_stderr { "run://stderr" } else { "run://stdout" };
    state.emit(ev, json!({ "runId": run.run_id, "line": line }));
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    if s.len() >= 2 && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\''))) {
        let inner = &s[1..s.len() - 1];
        // Accept JSON-style escapes when possible.
        if s.starts_with('"') {
            if let Ok(v) = serde_json::from_str::<String>(s) { return v; }
        }
        inner.to_string()
    } else {
        s.to_string()
    }
}

/// Returns true if the directive was recognized and consumed.
fn handle_directive(state: &Arc<AppState>, run: &Arc<ActiveRun>, rest: &str) -> bool {
    let (name, arg) = match rest.split_once(char::is_whitespace) {
        Some((n, a)) => (n, a.trim()),
        None => (rest.trim(), ""),
    };
    match name {
        "progress" => {
            let (num, msg) = match arg.split_once(char::is_whitespace) {
                Some((n, m)) => (n, unquote(m)),
                None => (arg, String::new()),
            };
            let Ok(pct) = num.trim().parse::<f64>() else { return false };
            let pct = pct.round().clamp(0.0, 100.0) as i64;
            state.emit("run://progress", json!({ "runId": run.run_id, "percent": pct, "message": msg }));
            true
        }
        "artifact" => {
            let raw = unquote(arg);
            if raw.is_empty() { return false; }
            let mut p = PathBuf::from(&raw);
            if p.is_relative() { p = run.root.join(p); }
            let p = std::fs::canonicalize(&p).unwrap_or(p);
            let path_str = p.to_string_lossy().to_string();
            let size = std::fs::metadata(&p).map(|m| m.len() as i64).unwrap_or(0);
            let mime = mime_guess::from_path(&p).first_or_octet_stream().essence_str().to_string();
            let rec = ArtifactRecord {
                id: uuid::Uuid::new_v4().to_string(),
                run_id: run.run_id.clone(),
                app_id: run.app_id.clone(),
                file_path: path_str.clone(),
                file_size_bytes: size,
                mime_type: mime,
                kind: "output".into(),
                created_at: now_ms(),
            };
            {
                let c = state.db.lock().unwrap();
                let _ = crate::db::queries::insert_artifact(&c, &rec);
            }
            run.announced_artifacts.lock().unwrap().push(path_str);
            state.emit("run://artifact", rec);
            true
        }
        "metric" => {
            let Ok(data) = serde_json::from_str::<Value>(arg) else { return false };
            state.emit("run://metric", json!({ "runId": run.run_id, "data": data }));
            true
        }
        "port" => {
            let Ok(port) = arg.trim().parse::<u16>() else { return false };
            *run.port.lock().unwrap() = Some(port);
            state.emit("run://port", json!({ "runId": run.run_id, "port": port }));
            true
        }
        "worker_ready" => {
            run.worker_ready.store(true, Ordering::SeqCst);
            state.emit("run://worker_ready", json!({ "runId": run.run_id }));
            true
        }
        "rpc_response" => {
            let Ok(v) = serde_json::from_str::<Value>(arg) else { return false };
            let id = match v.get("id") {
                Some(Value::String(s)) => s.clone(),
                Some(other) if !other.is_null() => other.to_string(),
                _ => return true,
            };
            if let Some((_, tx)) = run.rpc_pending.remove(&id) { let _ = tx.send(v); }
            true
        }
        _ => false,
    }
}

/// Last `n` lines of the ring buffer joined by '\n'.
pub fn tail(run: &ActiveRun, n: usize) -> String {
    let log = run.log.lock().unwrap();
    let skip = log.len().saturating_sub(n);
    log.iter().skip(skip).cloned().collect::<Vec<_>>().join("\n")
}
