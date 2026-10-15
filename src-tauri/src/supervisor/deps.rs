//! `[app] env_manager` enforcement: install dependencies before a run (uv sync / pnpm install) when the
//! lockfiles changed, put the app's env bin dir on PATH for runs and shells, and record what each app owns
//! (packages, env dir, shared_cache models) in the `deps` table so `delete_app` can prune what nobody else uses.
use crate::db::queries;
use crate::error::{err, Result};
use crate::integrations::agents;
use crate::manifest::schema::HomemadeManifest;
use crate::state::AppState;
use crate::storage::venv_scanner;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, BufReader};

/// What running the app needs: install commands (run in order), an argv prefix, bin dirs for PATH,
/// the env dir that holds the install, and the input files whose content decides whether to reinstall.
#[derive(Debug, Clone, Default)]
pub struct Plan {
    pub install: Vec<Vec<String>>,
    pub wrap: Vec<String>,
    pub bins: Vec<PathBuf>,
    pub env_dir: Option<PathBuf>,
    pub inputs: Vec<PathBuf>,
    pub manager: String,
}

fn sv(parts: &[&str]) -> Vec<String> { parts.iter().map(|s| s.to_string()).collect() }

/// Decide the plan for `env_manager` using the current process PATH to pick pnpm vs npm.
pub fn plan(root: &Path, m: &HomemadeManifest, argv0: &str) -> Plan {
    let path = std::env::var("PATH").unwrap_or_default();
    plan_with_path(root, m, argv0, &path)
}

pub fn plan_with_path(root: &Path, m: &HomemadeManifest, argv0: &str, path: &str) -> Plan {
    let manager = m.app.env_manager.trim().to_string();
    let venv_bin = root.join(".venv").join("bin");
    let node_bin = root.join("node_modules").join(".bin");
    let mut p = Plan { manager: manager.clone(), ..Default::default() };
    match manager.as_str() {
        "uv" => {
            if root.join("pyproject.toml").exists() {
                p.install = vec![sv(&["uv", "sync"])];
                p.inputs = vec![root.join("pyproject.toml"), root.join("uv.lock")];
                p.env_dir = Some(root.join(".venv"));
                p.bins = vec![venv_bin];
                if argv0 != "uv" && !argv0.contains(".venv/") { p.wrap = sv(&["uv", "run"]); }
            } else if root.join("requirements.txt").exists() {
                p.install = vec![
                    sv(&["uv", "venv", "--allow-existing", ".venv"]),
                    sv(&["uv", "pip", "install", "-r", "requirements.txt", "--python", ".venv/bin/python"]),
                ];
                p.inputs = vec![root.join("requirements.txt")];
                p.env_dir = Some(root.join(".venv"));
                p.bins = vec![venv_bin];
            } else if venv_bin.is_dir() {
                p.bins = vec![venv_bin];
            }
        }
        "pnpm" => {
            if root.join("package.json").exists() {
                let has_pnpm = agents::which("pnpm", path).is_some();
                if !has_pnpm && agents::which("npm", path).is_some() {
                    p.install = vec![sv(&["npm", "install"])];
                    p.inputs = vec![root.join("package.json"), root.join("package-lock.json")];
                } else {
                    p.install = vec![sv(&["pnpm", "install"])];
                    p.inputs = vec![root.join("package.json"), root.join("pnpm-lock.yaml")];
                }
                p.env_dir = Some(root.join("node_modules"));
                p.bins = vec![node_bin];
            }
        }
        "byo" => {
            p.bins = [venv_bin, node_bin].into_iter().filter(|b| b.is_dir()).collect();
        }
        _ => {} // "cargo", "system": nothing to do
    }
    p
}

/// PATH with the plan's existing bin dirs prepended (deduplicated), and VIRTUAL_ENV when `.venv/bin` is active.
pub fn shell_path(root: &Path, m: &HomemadeManifest, path: &str) -> (String, Option<String>) {
    let p = plan_with_path(root, m, "", path);
    let mut parts: Vec<String> = vec![];
    let mut venv = None;
    for b in p.bins.iter().filter(|b| b.is_dir()) {
        let s = b.to_string_lossy().into_owned();
        if b.ends_with(".venv/bin") { venv = Some(root.join(".venv").to_string_lossy().into_owned()); }
        if !parts.contains(&s) { parts.push(s); }
    }
    for seg in path.split(':').filter(|s| !s.is_empty()) {
        if !parts.iter().any(|p| p == seg) { parts.push(seg.to_string()); }
    }
    (parts.join(":"), venv)
}

