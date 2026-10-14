//! Blocking client for the control socket (used by the CLI and the MCP server, which are plain processes).
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

fn home() -> PathBuf { crate::storage::config::homemade_home() }

/// True when a GUI instance is serving the socket.
pub fn available() -> bool { call_with_timeout("status", &json!({}), Some(Duration::from_secs(3))).is_ok() }

pub fn call(method: &str, params: &Value) -> Result<Value, String> {
    // wait_run may legitimately take minutes; everything else should answer fast.
    let t = if method == "wait_run" { None } else { Some(Duration::from_secs(120)) };
    call_with_timeout(method, params, t)
}

pub fn call_with_timeout(method: &str, params: &Value, timeout: Option<Duration>) -> Result<Value, String> {
    let h = home();
    let info: Value = std::fs::read_to_string(super::token_path(&h)).ok().and_then(|t| serde_json::from_str(&t).ok()).ok_or_else(|| not_running(&h))?;
    let token = info["token"].as_str().unwrap_or("").to_string();
    let sock = info["socket"].as_str().map(PathBuf::from).unwrap_or_else(|| super::socket_path(&h));
    let mut st = UnixStream::connect(sock).map_err(|_| not_running(&h))?;
    st.set_read_timeout(timeout).ok();
    st.set_write_timeout(Some(Duration::from_secs(10))).ok();
    let req = json!({ "id": 1, "token": token, "method": method, "params": params });
    st.write_all(format!("{req}\n").as_bytes()).map_err(|e| e.to_string())?;
    let mut line = String::new();
    BufReader::new(st).read_line(&mut line).map_err(|e| format!("control socket: {e}"))?;
    if line.is_empty() { return Err("control socket closed (is Homemade still running?)".into()); }
    let v: Value = serde_json::from_str(&line).map_err(|e| format!("bad response: {e}"))?;
    if let Some(err) = v.get("error").and_then(|e| e.as_str()) { return Err(err.to_string()); }
    Ok(v.get("result").cloned().unwrap_or(Value::Null))
}

fn not_running(h: &std::path::Path) -> String {
    format!("Homemade is not running (no control socket in {}). Launch it: `homemade launch` or `open -a Homemade`.", h.display())
}

/// Launch the GUI (macOS: the .app bundle this binary lives in, else the binary itself, detached) and wait for the socket.
pub fn launch_gui(wait: Duration) -> Result<(), String> {
    if available() { return Ok(()); }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut launched = false;
    #[cfg(target_os = "macos")]
    {
        // <Bundle>.app/Contents/MacOS/homemade → open the bundle so it gets a proper app session.
        if let Some(bundle) = exe.ancestors().find(|p| p.extension().map(|x| x == "app").unwrap_or(false)) {
            launched = std::process::Command::new("open").arg(bundle).status().map(|s| s.success()).unwrap_or(false);
        }
    }
    if !launched {
        use std::process::Stdio;
        std::process::Command::new(&exe).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|e| e.to_string())?;
    }
    let deadline = std::time::Instant::now() + wait;
    while std::time::Instant::now() < deadline {
        if available() { return Ok(()); }
        std::thread::sleep(Duration::from_millis(300));
    }
    Err("Homemade did not come up in time".into())
}
