//! Login-env hydration and process-group spawning + run lifecycle. OWNER: agent A.
use crate::db::queries;
use crate::error::{err, Result};
use crate::state::{ActiveRun, AppState, PendingJob};
use crate::storage::{artifact_differ, env_router, secrets};
use crate::supervisor::{reaper, scheduler, stdout_parser};
use crate::types::{now_ms, RunRecord};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWriteExt, BufReader};

/// Spec §5A: run `$SHELL -l -c 'env -0'` with a 5s timeout; merge over std::env::vars();
/// force PYTHONUNBUFFERED=1, FORCE_COLOR=1, PYTHONIOENCODING=utf-8. Called once at startup.
pub fn load_user_login_env() -> HashMap<String, String> {
    let mut m: HashMap<String, String> = std::env::vars().collect();
    let shell = std::env::var("SHELL").unwrap_or_else(|_| {
        if cfg!(target_os = "macos") { "/bin/zsh".into() } else { "/bin/bash".into() }
    });
    let child = std::process::Command::new(&shell)
        .args(["-l", "-c", "env -0"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .spawn();
    if let Ok(child) = child {
        let pid = child.id() as i32;
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || { let _ = tx.send(child.wait_with_output()); });
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(out)) => {
                for entry in out.stdout.split(|&b| b == 0) {
                    if let Ok(s) = std::str::from_utf8(entry) {
                        if let Some((k, v)) = s.split_once('=') {
                            let k = k.trim_start_matches('\n');
                            if !k.is_empty() && k != "_" && k != "SHLVL" && k != "PWD" && k != "OLDPWD" {
                                m.insert(k.to_string(), v.to_string());
                            }
                        }
                    }
                }
            }
            _ => unsafe { libc::kill(pid, libc::SIGKILL); },
        }
    }
    m.insert("PYTHONUNBUFFERED".into(), "1".into());
    m.insert("FORCE_COLOR".into(), "1".into());
    m.insert("PYTHONIOENCODING".into(), "utf-8".into());
    // A GUI launch can have no locale, so zsh's line editor treats multibyte text bytewise.
    if !["LC_ALL", "LC_CTYPE", "LANG"].iter().any(|k| m.get(*k).is_some_and(|v| !v.is_empty())) {
        let apple_locale = if cfg!(target_os = "macos") {
            std::process::Command::new("defaults").args(["read", "-g", "AppleLocale"]).output().ok()
                .filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        } else { None };
        let available = std::process::Command::new("/usr/bin/locale").arg("-a").output().ok()
            .filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
        if let Some(lang) = default_utf8_locale(apple_locale.as_deref(), &available) {
            m.insert("LANG".into(), lang);
        }
    }
    m
}

/// AppleLocale is an ICU preference, not necessarily an installed POSIX locale: en_KR is one
/// example. An unsupported LANG silently leaves zsh using ASCII despite its .UTF-8 suffix.
fn default_utf8_locale(apple_locale: Option<&str>, available: &str) -> Option<String> {
    let locales: Vec<&str> = available.lines().map(str::trim).filter(|l| {
        l.split('@').next().unwrap_or(l).replace('-', "").to_ascii_lowercase().ends_with(".utf8")
    }).collect();
    let preferred = apple_locale.map(|l| format!("{}.UTF-8", l.split(['@', '.']).next().unwrap_or(l)));
    for candidate in preferred.as_deref().into_iter().chain(["en_US.UTF-8", "C.UTF-8"]) {
        let normalized = candidate.replace('-', "").to_ascii_lowercase();
        if let Some(locale) = locales.iter().find(|l| l.replace('-', "").to_ascii_lowercase() == normalized) {
            return Some((*locale).to_string());
        }
    }
    locales.first().map(|l| (*l).to_string())
}

#[cfg(test)]
mod locale_tests {
    use super::default_utf8_locale;

    #[test]
    fn unsupported_apple_region_uses_an_installed_utf8_locale() {
        let available = "C\nPOSIX\nen_US.UTF-8\nko_KR.UTF-8\n";
        assert_eq!(default_utf8_locale(Some("en_KR"), available).as_deref(), Some("en_US.UTF-8"));
        assert_eq!(default_utf8_locale(Some("ko_KR@calendar=gregorian"), available).as_deref(), Some("ko_KR.UTF-8"));
    }