fn stamp_path(home: &Path, app_id: &str) -> PathBuf { home.join("deps").join(format!("{app_id}.stamp")) }

fn compute_stamp(plan: &Plan) -> String {
    let mut h = Sha256::new();
    h.update(plan.manager.as_bytes());
    for f in &plan.inputs {
        h.update(b"\0");
        h.update(f.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default().as_bytes());
        h.update(b"\0");
        if let Ok(b) = std::fs::read(f) { h.update(&b); }
    }
    hex::encode(h.finalize())
}

fn short_home(home: &Path) -> String {
    match dirs::home_dir() {
        Some(h) if home.starts_with(&h) => format!("~/{}", home.strip_prefix(&h).unwrap_or(home).display()),
        _ => home.display().to_string(),
    }
}

/// Make sure the app's dependencies are installed and its env is active, then wrap argv if needed.
/// Returns the `[homemade] deps:` lines that were emitted (so the caller can seed the run log).
pub async fn ensure(state: &Arc<AppState>, run_id: &str, app_id: &str, root: &Path, m: &HomemadeManifest,
                    env: &mut HashMap<String, String>, argv: &mut Vec<String>) -> Result<Vec<String>> {
    let path = env.get("PATH").cloned().unwrap_or_default();
    let plan = plan_with_path(root, m, argv.first().map(String::as_str).unwrap_or(""), &path);
    let mut lines = vec![];

    if !plan.install.is_empty() {
        let env_dir = plan.env_dir.clone().unwrap_or_else(|| root.join(".venv"));
        let stamp_file = stamp_path(&state.home, app_id);
        let stamp = compute_stamp(&plan);
        let prev = std::fs::read_to_string(&stamp_file).ok().map(|s| s.trim().to_string());
        let why = if !env_dir.is_dir() {
            Some(format!("{} missing", env_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()))
        } else if prev.as_deref() != Some(stamp.as_str()) {
            let changed: Vec<String> = plan.inputs.iter().filter(|f| f.exists())
                .filter_map(|f| f.file_name().map(|n| n.to_string_lossy().into_owned())).collect();
            Some(if prev.is_none() { "first run under Homemade".into() } else { format!("{} changed", changed.join("/")) })
        } else { None };
        if let Some(why) = why {
            let tool = &plan.install[0][0];
            if agents::which(tool, &path).is_none() {
                let hint = match tool.as_str() {
                    "uv" => "brew install uv",
                    "pnpm" => "npm i -g pnpm",
                    _ => "install it",
                };
                return err(format!("`{tool}` not found. Install it ({hint}) or set env_manager = \"system\" in homemade.toml"));
            }
            let store = match plan.manager.as_str() {
                "uv" => format!("shared store {}/store/uv_cache, identical packages are linked not downloaded", short_home(&state.home)),
                _ => format!("shared store {}/store/pnpm_store, identical packages are linked not downloaded", short_home(&state.home)),
            };
            let cmds: Vec<String> = plan.install.iter().map(|c| c.join(" ")).collect();
            emit_line(state, run_id, &mut lines, format!("{} ({why}; {store})", cmds.join(" && ")));
            for cmd in &plan.install {
                run_install(state, run_id, root, env, cmd, &mut lines).await?;
            }
            let _ = std::fs::create_dir_all(stamp_file.parent().unwrap_or(&state.home));
            let _ = std::fs::write(&stamp_file, &stamp);
            record_packages(state, app_id, root, &plan, &env_dir, env).await;
        }
    }
    // After the install the env dir exists, so its bin dir can go on PATH (and VIRTUAL_ENV be set).
    let (new_path, venv) = shell_path(root, m, &path);
    env.insert("PATH".into(), new_path);
    if let Some(v) = venv { env.insert("VIRTUAL_ENV".into(), v); }
    if !plan.wrap.is_empty() {
        let mut v = plan.wrap.clone();
        v.append(argv);
        *argv = v;
    }
    Ok(lines)
}

