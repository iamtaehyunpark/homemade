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
