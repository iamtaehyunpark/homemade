//! Apple on-device Foundation Model via the `homemade-fm` Swift helper (JSON Lines over stdin/stdout).
//! The helper is spawned on demand, serialized behind a mutex, and killed after 120 s idle.
use crate::error::{err, Result};
use serde::Serialize;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const IDLE_KILL: Duration = Duration::from_secs(120);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);
const STATUS_TTL: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    /// "apple" | "none"
    pub provider: String,
    pub available: bool,
    pub reason: Option<String>,
    pub helper_path: Option<String>,
}

struct Helper {
    child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    stdout: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
}

pub struct LocalModel {
    provider: String,
    helper_path: Option<PathBuf>,
    proc: Arc<tokio::sync::Mutex<Option<Helper>>>,
    /// Instant of the last request, as millis since `epoch`.
    last_used: Arc<AtomicU64>,
    epoch: Instant,
    status_cache: std::sync::Mutex<Option<(Instant, ModelStatus)>>,
    next_id: AtomicU64,
}

impl LocalModel {
    /// provider from `[model] provider = "apple" | "off"` (default "apple" on macOS, "none" elsewhere).
    pub fn new(res_dir: &Path, config: &toml::Table) -> Self {
        let cfg = config.get("model").and_then(|v| v.as_table()).and_then(|t| t.get("provider")).and_then(|v| v.as_str()).map(str::to_string);
        let provider = match cfg.as_deref() {
            Some("off") | Some("none") => "none".to_string(),
            _ if cfg!(target_os = "macos") => "apple".to_string(),
            _ => "none".to_string(),
        };
        let p = res_dir.join("bin").join("homemade-fm");
        let helper_path = if provider == "apple" && p.is_file() { Some(p) } else { None };
        Self {
            provider, helper_path,
            proc: Arc::new(tokio::sync::Mutex::new(None)),
            last_used: Arc::new(AtomicU64::new(0)),
            epoch: Instant::now(),
            status_cache: std::sync::Mutex::new(None),
            next_id: AtomicU64::new(1),
        }
    }

    /// A model that is always unavailable (tests, disabled config).
    pub fn disabled() -> Self { Self::new(Path::new("/nonexistent"), &toml::Table::from_iter([("model".to_string(), toml::Value::Table(toml::Table::from_iter([("provider".to_string(), toml::Value::String("off".into()))])))])) }

    /// Static part of the status (no helper call).
    pub fn base_status_public(&self) -> ModelStatus { self.base_status() }

    fn base_status(&self) -> ModelStatus {
        ModelStatus { provider: self.provider.clone(), available: false, reason: None, helper_path: self.helper_path.as_ref().map(|p| p.to_string_lossy().to_string()) }
    }

    /// Availability (cached 60 s). Spawns the helper for the check; it then idles out like any other use.
    pub async fn status(&self) -> ModelStatus {
        if let Some((t, s)) = self.status_cache.lock().unwrap().clone() {
            if t.elapsed() < STATUS_TTL { return s; }
        }
        let mut s = self.base_status();
        if self.provider != "apple" {
            s.reason = Some(if cfg!(target_os = "macos") { "disabled in config" } else { "not supported on this platform" }.into());
        } else if self.helper_path.is_none() {
            s.reason = Some("helper not built (requires macOS 26 SDK)".into());
        } else {
            match self.request(json!({ "op": "status" }), Duration::from_secs(20)).await {
                Ok(v) => {
                    s.available = v.get("available").and_then(|x| x.as_bool()).unwrap_or(false);
                    s.reason = v.get("reason").and_then(|x| x.as_str()).map(str::to_string);
                }
                Err(e) => s.reason = Some(e.to_string()),
            }
        }
        *self.status_cache.lock().unwrap() = Some((Instant::now(), s.clone()));
        s
    }

    /// One completion with a fresh session. 120 s timeout. Temperature 0.2.
    pub async fn complete(&self, system: &str, prompt: &str, json: bool, max_tokens: u32) -> Result<String> {
        if self.helper_path.is_none() { return err(self.status().await.reason.unwrap_or_else(|| "local model unavailable".into())); }
        let v = self.request(json!({ "op": "complete", "system": system, "prompt": prompt, "max_tokens": max_tokens, "temperature": 0.2, "json": json }), REQUEST_TIMEOUT).await?;
        if v.get("ok").and_then(|x| x.as_bool()) == Some(true) {
            Ok(v.get("text").and_then(|x| x.as_str()).unwrap_or("").to_string())
        } else {
            err(v.get("error").and_then(|x| x.as_str()).unwrap_or("local model error").to_string())
        }
    }

    fn touch(&self) { self.last_used.store(self.epoch.elapsed().as_millis() as u64, Ordering::SeqCst); }

    async fn request(&self, mut req: Value, timeout: Duration) -> Result<Value> {
        let Some(path) = self.helper_path.clone() else { return err("local model helper not available") };
        let id = self.next_id.fetch_add(1, Ordering::SeqCst).to_string();
        req["id"] = Value::String(id.clone());
        let mut guard = self.proc.lock().await;
        self.touch();
        if guard.as_mut().map(|h| h.child.try_wait().ok().flatten().is_some()).unwrap_or(true) {
            *guard = Some(spawn_helper(&path)?);
            self.start_idle_reaper();
        }
        let h = guard.as_mut().unwrap();
        let line = format!("{}\n", serde_json::to_string(&req)?);
        let io = async {
            h.stdin.write_all(line.as_bytes()).await?;
            h.stdin.flush().await?;
            loop {
                let Some(l) = h.stdout.next_line().await? else { return err("local model helper exited") };
                let Ok(v) = serde_json::from_str::<Value>(&l) else { continue };
                if v.get("id").and_then(|x| x.as_str()) == Some(id.as_str()) { return Ok(v); }
            }
        };
        let res = match tokio::time::timeout(timeout, io).await {
            Ok(r) => r,
            Err(_) => err(format!("local model timed out after {} s", timeout.as_secs())),
        };
        if res.is_err() {
            if let Some(mut h) = guard.take() { let _ = h.child.start_kill(); }
        }
        self.touch();
        res
    }

    /// Kill the helper once it has been idle for 120 s. One reaper per spawned helper; it exits with it.
    fn start_idle_reaper(&self) {
        let proc = self.proc.clone();
        let last = self.last_used.clone();
        let epoch = self.epoch;
        tauri::async_runtime::spawn(async move {
            loop {
                tokio::time::sleep(Duration::from_secs(10)).await;
                let idle = epoch.elapsed().as_millis() as u64 - last.load(Ordering::SeqCst);
                if idle < IDLE_KILL.as_millis() as u64 { continue; }
                let Ok(mut g) = proc.try_lock() else { continue }; // busy => not idle
                if let Some(mut h) = g.take() { let _ = h.child.start_kill(); let _ = h.child.wait().await; }
                return;
            }
        });
    }
}

fn spawn_helper(path: &Path) -> Result<Helper> {
    let mut child = tokio::process::Command::new(path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| crate::error::Error::Msg(format!("spawn {}: {e}", path.display())))?;
    let stdin = child.stdin.take().ok_or("helper stdin")?;
    let stdout = BufReader::new(child.stdout.take().ok_or("helper stdout")?).lines();
    Ok(Helper { child, stdin, stdout })
}
