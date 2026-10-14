//! `homemade --mcp`: stdio MCP server. When the GUI is running every tool is forwarded to its control socket
//! (full control: import, run, stop, wait, open windows, …); otherwise the read-only tools answer from the DB
//! (spec §10D) and the write tools say how to launch the GUI. OWNER: agent G2.
//! Newline-delimited JSON-RPC 2.0 on stdin/stdout. Nothing but responses is written to stdout.
use crate::db::queries;
use crate::error::Result;
use crate::manifest::schema::HomemadeManifest;
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

const PROTOCOL_VERSION: &str = "2024-11-05";

/// Resources dir + local model for the CLI process (built lazily on the first draft_manifest call).
static MODEL: std::sync::OnceLock<(PathBuf, crate::intelligence::local_model::LocalModel)> = std::sync::OnceLock::new();

fn model() -> &'static (PathBuf, crate::intelligence::local_model::LocalModel) {
    MODEL.get_or_init(|| {
        let res = crate::commands::resources_dir_from_exe();
        let cfg = crate::storage::config::load(&crate::storage::config::homemade_home());
        let m = crate::intelligence::local_model::LocalModel::new(&res, &cfg);
        (res, m)
    })
}

fn db_path() -> Option<PathBuf> {
    Some(crate::storage::config::homemade_home().join("state.db"))
}

fn open_db() -> std::result::Result<Connection, String> {
    let p = db_path().ok_or("no home directory")?;
    if !p.exists() {
        return Err(format!("Homemade state database not found at {} (launch Homemade once first)", p.display()));
    }
    let c = Connection::open_with_flags(&p, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
        .map_err(|e| format!("open {}: {e}", p.display()))?;
    let _ = c.busy_timeout(std::time::Duration::from_millis(2000));
    Ok(c)
}

pub fn run_stdio() -> Result<()> {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut db: Option<Connection> = None;
    for line in stdin.lock().lines() {
        let line = match line { Ok(l) => l, Err(_) => break };
        if line.trim().is_empty() { continue; }
        let resp = match serde_json::from_str::<Value>(&line) {
            Ok(Value::Array(batch)) => {
                let out: Vec<Value> = batch.iter().filter_map(|m| handle(m, &mut db)).collect();
                if out.is_empty() { None } else { Some(Value::Array(out)) }
            }
            Ok(msg) => handle(&msg, &mut db),
            Err(e) => Some(error_resp(Value::Null, -32700, &format!("parse error: {e}"))),
        };
        if let Some(r) = resp {
            let s = serde_json::to_string(&r).unwrap_or_else(|_| "{}".into());
            if writeln!(stdout, "{s}").and_then(|_| stdout.flush()).is_err() { break; }
        }
    }
    Ok(())
}

fn error_resp(id: Value, code: i64, msg: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": msg } })
}

fn handle(msg: &Value, db: &mut Option<Connection>) -> Option<Value> {
    let method = msg.get("method").and_then(|m| m.as_str());
    let id = msg.get("id").cloned();
    let Some(method) = method else {
        // A response or garbage; reply only if it carried an id.
        return id.map(|id| error_resp(id, -32600, "invalid request"));
    };
    let Some(id) = id else { return None }; // notification (incl. notifications/initialized)
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    let result = match method {
        "initialize" => json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": { "tools": {} },
            "serverInfo": { "name": "homemade", "version": env!("CARGO_PKG_VERSION") }
        }),
        "ping" => json!({}),
        "tools/list" => json!({ "tools": tool_defs() }),
        "tools/call" => {
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or(json!({}));
            match call_tool(name, &args, db) {
                Ok(v) => json!({ "content": [{ "type": "text", "text": serde_json::to_string_pretty(&v).unwrap_or_default() }] }),
                Err(e) => json!({ "content": [{ "type": "text", "text": e }], "isError": true }),
            }
        }
        _ => return Some(error_resp(id, -32601, &format!("method not found: {method}"))),
    };
    Some(json!({ "jsonrpc": "2.0", "id": id, "result": result }))
}