fn emit_line(state: &AppState, run_id: &str, lines: &mut Vec<String>, text: String) {
    let line = format!("[homemade] deps: {text}");
    state.emit("run://stdout", serde_json::json!({ "runId": run_id, "line": line }));
    lines.push(line);
}

/// Run one install command in its own process group, streaming merged stdout/stderr as run://stdout lines.
async fn run_install(state: &Arc<AppState>, run_id: &str, root: &Path, env: &HashMap<String, String>, argv: &[String], lines: &mut Vec<String>) -> Result<()> {
    let mut cmd = tokio::process::Command::new(&argv[0]);
    cmd.args(&argv[1..]).current_dir(root).env_clear().envs(env)
        .stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped())
        .kill_on_drop(false);
    unsafe { cmd.pre_exec(|| { nix::unistd::setsid().map_err(std::io::Error::from)?; Ok(()) }); }
    let mut child = cmd.spawn().map_err(|e| crate::error::Error::Msg(format!("{}: {e}", argv[0])))?;
    let pid = child.id().map(|p| p as i32).unwrap_or(0);
    state.installing.insert(run_id.to_string(), pid);
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    let mut tasks = vec![];
    if let Some(out) = child.stdout.take() {
        let tx = tx.clone();
        tasks.push(tokio::spawn(async move { let mut l = BufReader::new(out).lines(); while let Ok(Some(x)) = l.next_line().await { let _ = tx.send(x); } }));
    }
    if let Some(errs) = child.stderr.take() {
        let tx = tx.clone();
        tasks.push(tokio::spawn(async move { let mut l = BufReader::new(errs).lines(); while let Ok(Some(x)) = l.next_line().await { let _ = tx.send(x); } }));
    }
    drop(tx);
    let mut tail: Vec<String> = vec![];
    while let Some(l) = rx.recv().await {
        if l.trim().is_empty() { continue; }
        tail.push(l.clone());
        if tail.len() > 3 { tail.remove(0); }
        emit_line(state, run_id, lines, l);
    }
    for t in tasks { let _ = t.await; }
    let status = child.wait().await;
    // `stop_run` removes the entry before killing the group: then the run was cancelled, not broken.
    let stopped = state.installing.remove(run_id).is_none();
    if stopped {
        {
            let c = state.db.lock().unwrap();
            let _ = queries::finish_run(&c, run_id, "killed", None, 0, 0, crate::types::now_ms(), &lines.join("\n"));
        }
        if let Ok(Some(r)) = { let c = state.db.lock().unwrap(); queries::get_run(&c, run_id) } { state.emit("run://status", r); }
        return err("dependency install stopped");
    }
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => err(format!("dependency install failed (exit {}): {}", s.code().map(|c| c.to_string()).unwrap_or_else(|| "signal".into()), tail.join(" | "))),
        Err(e) => err(format!("dependency install failed: {e}")),
    }
}

/// Record package + env rows for the app after a successful install.
async fn record_packages(state: &AppState, app_id: &str, root: &Path, plan: &Plan, env_dir: &Path, env: &HashMap<String, String>) {
    let mut pkgs: Vec<(String, i64)> = vec![];
    match plan.manager.as_str() {
        "uv" => {
            let py = env_dir.join("bin").join("python").to_string_lossy().into_owned();
            if let Ok(out) = tokio::process::Command::new("uv").args(["pip", "list", "--format=json", "--python", &py])
                .current_dir(root).env_clear().envs(env).stdin(std::process::Stdio::null()).output().await
            {
                if let Ok(v) = serde_json::from_slice::<Vec<serde_json::Value>>(&out.stdout) {
                    for e in v {
                        if let (Some(n), Some(ver)) = (e.get("name").and_then(|x| x.as_str()), e.get("version").and_then(|x| x.as_str())) {
                            pkgs.push((format!("{n}@{ver}"), 0));
                        }
                    }
                }
            }
        }
        "pnpm" => {
            if let Ok(lock) = std::fs::read_to_string(root.join("pnpm-lock.yaml")) {
                pkgs = parse_pnpm_lock(&lock).into_iter().map(|k| (k, 0)).collect();
            }
        }
        _ => {}
    }
    let env_key = env_dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| ".venv".into());
    let bytes = venv_scanner::dir_size(env_dir) as i64;
    let c = state.db.lock().unwrap();
    let _ = queries::replace_deps(&c, app_id, "package", &pkgs, "homemade");
    let _ = queries::upsert_dep(&c, app_id, "env", &env_key, bytes, "homemade", false);
}

