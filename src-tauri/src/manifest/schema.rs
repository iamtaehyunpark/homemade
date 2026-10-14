//! Serde model of homemade.toml (spec.md §3). Unknown keys are ignored.
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomemadeManifest {
    pub app: AppConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub resources: ResourceConfig,
    #[serde(default)]
    pub environment: EnvConfig,
    #[serde(default)]
    pub artifacts: ArtifactConfig,
    #[serde(default)]
    pub inputs: IndexMap<String, InputFieldSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub id: String,
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub mode: ExecutionMode,
    /// Command executed in root_dir, e.g. "uv run python main.py". Inputs are appended as flags.
    pub exec: String,
    #[serde(default = "default_cwd")]
    pub cwd: String,
    #[serde(default = "default_env_mgr")]
    pub env_manager: String, // "uv" | "pnpm" | "cargo" | "byo" | "system"
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    #[default]
    Cli,
    RpcWorker,
    Webapp,
}
impl ExecutionMode {
    pub fn as_str(&self) -> &'static str {
        match self { ExecutionMode::Cli => "cli", ExecutionMode::RpcWorker => "rpc_worker", ExecutionMode::Webapp => "webapp" }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UiConfig {
    /// Relative path to custom HTML entry (e.g. "ui/index.html"). None => AutoForm.
    #[serde(default)]
    pub entry: Option<String>,
    #[serde(default = "default_layout")]
    pub default_layout: String, // "tab" | "split_right" | "window"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceConfig {
    #[serde(default)]
    pub requires_gpu: bool,
    #[serde(default)]
    pub vram_estimate_mb: Option<u64>,
    #[serde(default)]
    pub max_ram_mb: Option<u64>,
    #[serde(default)]
    pub isolated_cache: bool,
    #[serde(default = "default_graceful_ms")]
    pub graceful_timeout_ms: u64,
    #[serde(default = "default_kill_ms")]
    pub kill_timeout_ms: u64,
}
impl Default for ResourceConfig {
    fn default() -> Self {
        Self { requires_gpu: false, vram_estimate_mb: None, max_ram_mb: None, isolated_cache: false, graceful_timeout_ms: default_graceful_ms(), kill_timeout_ms: default_kill_ms() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnvConfig {
    /// Secret names resolved from the OS keychain vault at spawn.
    #[serde(default)]
    pub secrets: Vec<String>,
    #[serde(default)]
    pub vars: IndexMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactConfig {
    #[serde(default = "default_output_dir")]
    pub output_dir: String,
    #[serde(default)]
    pub scratch_globs: Vec<String>,
}
impl Default for ArtifactConfig {
    fn default() -> Self { Self { output_dir: default_output_dir(), scratch_globs: vec![] } }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct InputFieldSpec {
    /// "file" | "directory" | "file_list" | "string" | "int" | "float" | "bool" | "select" | "range"
    #[serde(rename = "type")]
    pub field_type: String,
    #[serde(default)]
    pub cli_flag: Option<String>,
    #[serde(default)]
    pub positional_index: Option<usize>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub required: Option<bool>,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub options: Option<Vec<String>>,
    #[serde(default)]
    pub extensions: Option<Vec<String>>,
    #[serde(default)]
    pub min: Option<f64>,
    #[serde(default)]
    pub max: Option<f64>,
    #[serde(default)]
    pub step: Option<f64>,
    /// "flag_only" (--translate) | "key_value" (--translate true)
    #[serde(default)]
    pub bool_style: Option<String>,
}

fn default_version() -> String { "0.1.0".into() }
fn default_cwd() -> String { ".".into() }
fn default_env_mgr() -> String { "uv".into() }
fn default_layout() -> String { "tab".into() }
fn default_graceful_ms() -> u64 { 3000 }
fn default_kill_ms() -> u64 { 2000 }
fn default_output_dir() -> String { "./outputs".into() }

impl HomemadeManifest {
    pub fn parse(toml_src: &str) -> Result<Self, toml::de::Error> { toml::from_str(toml_src) }
    pub fn to_toml(&self) -> String { toml::to_string_pretty(self).unwrap_or_default() }
}
