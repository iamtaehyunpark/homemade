//! Unmanaged .venv detection and 1-click migration (spec §8B). OWNER: agent C.
use crate::error::{err, Result};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

fn find_venv(root: &Path) -> Option<PathBuf> {
    [".venv", "venv"].iter().map(|d| root.join(d)).find(|p| p.join("pyvenv.cfg").exists() || p.join("bin").is_dir())
}

/// Physical bytes of a tree: st_blocks*512, deduped by (dev, ino) so hardlinks count once.
fn physical_size(path: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    let mut seen = HashSet::new();
    walkdir::WalkDir::new(path).follow_links(false).into_iter().filter_map(|e| e.ok())
        .filter_map(|e| e.metadata().ok())
        .filter(|m| !m.is_dir() && seen.insert((m.dev(), m.ino())))
        .map(|m| m.blocks() * 512).sum()
}

/// Physical bytes of `<root>/.venv` or `<root>/venv` (0 if none). Count st_blocks*512, dedupe by inode.
pub fn scan_unmanaged_venv(root: &Path) -> u64 {
    find_venv(root).map(|v| physical_size(&v)).unwrap_or(0)
}

fn which(name: &str, env: &HashMap<String, String>) -> Option<PathBuf> {
    let path = env.get("PATH").cloned().or_else(|| std::env::var("PATH").ok())?;
    std::env::split_paths(&path).map(|d| d.join(name)).find(|p| p.is_file())
}

async fn run_logged(state: &crate::state::AppState, app_id: &str, cwd: &Path, env: &HashMap<String, String>, prog: &Path, args: &[&str], stdout_to: Option<&Path>) -> Result<()> {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let progress = |m: String| state.emit("storage://progress", serde_json::json!({"appId": app_id, "message": m}));
    progress(format!("$ {} {}", prog.display(), args.join(" ")));
    let mut cmd = tokio::process::Command::new(prog);
    cmd.args(args).current_dir(cwd).env_clear().envs(env).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::piped());
    match stdout_to {
        Some(f) => { cmd.stdout(std::fs::File::create(f)?); }
        None => { cmd.stdout(std::process::Stdio::piped()); }
    }
    let mut child = cmd.spawn()?;
    let out = child.stdout.take();
    let errs = child.stderr.take();
    let pump = |r: Option<Box<dyn tokio::io::AsyncRead + Unpin + Send>>| async move {
        let mut v = Vec::new();
        if let Some(r) = r { let mut l = BufReader::new(r).lines(); while let Ok(Some(x)) = l.next_line().await { v.push(x); } }
        v
    };
    let o: Option<Box<dyn tokio::io::AsyncRead + Unpin + Send>> = out.map(|x| Box::new(x) as _);
    let e: Option<Box<dyn tokio::io::AsyncRead + Unpin + Send>> = errs.map(|x| Box::new(x) as _);
    let (ol, el) = tokio::join!(pump(o), pump(e));
    for l in ol.into_iter().chain(el) { progress(l); }
    let status = child.wait().await?;
    if !status.success() { return err(format!("{} failed ({status})", prog.display())); }
    Ok(())
}

/// Migration per spec: capture python version, `pip freeze > requirements.homemade.lock` (if no pyproject/requirements),
/// rm -rf .venv, `uv venv .venv --python X.Y`, `uv pip install -r <lock or requirements.txt>` with UV_CACHE_DIR/UV_LINK_MODE env.
/// Returns bytes after migration. Emit "storage://progress" {appId, message} lines while running.
pub async fn migrate_venv(root: &Path, homemade_home: &Path, app_id: &str, state: &crate::state::AppState) -> Result<u64> {
    let mut env = state.login_env.clone();
    super::env_router::inject_shared_storage_env(&mut env, homemade_home, true);
    env.remove("VIRTUAL_ENV");
    let Some(uv) = which("uv", &env) else { return err("uv not found") };
    let Some(venv) = find_venv(root) else { return err("No unmanaged .venv found") };
    let venv_name = venv.file_name().unwrap().to_string_lossy().into_owned();
    let py = venv.join("bin/python");
    let progress = |m: &str| state.emit("storage://progress", serde_json::json!({"appId": app_id, "message": m}));

    progress("Detecting Python version...");
    let ver = tokio::process::Command::new(&py).args(["-c", "import sys;print('%d.%d'%sys.version_info[:2])"]).output().await
        .ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let req = if root.join("requirements.txt").exists() { "requirements.txt".to_string() }
        else if root.join("pyproject.toml").exists() { "pyproject.toml".to_string() }
        else {
            progress("Freezing dependencies to requirements.homemade.lock...");
            let lock = root.join("requirements.homemade.lock");
            // Prefer pip inside the venv; fall back to uv pip freeze against it.
            if run_logged(state, app_id, root, &env, &py, &["-m", "pip", "freeze"], Some(&lock)).await.is_err() {
                let pystr = py.to_string_lossy().into_owned();
                run_logged(state, app_id, root, &env, &uv, &["pip", "freeze", "--python", &pystr], Some(&lock)).await?;
            }
            "requirements.homemade.lock".to_string()
        };

    progress(&format!("Removing {venv_name}..."));
    tokio::fs::remove_dir_all(&venv).await?;

    let mut args = vec!["venv", venv_name.as_str()];
    if !ver.is_empty() { args.extend(["--python", ver.as_str()]); }
    run_logged(state, app_id, root, &env, &uv, &args, None).await?;
    let pystr = venv.join("bin/python").to_string_lossy().into_owned();
    run_logged(state, app_id, root, &env, &uv, &["pip", "install", "--python", &pystr, "-r", &req], None).await?;

    let bytes = scan_unmanaged_venv(root);
    progress(&format!("Done. {venv_name} now uses {} MB on disk.", bytes / (1024 * 1024)));
    Ok(bytes)
}

/// Sizes of the store subdirectories for StorageSummary.
pub fn dir_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path).into_iter().filter_map(|e| e.ok()).filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum()
}
