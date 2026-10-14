//! Global application state shared by every command and background task.
use crate::manifest::schema::HomemadeManifest;
use dashmap::DashMap;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};

pub struct AppState {
    pub db: std::sync::Mutex<rusqlite::Connection>,
    /// ~/.homemade
    pub home: PathBuf,
    /// Hydrated login-shell environment (supervisor::spawner::load_user_login_env).
    pub login_env: HashMap<String, String>,
    pub active_runs: DashMap<String, Arc<ActiveRun>>,
    pub active_ptys: DashMap<String, Arc<crate::supervisor::pty_manager::ActivePty>>,
    pub vram_queue: Mutex<VecDeque<PendingJob>>,
    pub resources: std::sync::Mutex<crate::types::ResourceSnapshot>,
    pub app_handle: tauri::AppHandle,
    /// Apple on-device model helper (spawned on demand).
    pub model: Arc<crate::intelligence::local_model::LocalModel>,
    /// Spec §12 assistor (Apple-backed or rule-based no-op).
    pub assistor: Arc<dyn crate::intelligence::traits::SemanticAssistor>,
}

/// A run that has been admitted and spawned. Owned by supervisor (agent A may add fields).
pub struct ActiveRun {
    pub run_id: String,
    pub app_id: String,
    pub pid: i32,
    /// == pid because we setsid() in pre_exec.
    pub pgid: i32,
    pub requires_gpu: bool,
    pub started_at: i64,
    pub graceful_timeout_ms: u64,
    pub kill_timeout_ms: u64,
    /// Ring buffer of the last 5000 stdout/stderr lines.
    pub log: std::sync::Mutex<VecDeque<String>>,
    /// Child stdin for rpc_worker mode; None once closed.
    pub stdin: Mutex<Option<tokio::process::ChildStdin>>,
    /// Pending RPC requests awaiting `@homemade:rpc_response` keyed by request id.
    pub rpc_pending: DashMap<String, oneshot::Sender<Value>>,
    /// Set once `@homemade:worker_ready` was seen.
    pub worker_ready: std::sync::atomic::AtomicBool,
    /// Port announced via `@homemade:port` (webapp mode).
    pub port: std::sync::Mutex<Option<u16>>,
    /// Set once the readiness probe connected to `port` (so late subscribers, e.g. a new window, can catch up).
    pub webapp_ready: std::sync::atomic::AtomicBool,
    /// Set when a stop was requested, so the exit handler records 'killed' instead of 'failed'.
    pub stop_requested: std::sync::atomic::AtomicBool,
    /// Peak RSS observed by the telemetry poller.
    pub peak_ram_mb: std::sync::atomic::AtomicU64,
    pub peak_vram_mb: std::sync::atomic::AtomicU64,
    /// App root dir (relative @homemade:artifact paths resolve against it).
    pub root: PathBuf,
    /// Execution mode string ("cli" | "rpc_worker" | "webapp").
    pub mode: String,
    /// Resolved secret values, used to redact stored logs.
    pub secrets: HashMap<String, String>,
    /// Paths already indexed via @homemade:artifact (post-run diff skips them).
    pub announced_artifacts: std::sync::Mutex<Vec<String>>,
}

/// A job waiting for admission (scheduler) or about to be spawned.
#[derive(Debug, Clone)]
pub struct PendingJob {
    pub run_id: String,
    pub app_id: String,
    pub root_dir: PathBuf,
    pub manifest: HomemadeManifest,
    pub inputs: Value,
    /// argv built by manifest::cli::build_argv (exec string split + flags from inputs).
    pub argv: Vec<String>,
    /// Shell-escaped single string for "Copy as CLI".
    pub cli_command: String,
    pub requires_gpu: bool,
    pub required_vram_mb: u64,
}

impl AppState {
    pub fn emit<T: serde::Serialize + Clone>(&self, event: &str, payload: T) {
        use tauri::Emitter;
        let _ = self.app_handle.emit(event, payload);
    }
    pub fn active_count(&self) -> usize {
        self.active_runs.len() + self.active_ptys.len()
    }
}
