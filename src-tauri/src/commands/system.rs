//! Owned by the skeleton; agents may extend.
use super::St;
use crate::error::Result;

#[tauri::command]
pub async fn active_count(state: St<'_>) -> Result<usize> { Ok(state.active_count()) }

/// Stop everything then exit the process.
#[tauri::command]
pub async fn confirm_quit(state: St<'_>, app: tauri::AppHandle) -> Result<()> {
    crate::supervisor::spawner::stop_all(state.inner().clone()).await;
    app.exit(0);
    Ok(())
}

#[tauri::command]
pub async fn reveal_in_finder(path: String) -> Result<()> {
    #[cfg(target_os = "macos")]
    { std::process::Command::new("open").arg("-R").arg(&path).spawn()?; }
    #[cfg(target_os = "linux")]
    { std::process::Command::new("xdg-open").arg(std::path::Path::new(&path).parent().unwrap_or(std::path::Path::new("/"))).spawn()?; }
    Ok(())
}

#[tauri::command]
pub async fn homemade_home(state: St<'_>) -> Result<String> { Ok(state.home.to_string_lossy().into()) }

/// Read a small text file (used for manifest editor / log viewers). Max 2MB.
#[tauri::command]
pub async fn read_text_file(path: String) -> Result<String> {
    let meta = std::fs::metadata(&path)?;
    if meta.len() > 2 * 1024 * 1024 { return crate::error::err("file too large"); }
    Ok(std::fs::read_to_string(&path)?)
}

/// E2E harness: print a line to stderr.
#[tauri::command]
pub async fn e2e_report(msg: String) -> Result<()> { eprintln!("[e2e] {msg}"); Ok(()) }
/// E2E harness: stop everything and exit with a code.
#[tauri::command]
pub async fn e2e_done(state: St<'_>, code: i32) -> Result<()> {
    crate::supervisor::spawner::stop_all(state.inner().clone()).await;
    eprintln!("[e2e] done code={code}");
    state.app_handle.exit(code);
    Ok(())
}

#[derive(serde::Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ListeningPort {
    pub port: u16,
    pub pid: i32,
    pub command: String,
    /// Set when the process belongs to a process group Homemade launched (this or a previous session).
    pub run_id: Option<String>,
    pub app_id: Option<String>,
    /// True when that run is active right now (so it is expected, not an orphan).
    pub active: bool,
}

/// Every TCP port in LISTEN state on this machine (via `lsof -nP -iTCP -sTCP:LISTEN`), attributed to
/// Homemade runs by process group. Lets the Monitor answer "who is holding port 8765?".
#[tauri::command]
pub async fn list_listening_ports(state: St<'_>) -> Result<Vec<ListeningPort>> {
    let out = tokio::process::Command::new("lsof").args(["-nP", "-iTCP", "-sTCP:LISTEN", "-F", "pcn"]).output().await?;
    let text = String::from_utf8_lossy(&out.stdout);
    let known: Vec<(i32, String, String)> = { let c = state.db.lock().unwrap(); crate::db::queries::list_run_pgids(&c).unwrap_or_default() };
    let active: std::collections::HashSet<String> = state.active_runs.iter().map(|e| e.key().clone()).collect();
    let (mut pid, mut cmd) = (0i32, String::new());
    let mut seen = std::collections::HashSet::new();
    let mut res = vec![];
    for line in text.lines() {
        let (k, v) = line.split_at(1);
        match k {
            "p" => { pid = v.parse().unwrap_or(0); cmd.clear(); }
            "c" => cmd = v.to_string(),
            "n" => {
                let Some(port) = v.rsplit(':').next().and_then(|p| p.parse::<u16>().ok()) else { continue };
                if !seen.insert((pid, port)) { continue; }
                let pg = unsafe { libc::getpgid(pid) };
                let hit = known.iter().find(|(g, _, _)| *g == pg || *g == pid);
                res.push(ListeningPort {
                    port, pid, command: cmd.clone(),
                    run_id: hit.map(|h| h.1.clone()), app_id: hit.map(|h| h.2.clone()),
                    active: hit.map(|h| active.contains(&h.1)).unwrap_or(false),
                });
            }
            _ => {}
        }
    }
    res.sort_by_key(|p| p.port);
    Ok(res)
}

/// Kill the process group holding a port, but only if Homemade launched it (its pgid matches a recorded run).
#[tauri::command]
pub async fn kill_port_owner(state: St<'_>, pid: i32) -> Result<()> {
    let pg = unsafe { libc::getpgid(pid) };
    let known = { let c = state.db.lock().unwrap(); crate::db::queries::list_run_pgids(&c).unwrap_or_default() };
    let Some((_, run_id, _)) = known.into_iter().find(|(g, _, _)| *g == pg || *g == pid) else {
        return crate::error::err(format!("Process {pid} was not started by Homemade; stop it yourself (e.g. `kill {pid}`)."));
    };
    if state.active_runs.contains_key(&run_id) {
        return crate::supervisor::spawner::stop_run(state.inner().clone(), &run_id).await;
    }
    crate::supervisor::reaper::terminate_process_group(if pg > 1 { pg } else { pid }, 1000, 2000).await;
    Ok(())
}