/// `name@version` keys from the `packages:` section of a pnpm-lock.yaml (v6 `/name@version:` and v9 `name@version:` forms).
pub fn parse_pnpm_lock(lock: &str) -> Vec<String> {
    let mut out = vec![];
    let mut in_packages = false;
    for line in lock.lines() {
        if line.trim().is_empty() { continue; }
        if !line.starts_with(' ') && !line.starts_with('\t') {
            in_packages = line.trim_end() == "packages:";
            continue;
        }
        if !in_packages { continue; }
        // Entries are exactly one indent level deep and end with ':'.
        let indent = line.len() - line.trim_start().len();
        if indent != 2 { continue; }
        let mut key = line.trim().trim_end_matches(':').trim_matches(|c| c == '\'' || c == '"').to_string();
        if let Some(i) = key.find('(') { key.truncate(i); }
        let key = key.trim_start_matches('/').trim().to_string();
        if key.contains('@') && !out.contains(&key) { out.push(key); }
    }
    out
}

/// Relative paths of every entry up to depth 3 under <home>/store/shared_cache.
pub fn cache_snapshot(home: &Path) -> Vec<String> {
    let base = home.join("store").join("shared_cache");
    walkdir::WalkDir::new(&base).min_depth(1).max_depth(3).follow_links(false).into_iter()
        .filter_map(|e| e.ok())
        .filter_map(|e| e.path().strip_prefix(&base).ok().map(|p| p.to_string_lossy().into_owned()))
        .collect()
}

/// Attribute shared_cache entries that appeared since `before` to the app (top-most new entries only).
pub fn record_new_cache(state: &AppState, app_id: &str, before: &[String]) {
    let now = cache_snapshot(&state.home);
    let mut new: Vec<&String> = now.iter().filter(|p| !before.contains(p)).collect();
    new.sort();
    let mut top: Vec<String> = vec![];
    for p in new {
        if top.iter().any(|t| p.starts_with(&format!("{t}/"))) { continue; }
        top.push(p.clone());
    }
    if top.is_empty() { return; }
    let base = state.home.join("store").join("shared_cache");
    let rows: Vec<(String, i64)> = top.into_iter().map(|k| { let b = venv_scanner::dir_size(&base.join(&k)) as i64; (k, b) }).collect();
    let c = state.db.lock().unwrap();
    for (k, b) in rows { let _ = queries::upsert_dep(&c, app_id, "model", &k, b, "homemade", true); }
}

/// Abort an install in flight for `run_id` (SIGTERM to its group, SIGKILL after ~3 s). Returns true if one was running.
pub async fn stop_install(state: &Arc<AppState>, run_id: &str) -> bool {
    let Some((_, pid)) = state.installing.remove(run_id) else { return false };
    if pid <= 1 { return true; }
    unsafe { libc::kill(-pid, libc::SIGTERM); }
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let alive = unsafe { libc::kill(-pid, 0) } == 0;
        if alive { unsafe { libc::kill(-pid, libc::SIGKILL); } }
    });
    true
}

#[cfg(test)]
mod tests {
    use super::parse_pnpm_lock;

    #[test]
    fn parses_v6_and_v9_lock_keys() {
        let v6 = "lockfileVersion: '6.0'\n\npackages:\n\n  /react@18.3.1:\n    resolution: {}\n\n  /foo@1.0.0(react@18.3.1):\n    dev: true\n";
        assert_eq!(parse_pnpm_lock(v6), vec!["react@18.3.1", "foo@1.0.0"]);
        let v9 = "lockfileVersion: '9.0'\n\nimporters:\n  .:\n    dependencies:\n      react:\n        specifier: ^18\n\npackages:\n\n  '@types/node@20.1.0':\n    resolution: {}\n\n  react@18.3.1:\n    resolution: {}\n\nsnapshots:\n\n  react@18.3.1: {}\n";
        assert_eq!(parse_pnpm_lock(v9), vec!["@types/node@20.1.0", "react@18.3.1"]);
    }
}