/// Tools that work without the GUI (read-only DB / local model). Everything else needs the control socket.
const OFFLINE_TOOLS: &[&str] = &["list_apps", "get_app", "list_runs", "get_run_log", "last_failed_run", "list_artifacts", "draft_manifest", "validate_manifest", "status", "launch_gui"];

fn tool_defs() -> Value {
    let s = |props: Value, req: &[&str]| json!({ "type": "object", "properties": props, "required": req });
    let id = json!({ "type": "string" });
    let inputs = json!({ "type": "object", "description": "input values keyed by the manifest's [inputs] names" });
    json!([
        { "name": "app_activity", "description": "What an app is doing right now: current run (pid, port, elapsed, live log tail), live CPU/RAM, last finished run with exit code and log tail, recent runs, latest outputs, open agent sessions. Call this to catch up on anything the user did in the Homemade window.", "inputSchema": s(json!({ "app_id": id, "lines": { "type": "integer", "default": 40 } }), &["app_id"]) },
        { "name": "app_usage", "description": "Per-program resource usage: current CPU%/RAM/GPU of the app's run, its peak, and 1 Hz samples for the last `seconds` (≤300).", "inputSchema": s(json!({ "app_id": id, "seconds": { "type": "integer", "default": 60 } }), &["app_id"]) },
        { "name": "app_processes", "description": "Process tree of the app's running program: leader pid, pgid, port, and every process in the group with cpu% and RSS.", "inputSchema": s(json!({ "app_id": id }), &["app_id"]) },
        { "name": "signal_run", "description": "Send a POSIX signal (INT, TERM, KILL, HUP, USR1, USR2, STOP, CONT, QUIT) to a run's process group (or only the leader).", "inputSchema": s(json!({ "run_id": id, "signal": { "type": "string" }, "leader_only": { "type": "boolean" } }), &["run_id", "signal"]) },
        { "name": "write_stdin", "description": "Write text to the running program's stdin (include a trailing newline for line-reading programs; close=true then closes stdin).", "inputSchema": s(json!({ "run_id": id, "data": { "type": "string" }, "close": { "type": "boolean" } }), &["run_id", "data"]) },
        { "name": "capture", "description": "Screenshot the program's window (its solo window if open, else the Homemade main window) to a PNG; returns {path}. macOS only. Read the PNG to see what the user sees.", "inputSchema": s(json!({ "app_id": id, "path": { "type": "string" } }), &[]) },
        { "name": "status", "description": "Is the Homemade GUI running? Returns {running, version, active_runs, model} or {running:false}.", "inputSchema": s(json!({}), &[]) },
        { "name": "launch_gui", "description": "Start the Homemade GUI if it is not running and wait for it (needed for every tool that runs or controls programs).", "inputSchema": s(json!({}), &[]) },
        { "name": "import_app", "description": "Register a project folder (or a script; its folder is used) with Homemade. Infers homemade.toml when absent. Returns the app record.", "inputSchema": s(json!({ "path": { "type": "string" } }), &["path"]) },
        { "name": "remove_app", "description": "Unregister an app (source files are never deleted).", "inputSchema": s(json!({ "app_id": id, "purge_scratch": { "type": "boolean" }, "delete_venv": { "type": "boolean" } }), &["app_id"]) },
        { "name": "apply_manifest", "description": "Write homemade.toml into a folder (after validation) and register it. Use after draft_manifest.", "inputSchema": s(json!({ "path": { "type": "string" }, "toml": { "type": "string" } }), &["path", "toml"]) },
        { "name": "save_manifest", "description": "Overwrite a registered app's homemade.toml and reload it.", "inputSchema": s(json!({ "app_id": id, "toml": { "type": "string" } }), &["app_id", "toml"]) },
        { "name": "start_run", "description": "Start a run of an app with the given inputs. Returns the run record (id, status, cli_command). Pair with wait_run.", "inputSchema": s(json!({ "app_id": id, "inputs": inputs }), &["app_id"]) },
        { "name": "wait_run", "description": "Block until a run finishes (default timeout 10 min). Returns the final run record with exit_code and log_tail.", "inputSchema": s(json!({ "run_id": id, "timeout_ms": { "type": "integer" } }), &["run_id"]) },
        { "name": "stop_run", "description": "Stop a run cleanly (SIGINT, then SIGTERM, then SIGKILL).", "inputSchema": s(json!({ "run_id": id }), &["run_id"]) },
        { "name": "get_run", "description": "One run record by id.", "inputSchema": s(json!({ "run_id": id }), &["run_id"]) },
        { "name": "list_active_runs", "description": "Runs in progress right now.", "inputSchema": s(json!({}), &[]) },
        { "name": "build_cli_command", "description": "The exact shell command a run with these inputs would execute (without running it).", "inputSchema": s(json!({ "app_id": id, "inputs": inputs }), &["app_id"]) },
        { "name": "rpc_call", "description": "Call a method on an app's warm rpc_worker (start it first with start_run).", "inputSchema": s(json!({ "app_id": id, "method": { "type": "string" }, "params": { "type": "object" } }), &["app_id", "method"]) },
        { "name": "explain_run", "description": "Local-model explanation of what went wrong in a run (needs Apple Intelligence).", "inputSchema": s(json!({ "run_id": id }), &["run_id"]) },
        { "name": "list_presets", "description": "Saved input presets of an app.", "inputSchema": s(json!({ "app_id": id }), &["app_id"]) },
        { "name": "save_preset", "description": "Save an input preset.", "inputSchema": s(json!({ "app_id": id, "name": { "type": "string" }, "inputs": inputs }), &["app_id", "name"]) },
        { "name": "git_status", "description": "Git status of the app folder (branch, log with Homemade checkpoints, dirty files).", "inputSchema": s(json!({ "app_id": id }), &["app_id"]) },
        { "name": "git_checkpoint", "description": "Commit everything in the app folder as a Homemade checkpoint. Returns {sha}.", "inputSchema": s(json!({ "app_id": id, "message": { "type": "string" } }), &["app_id"]) },
        { "name": "open_app", "description": "Show the app in the GUI: as a tab, or (window=true) in its own solo window like a standalone program.", "inputSchema": s(json!({ "app_id": id, "window": { "type": "boolean" } }), &["app_id"]) },
        { "name": "close_window", "description": "Close an app's solo window; this stops its run cleanly.", "inputSchema": s(json!({ "app_id": id }), &["app_id"]) },
        { "name": "show_view", "description": "Switch the main window to a view.", "inputSchema": s(json!({ "view": { "type": "string", "enum": ["workbench", "launch", "monitor", "settings"] } }), &["view"]) },
        { "name": "resource_snapshot", "description": "Live CPU / RAM / GPU usage, per-run usage and the GPU admission queue.", "inputSchema": s(json!({}), &[]) },
        { "name": "list_listening_ports", "description": "Every listening TCP port on the machine, attributed to Homemade runs where possible.", "inputSchema": s(json!({}), &[]) },
        { "name": "kill_port_owner", "description": "Stop the process group holding a port, only if Homemade launched it.", "inputSchema": s(json!({ "pid": { "type": "integer" } }), &["pid"]) },
        { "name": "list_apps", "description": "List apps registered in Homemade (id, name, root_dir, mode, last_run_at).",
          "inputSchema": s(json!({}), &[]) },
        { "name": "get_app", "description": "Get an app's parsed homemade.toml manifest, root_dir and whether it has a custom UI.",
          "inputSchema": s(json!({ "app_id": { "type": "string" } }), &["app_id"]) },
        { "name": "list_runs", "description": "List an app's runs, newest first.",
          "inputSchema": s(json!({ "app_id": { "type": "string" }, "limit": { "type": "integer", "minimum": 1, "maximum": 500, "default": 20 } }), &["app_id"]) },
        { "name": "get_run_log", "description": "Log of a run (live ring buffer while running, stored tail after). With `offset`, returns {lines, next, total, active} from that line on, for incremental tailing: pass back `next` on the following call.",
          "inputSchema": s(json!({ "run_id": { "type": "string" }, "offset": { "type": "integer" } }), &["run_id"]) },
        { "name": "last_failed_run", "description": "Most recent failed/killed run of an app with its log tail, or null.",
          "inputSchema": s(json!({ "app_id": { "type": "string" } }), &["app_id"]) },
        { "name": "list_artifacts", "description": "List artifact files produced by an app (optionally for one run).",
          "inputSchema": s(json!({ "app_id": { "type": "string" }, "run_id": { "type": "string" } }), &["app_id"]) },
        { "name": "draft_manifest", "description": "Draft a homemade.toml for a project folder (reads README/docs/entry scripts; uses Apple's on-device model when available, else deterministic inference). Does not write anything. Returns {toml, agentPrompt, factsSummary, validationErrors, usedModel, notes}.",
          "inputSchema": s(json!({ "path": { "type": "string", "description": "absolute path of the project folder" } }), &["path"]) },
        { "name": "validate_manifest", "description": "Validate homemade.toml text against Homemade's schema. Returns {ok, errors[], normalized_toml}.",
          "inputSchema": s(json!({ "toml_text": { "type": "string" } }), &["toml_text"]) },
    ])
}

