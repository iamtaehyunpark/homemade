//! Shared records exchanged between Rust and the frontend. All serialize camelCase.
use crate::manifest::schema::HomemadeManifest;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppRecord {
    pub id: String,
    pub name: String,
    pub root_dir: String,
    pub manifest_hash: String,
    pub execution_mode: String, // "cli" | "rpc_worker" | "webapp"
    pub unmanaged_venv_bytes: i64,
    pub learned_peak_ram_mb: i64,
    pub learned_peak_vram_mb: i64,
    pub created_at: i64,
    pub last_run_at: Option<i64>,
    /// Parsed (or inferred) manifest, always present in responses.
    pub manifest: HomemadeManifest,
    pub has_git: bool,
    pub has_custom_ui: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    pub id: String,
    pub app_id: String,
    /// 'queued_vram' | 'running' | 'stopping' | 'success' | 'failed' | 'killed'
    pub status: String,
    pub inputs_json: String,
    pub cli_command: String,
    pub exit_code: Option<i64>,
    pub peak_ram_mb: i64,
    pub peak_vram_mb: i64,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub log_tail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactRecord {
    pub id: String,
    pub run_id: String,
    pub app_id: String,
    pub file_path: String,
    pub file_size_bytes: i64,
    pub mime_type: String,
    pub kind: String, // 'output' | 'scratch'
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresetRecord {
    pub id: String,
    pub app_id: String,
    pub name: String,
    pub inputs_json: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentSessionRecord {
    pub session_id: String,
    pub app_id: String,
    /// 'claude_code' | 'codex' | 'antigravity' | 'custom_shell'
    pub agent_type: String,
    pub title: Option<String>,
    pub git_commit_before: Option<String>,
    pub git_commit_after: Option<String>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageRef {
    pub asset_hash: String,
    pub asset_path: String,
    pub app_id: String,
    pub size_bytes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSnapshot {
    pub cpu_pct: f32,
    pub ram_used_mb: u64,
    pub ram_total_mb: u64,
    /// On macOS this is unified-memory "In use system memory" from IOAccelerator; on Linux NVML used.
    pub gpu_used_mb: u64,
    /// macOS: recommended working-set budget (~75% of RAM); Linux: NVML total. 0 if no GPU telemetry.
    pub gpu_total_mb: u64,
    pub gpu_label: String, // "VRAM" on Linux/NVIDIA, "GPU memory headroom" on macOS
    pub per_run: Vec<RunResourceUsage>,
    pub queue: Vec<QueuedJobInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct RunResourceUsage {
    pub run_id: String,
    pub app_id: String,
    pub cpu_pct: f32,
    pub ram_mb: u64,
    pub vram_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueuedJobInfo {
    pub run_id: String,
    pub app_id: String,
    pub required_vram_mb: u64,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatus {
    pub initialized: bool,
    pub dirty: bool,
    pub head: Option<String>,
    pub log: Vec<GitLogEntry>,
    /// `git status --porcelain` entries: XY status code + path.
    #[serde(default)]
    pub dirty_files: Vec<GitDirtyFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDirtyFile { pub path: String, pub status: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitLogEntry {
    pub sha: String,
    pub message: String,
    pub timestamp: i64,
    #[serde(default)]
    pub parents: Vec<String>,
    /// Decorations split into names: "HEAD", branch and tag names.
    #[serde(default)]
    pub refs: Vec<String>,
    #[serde(default)]
    pub author: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedAgent {
    pub agent_type: String,
    pub binary_path: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PtyInfo {
    pub pty_id: String,
    pub app_id: String,
    pub agent_type: String,
    pub session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageSummary {
    pub store_root: String,
    pub uv_cache_bytes: u64,
    pub pnpm_store_bytes: u64,
    pub shared_cache_bytes: u64,
    pub unmanaged_venv_total_bytes: u64,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