    #[test]
    fn fallback_accepts_system_utf8_spelling_and_never_invents_a_locale() {
        assert_eq!(default_utf8_locale(None, "C\nC.utf8\n").as_deref(), Some("C.utf8"));
        assert_eq!(default_utf8_locale(Some("en_KR"), "C\nko_KR.UTF-8\n").as_deref(), Some("ko_KR.UTF-8"));
        assert_eq!(default_utf8_locale(None, "C\nPOSIX\n"), None);
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn fallback_makes_zsh_count_korean_characters_instead_of_utf8_bytes() {
        let available = std::process::Command::new("/usr/bin/locale").arg("-a").output().unwrap();
        assert!(available.status.success());
        let lang = default_utf8_locale(Some("en_KR"), &String::from_utf8(available.stdout).unwrap()).unwrap();
        let output = std::process::Command::new("/bin/zsh")
            .env_clear().env("LANG", lang).args(["-df", "-c", "text='한글'; print -r -- ${#text}"])
            .output().unwrap();
        assert!(output.status.success());
        assert_eq!(String::from_utf8(output.stdout).unwrap().trim(), "2");
    }
}

/// Boxed as `dyn Future + Send` to break the launch -> scheduler -> launch auto-trait cycle.
fn notify_scheduler(state: Arc<AppState>) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send>> {
    Box::pin(scheduler::on_run_finished(state))
}

fn get_run_record(state: &AppState, run_id: &str) -> Option<RunRecord> {
    let c = state.db.lock().unwrap();
    queries::get_run(&c, run_id).ok().flatten()
}

fn emit_status(state: &AppState, run_id: &str) {
    if let Some(r) = get_run_record(state, run_id) { state.emit("run://status", r); }
}

/// Mark a run that never spawned as failed with a message in its log_tail.
fn fail_unspawned(state: &AppState, run_id: &str, msg: &str) {
    {
        let c = state.db.lock().unwrap();
        let _ = queries::finish_run(&c, run_id, "failed", None, 0, 0, now_ms(), msg);
    }
    emit_status(state, run_id);
}

/// Spawn an admitted job (called by scheduler, never directly by commands). See CONTRACT "Run lifecycle".
pub async fn launch(state: Arc<AppState>, job: PendingJob) -> Result<()> {
    match launch_inner(state.clone(), job.clone()).await {
        Ok(()) => Ok(()),
        Err(e) => {
            fail_unspawned(&state, &job.run_id, &format!("Failed to start: {e}"));
            notify_scheduler(state).await;
            Err(e)
        }
    }
}

async fn launch_inner(state: Arc<AppState>, job: PendingJob) -> Result<()> {
    if job.argv.is_empty() { return err("Empty command (manifest app.exec)"); }
    let m = &job.manifest;
    let root = job.root_dir.clone();
    let cwd = root.join(&m.app.cwd);
    let cfg_table = crate::storage::config::load(&state.home);
    let aliases = crate::storage::config::get_table(&cfg_table, "aliases");
    let mut artifacts_cfg = m.artifacts.clone();
    artifacts_cfg.output_dir = env_router::expand_vars(&artifacts_cfg.output_dir, &aliases, &state.login_env);
    let out_dir = {
        let p = PathBuf::from(&artifacts_cfg.output_dir);
        if p.is_absolute() { p } else { root.join(p) }
    };
    let _ = std::fs::create_dir_all(&out_dir);
    let out_dir = std::fs::canonicalize(&out_dir).unwrap_or(out_dir);

    // ---- environment ----
    let mut env = state.login_env.clone();
    env_router::inject_shared_storage_env(&mut env, &state.home, m.resources.isolated_cache);
    for (k, v) in &m.environment.vars { env.insert(k.clone(), env_router::expand_vars(v, &aliases, &state.login_env)); }
    let secret_vals = secrets::resolve(&m.environment.secrets)?;
    for (k, v) in &secret_vals { env.insert(k.clone(), v.clone()); }
    let res_dir = crate::commands::resources_dir(&state.app_handle);
    let res_str = res_dir.to_string_lossy().to_string();
    env.insert("HOMEMADE_RESOURCES".into(), res_str.clone());
    let pp = match env.get("PYTHONPATH") {
        Some(old) if !old.is_empty() => format!("{res_str}:{old}"),
        _ => res_str,
    };
    env.insert("PYTHONPATH".into(), pp);
    env.insert("HOMEMADE_RUN_ID".into(), job.run_id.clone());
    env.insert("HOMEMADE_APP_ID".into(), job.app_id.clone());
    env.insert("HOMEMADE_ROOT".into(), root.to_string_lossy().to_string());
    env.insert("HOMEMADE_OUTPUT_DIR".into(), out_dir.to_string_lossy().to_string());
    // Programs can drive the launcher too (start sibling runs, read logs, …): `homemade` on PATH + pointers.
    let cur_path = env.get("PATH").cloned().unwrap_or_default();
    for (k, v) in crate::control::child_env(&state.home, &cur_path) { env.insert(k, v); }

    // ---- pre-run snapshot ----
    let snap_root = root.clone();
    let snapshot = tokio::task::spawn_blocking(move || artifact_differ::take(&snap_root))
        .await
        .map_err(|e| crate::error::Error::Msg(e.to_string()))?;

    // ---- spawn ----
    let mut cmd = tokio::process::Command::new(&job.argv[0]);
    cmd.args(&job.argv[1..])
        .current_dir(&cwd)
        .env_clear()
        .envs(&env)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(false);
    #[allow(unused_variables)]
    let max_ram_mb = m.resources.max_ram_mb;
    unsafe {
        cmd.pre_exec(move || {
            nix::unistd::setsid().map_err(std::io::Error::from)?;
            #[cfg(target_os = "linux")]
            {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                if let Some(mb) = max_ram_mb {
                    let bytes = (mb as libc::rlim_t).saturating_mul(1024 * 1024);
                    let lim = libc::rlimit { rlim_cur: bytes, rlim_max: bytes };
                    libc::setrlimit(libc::RLIMIT_AS, &lim);
                }
            }
            Ok(())
        });
    }
    let mut child = cmd.spawn().map_err(|e| crate::error::Error::Msg(format!("{}: {e}", job.argv[0])))?;
    let pid = child.id().map(|p| p as i32).unwrap_or(0);
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdin = child.stdin.take();

    let run = Arc::new(ActiveRun {
        run_id: job.run_id.clone(),
        app_id: job.app_id.clone(),
        pid,
        pgid: pid,
        requires_gpu: job.requires_gpu,
        started_at: now_ms(),
        graceful_timeout_ms: m.resources.graceful_timeout_ms,
        kill_timeout_ms: m.resources.kill_timeout_ms,
        log: std::sync::Mutex::new(VecDeque::new()),
        stdin: tokio::sync::Mutex::new(stdin),
        rpc_pending: Default::default(),
        worker_ready: AtomicBool::new(false),
        webapp_ready: AtomicBool::new(false),
        port: std::sync::Mutex::new(None),
        stop_requested: AtomicBool::new(false),
        peak_ram_mb: AtomicU64::new(0),
        peak_vram_mb: AtomicU64::new(0),
        root: root.clone(),
        mode: m.app.mode.as_str().to_string(),
        secrets: secret_vals,
        announced_artifacts: std::sync::Mutex::new(vec![]),
    });
    state.active_runs.insert(job.run_id.clone(), run.clone());
    {
        let c = state.db.lock().unwrap();
        let _ = queries::set_run_status(&c, &job.run_id, "running");
        let _ = queries::set_run_pgid(&c, &job.run_id, pid);
    }
    crate::supervisor::pty_manager::inject_notice(state.clone(), &job.app_id,
        format!("[homemade] Run {} of {} STARTED (pid {pid}): {}", &job.run_id[..8.min(job.run_id.len())], job.app_id, job.cli_command));
    emit_status(&state, &job.run_id);

    let t_out = stdout.map(|s| tauri::async_runtime::spawn(pump(state.clone(), run.clone(), s, false)));
    let t_err = stderr.map(|s| tauri::async_runtime::spawn(pump(state.clone(), run.clone(), s, true)));

    let isolated = m.resources.isolated_cache;
    tauri::async_runtime::spawn(async move {
        let status = child.wait().await;
        // Drain remaining output (bounded: grandchildren may keep pipes open).
        for t in [t_out, t_err].into_iter().flatten() {
            let _ = tokio::time::timeout(Duration::from_secs(3), t).await;
        }
        finalize(state, run, status.ok(), snapshot, artifacts_cfg, isolated).await;
    });
    Ok(())
}

async fn pump<R: AsyncRead + Unpin>(state: Arc<AppState>, run: Arc<ActiveRun>, r: R, is_stderr: bool) {
    let mut reader = BufReader::new(r);
    let mut buf = Vec::new();
    loop {
        buf.clear();
        match reader.read_until(b'\n', &mut buf).await {
            Ok(0) | Err(_) => break,
            Ok(_) => {
                let line = String::from_utf8_lossy(&buf);
                stdout_parser::handle_line(&state, &run, &line, is_stderr);
            }
        }
    }
}

async fn finalize(
    state: Arc<AppState>,
    run: Arc<ActiveRun>,
    status: Option<std::process::ExitStatus>,
    snapshot: artifact_differ::Snapshot,
    cfg: crate::manifest::schema::ArtifactConfig,
    isolated_cache: bool,
) {
    let code = status.and_then(|s| s.code()).map(|c| c as i64);
    let final_status = if run.stop_requested.load(Ordering::SeqCst) {
        "killed"
    } else if code == Some(0) {
        "success"
    } else {
        "failed"
    };

    // Post-run artifact diff.
    let already = run.announced_artifacts.lock().unwrap().clone();
    let (rid, aid) = (run.run_id.clone(), run.app_id.clone());
    let found = tokio::task::spawn_blocking(move || artifact_differ::diff(&snapshot, &cfg, &rid, &aid, &already))
        .await
        .unwrap_or_default();
    let already = run.announced_artifacts.lock().unwrap().clone();
    let found: Vec<_> = found.into_iter().filter(|a| !already.contains(&a.file_path)).collect();
    {
        let c = state.db.lock().unwrap();
        for a in &found { let _ = queries::insert_artifact(&c, a); }
    }
    for a in found { state.emit("run://artifact", a); }

    // storage_refs: shared_cache entries touched during this run.
    if !isolated_cache {
        let (home, since) = (state.home.clone(), run.started_at);
        let refs = tokio::task::spawn_blocking(move || env_router::scan_shared_cache_changes(&home, since))
            .await
            .unwrap_or_default();
        if !refs.is_empty() {
            let c = state.db.lock().unwrap();
            for (hash, path, size) in refs {
                let _ = queries::upsert_storage_ref(&c, &crate::types::StorageRef {
                    asset_hash: hash, asset_path: path, app_id: run.app_id.clone(), size_bytes: size,
                });
            }
        }
    }

    // Fail any pending RPC waiters.
    let keys: Vec<String> = run.rpc_pending.iter().map(|e| e.key().clone()).collect();
    for k in keys {
        if let Some((_, tx)) = run.rpc_pending.remove(&k) {
            let _ = tx.send(serde_json::json!({"id": k, "ok": false, "error": "worker exited"}));
        }
    }

    let tail = secrets::redact(&stdout_parser::tail(&run, 200), &run.secrets);
    let ram = run.peak_ram_mb.load(Ordering::Relaxed) as i64;
    let vram = run.peak_vram_mb.load(Ordering::Relaxed) as i64;
    let now = now_ms();
    {
        let c = state.db.lock().unwrap();
        let _ = queries::finish_run(&c, &run.run_id, final_status, code, ram, vram, now, &tail);
        if !run.secrets.is_empty() {
            if let Ok(Some(r)) = queries::get_run(&c, &run.run_id) {
                let red = secrets::redact(&r.cli_command, &run.secrets);
                if red != r.cli_command {
                    let _ = c.execute("UPDATE runs SET cli_command = ?2 WHERE id = ?1", rusqlite::params![run.run_id, red]);
                }
            }
        }
        let _ = queries::update_app_learned_peaks(&c, &run.app_id, ram, vram, now);
    }
    state.active_runs.remove(&run.run_id);
    emit_status(&state, &run.run_id);
    // Tell any agent working on this app what just happened (it may then fix + re-run via MCP/CLI).
    {
        let secs = (now - run.started_at).max(0) as f64 / 1000.0;
        let short = &run.run_id[..8.min(run.run_id.len())];
        let notice = if final_status == "success" {
            let arts = { let c = state.db.lock().unwrap(); queries::list_artifacts(&c, &run.app_id, Some(&run.run_id), Some("output")).unwrap_or_default() };
            let names: Vec<String> = arts.iter().take(5).map(|a| std::path::Path::new(&a.file_path).file_name().map(|f| f.to_string_lossy().into_owned()).unwrap_or_default()).collect();
            format!("[homemade] Run {short} of {} SUCCEEDED in {secs:.1}s{}. (get_run_log(\"{}\") for the full log.)", run.app_id,
                if arts.is_empty() { String::new() } else { format!("; {} output{}: {}", arts.len(), if arts.len() == 1 { "" } else { "s" }, names.join(", ")) }, run.run_id)
        } else {
            let last: Vec<&str> = tail.lines().rev().filter(|l| !l.trim().is_empty()).take(4).collect();
            let last: Vec<&str> = last.into_iter().rev().collect();
            format!("[homemade] Run {short} of {} {} after {secs:.1}s (exit {}). Last lines: {}. Fix it, then verify with start_run + wait_run.", run.app_id,
                final_status.to_uppercase(), code.map(|c| c.to_string()).unwrap_or_else(|| "?".into()), last.join(" | "))
        };
        crate::supervisor::pty_manager::inject_notice(state.clone(), &run.app_id, notice);
    }
    if final_status == "failed" { spawn_health_check(state.clone(), &run); }
    notify_scheduler(state).await;
}

/// Failed run: ask the assistor (local model) for a diagnosis in the background; never blocks the finish path.
fn spawn_health_check(state: Arc<AppState>, run: &Arc<ActiveRun>) {
    let lines: Vec<String> = {
        let log = run.log.lock().unwrap();
        log.iter().skip(log.len().saturating_sub(120)).map(|l| secrets::redact(l, &run.secrets)).collect()
    };
    let run_id = run.run_id.clone();
    let app_id = run.app_id.clone();
    tauri::async_runtime::spawn(async move {
        let Some(rep) = state.assistor.analyze_log_window(&lines).await else { return };
        {
            let c = state.db.lock().unwrap();
            let _ = c.execute("UPDATE runs SET log_tail = COALESCE(log_tail, '') || ?2 WHERE id = ?1",
                rusqlite::params![run_id, format!("\n[homemade] {}", rep.message)]);
        }
        state.emit("run://health", serde_json::json!({ "runId": run_id, "severity": rep.severity, "message": rep.message }));
        emit_status(&state, &run_id);
        crate::supervisor::pty_manager::inject_notice(state.clone(), &app_id, format!("[homemade] Diagnosis for run {}: {}", &run_id[..8.min(run_id.len())], rep.message));
    });
}

/// Begin stopping an active run; returns the reaper future's inputs.
async fn request_stop(state: &AppState, run: &Arc<ActiveRun>) {
    run.stop_requested.store(true, Ordering::SeqCst);
    {
        let c = state.db.lock().unwrap();
        let _ = queries::set_run_status(&c, &run.run_id, "stopping");
    }
    emit_status(state, &run.run_id);
    if run.mode == "rpc_worker" {
        let mut g = run.stdin.lock().await;
        if let Some(stdin) = g.as_mut() {
            let _ = stdin.write_all(b"{\"method\":\"__shutdown__\"}\n").await;
            let _ = stdin.flush().await;
        }
        *g = None;
    }
}

/// Stop: set stop_requested, DB status 'stopping' + emit run://status, then reaper::terminate_process_group.
/// If the run is still queued (not in active_runs), remove it from vram_queue and finish as 'killed'.
pub async fn stop_run(state: Arc<AppState>, run_id: &str) -> Result<()> {
    let active = state.active_runs.get(run_id).map(|r| r.value().clone());
    if let Some(run) = active {
        request_stop(&state, &run).await;
        let (pgid, g, k) = (run.pgid, run.graceful_timeout_ms, run.kill_timeout_ms);
        if pgid > 0 {
            tauri::async_runtime::spawn(async move { reaper::terminate_process_group(pgid, g, k).await; });
        }
        return Ok(());
    }
    if scheduler::remove_queued(&state, run_id).await.is_some() {
        {
            let c = state.db.lock().unwrap();
            queries::finish_run(&c, run_id, "killed", None, 0, 0, now_ms(), "")?;
        }
        emit_status(&state, run_id);
        return Ok(());
    }
    // Neither active nor queued: if the DB still says it's live (e.g. stale), mark killed.
    let rec = get_run_record(&state, run_id);
    if let Some(r) = rec {
        if matches!(r.status.as_str(), "running" | "queued_vram" | "stopping") {
            {
                let c = state.db.lock().unwrap();
                queries::finish_run(&c, run_id, "killed", None, 0, 0, now_ms(), r.log_tail.as_deref().unwrap_or(""))?;
            }
            emit_status(&state, run_id);
        }
        Ok(())
    } else {
        err(format!("Unknown run: {run_id}"))
    }
}

/// rpc_worker mode: write `{"id","method","params"}\n` to child stdin, await the matching
/// `@homemade:rpc_response` via ActiveRun.rpc_pending (30s timeout => Err). Err if worker not ready.
pub async fn rpc_call(state: Arc<AppState>, run_id: &str, method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
    let run = match state.active_runs.get(run_id) {
        Some(r) => r.value().clone(),
        None => return err(format!("Run {run_id} is not active")),
    };
    if !run.worker_ready.load(Ordering::SeqCst) { return err("Worker is not ready yet"); }
    let id = uuid::Uuid::new_v4().to_string();
    let (tx, rx) = tokio::sync::oneshot::channel();
    run.rpc_pending.insert(id.clone(), tx);
    let mut line = serde_json::to_string(&serde_json::json!({"id": id, "method": method, "params": params}))?;
    line.push('\n');
    {
        let mut g = run.stdin.lock().await;
        let Some(stdin) = g.as_mut() else {
            run.rpc_pending.remove(&id);
            return err("Worker stdin is closed");
        };
        if let Err(e) = async { stdin.write_all(line.as_bytes()).await?; stdin.flush().await }.await {
            run.rpc_pending.remove(&id);
            return Err(e.into());
        }
    }
    match tokio::time::timeout(Duration::from_secs(30), rx).await {
        Ok(Ok(v)) => {
            if v.get("ok").and_then(|o| o.as_bool()).unwrap_or(false) {
                Ok(v.get("result").cloned().unwrap_or(serde_json::Value::Null))
            } else {
                let msg = v.get("error").and_then(|e| e.as_str()).unwrap_or("RPC failed").to_string();
                err(msg)
            }
        }
        Ok(Err(_)) => err("Worker exited before responding"),
        Err(_) => {
            run.rpc_pending.remove(&id);
            err(format!("RPC '{method}' timed out after 30s"))
        }
    }
}

/// Stop every active run and PTY (used by the quit flow). Waits for the reapers to finish.
pub async fn stop_all(state: Arc<AppState>) {
    let runs: Vec<Arc<ActiveRun>> = state.active_runs.iter().map(|e| e.value().clone()).collect();
    let mut handles = vec![];
    for run in runs {
        request_stop(&state, &run).await;
        let (pgid, g, k) = (run.pgid, run.graceful_timeout_ms, run.kill_timeout_ms);
        if pgid > 0 {
            handles.push(tauri::async_runtime::spawn(async move { reaper::terminate_process_group(pgid, g, k).await; }));
        }
    }
    let queued: Vec<String> = state.vram_queue.lock().await.iter().map(|j| j.run_id.clone()).collect();
    for id in queued { let _ = stop_run(state.clone(), &id).await; }
    let ptys: Vec<String> = state.active_ptys.iter().map(|e| e.key().clone()).collect();
    for id in ptys { let _ = crate::supervisor::pty_manager::kill(&state, &id); }
    for h in handles { let _ = h.await; }
}