fn str_arg<'a>(args: &'a Value, k: &str) -> std::result::Result<&'a str, String> {
    args.get(k).and_then(|v| v.as_str()).filter(|s| !s.is_empty()).ok_or_else(|| format!("missing required argument `{k}`"))
}

fn conn<'a>(db: &'a mut Option<Connection>) -> std::result::Result<&'a Connection, String> {
    if db.is_none() { *db = Some(open_db()?); }
    Ok(db.as_ref().unwrap())
}

fn call_tool(name: &str, args: &Value, db: &mut Option<Connection>) -> std::result::Result<Value, String> {
    let e = |x: crate::error::Error| x.to_string();
    match name {
        "status" => return Ok(match crate::control::client::call("status", &json!({})) {
            Ok(v) => json!({ "running": true, "version": v["version"], "pid": v["pid"], "home": v["home"], "active_runs": v["active_runs"], "model": v["model"] }),
            Err(err) => json!({ "running": false, "hint": err }),
        }),
        "launch_gui" => { crate::control::client::launch_gui(std::time::Duration::from_secs(25))?; return Ok(json!({ "running": true })); }
        _ => {}
    }
    // Live path: the GUI's supervisor answers (same code as the UI). `get_run_log` returns lines there; keep this server's shape.
    if crate::control::client::available() {
        let method = match name { "last_failed_run" => "list_runs", n => n };
        let mut params = args.clone();
        if name == "last_failed_run" { params["limit"] = json!(200); }
        if name == "list_runs" && params.get("limit").is_none() { params["limit"] = json!(20); }
        let v = crate::control::client::call(method, &params)?;
        return Ok(match name {
            "last_failed_run" => v.as_array().and_then(|a| a.iter().find(|r| r["status"] == "failed" || r["status"] == "killed").cloned()).unwrap_or(Value::Null),
            "get_run_log" => if args.get("offset").is_some() { v } else { json!({ "run_id": args["run_id"], "log": v }) },
            _ => v,
        });
    }
    if !OFFLINE_TOOLS.contains(&name) {
        return Err("Homemade is not running, so this tool is unavailable. Call `launch_gui` first (or `open -a Homemade`).".into());
    }
    match name {
        "list_apps" => {
            let apps = queries::list_apps(conn(db)?).map_err(e)?;
            Ok(Value::Array(apps.into_iter().map(|a| json!({
                "id": a.id, "name": a.name, "root_dir": a.root_dir, "mode": a.execution_mode, "last_run_at": a.last_run_at,
            })).collect()))
        }
        "get_app" => {
            let id = str_arg(args, "app_id")?;
            let a = queries::get_app(conn(db)?, id).map_err(e)?.ok_or_else(|| format!("unknown app: {id}"))?;
            let root = Path::new(&a.root_dir);
            let mpath = root.join("homemade.toml");
            let (manifest, manifest_error) = match std::fs::read_to_string(&mpath) {
                Err(_) => (Value::Null, Some("no manifest (homemade.toml absent; Homemade infers one)".to_string())),
                Ok(t) => match HomemadeManifest::parse(&t) {
                    Ok(m) => (serde_json::to_value(&m).unwrap_or(Value::Null), None),
                    Err(err) => (Value::Null, Some(err.to_string())),
                },
            };
            let has_custom_ui = manifest.pointer("/ui/entry").and_then(|v| v.as_str())
                .map(|en| root.join(en).is_file()).unwrap_or(false);
            Ok(json!({
                "id": a.id, "name": a.name, "root_dir": a.root_dir, "mode": a.execution_mode, "last_run_at": a.last_run_at,
                "manifest_path": mpath, "manifest": manifest, "manifest_error": manifest_error, "has_custom_ui": has_custom_ui,
            }))
        }
        "list_runs" => {
            let id = str_arg(args, "app_id")?;
            let limit = args.get("limit").and_then(|v| v.as_i64()).unwrap_or(20).clamp(1, 500);
            let runs = queries::list_runs(conn(db)?, id, limit).map_err(e)?;
            Ok(Value::Array(runs.into_iter().map(|r| json!({
                "id": r.id, "status": r.status, "exit_code": r.exit_code, "cli_command": r.cli_command,
                "inputs_json": r.inputs_json, "started_at": r.started_at, "finished_at": r.finished_at,
            })).collect()))
        }
        "get_run_log" => {
            let id = str_arg(args, "run_id")?;
            let r = queries::get_run(conn(db)?, id).map_err(e)?.ok_or_else(|| format!("unknown run: {id}"))?;
            Ok(json!({ "run_id": r.id, "status": r.status, "log_tail": r.log_tail.unwrap_or_default() }))
        }
        "last_failed_run" => {
            let id = str_arg(args, "app_id")?;
            let runs = queries::list_runs(conn(db)?, id, 200).map_err(e)?;
            Ok(runs.into_iter().find(|r| r.status == "failed" || r.status == "killed")
                .map(|r| serde_json::to_value(&r).unwrap_or(Value::Null)).unwrap_or(Value::Null))
        }
        "list_artifacts" => {
            let id = str_arg(args, "app_id")?;
            let run = args.get("run_id").and_then(|v| v.as_str()).filter(|s| !s.is_empty());
            let a = queries::list_artifacts(conn(db)?, id, run, None).map_err(e)?;
            serde_json::to_value(&a).map_err(|x| x.to_string())
        }
        "validate_manifest" => {
            let t = str_arg(args, "toml_text")?;
            Ok(match HomemadeManifest::parse(t) {
                Ok(m) => {
                    let mut errors: Vec<String> = vec![];
                    if m.app.id.trim().is_empty() { errors.push("app.id is empty".into()); }
                    if m.app.exec.trim().is_empty() { errors.push("app.exec is empty".into()); }
                    json!({ "ok": errors.is_empty(), "errors": errors, "normalized_toml": m.to_toml() })
                }
                Err(err) => json!({ "ok": false, "errors": [err.to_string()], "normalized_toml": Value::Null }),
            })
        }
        "draft_manifest" => {
            let p = str_arg(args, "path")?;
            let root = PathBuf::from(p).canonicalize().map_err(|x| format!("cannot open {p}: {x}"))?;
            let root = if root.is_file() { root.parent().map(Path::to_path_buf).unwrap_or(root) } else { root };
            let (res, m) = model();
            let d = tauri::async_runtime::block_on(crate::intelligence::draft::draft_manifest(m, &root, res)).map_err(e)?;
            serde_json::to_value(&d).map_err(|x| x.to_string())
        }
        other => Err(format!("unknown tool: {other}")),
    }
}
