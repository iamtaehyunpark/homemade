//! `homemade --mcp`: stdio MCP server, read-only DB (spec §10D) + draft_manifest (local model). OWNER: agent G2.
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
        let cfg = dirs::home_dir().map(|h| crate::storage::config::load(&h.join(".homemade"))).unwrap_or_default();
        let m = crate::intelligence::local_model::LocalModel::new(&res, &cfg);
        (res, m)
    })
}

fn db_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".homemade").join("state.db"))
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

fn tool_defs() -> Value {
    let s = |props: Value, req: &[&str]| json!({ "type": "object", "properties": props, "required": req });
    json!([
        { "name": "list_apps", "description": "List apps registered in Homemade (id, name, root_dir, mode, last_run_at).",
          "inputSchema": s(json!({}), &[]) },
        { "name": "get_app", "description": "Get an app's parsed homemade.toml manifest, root_dir and whether it has a custom UI.",
          "inputSchema": s(json!({ "app_id": { "type": "string" } }), &["app_id"]) },
        { "name": "list_runs", "description": "List an app's runs, newest first.",
          "inputSchema": s(json!({ "app_id": { "type": "string" }, "limit": { "type": "integer", "minimum": 1, "maximum": 500, "default": 20 } }), &["app_id"]) },
        { "name": "get_run_log", "description": "Get the log tail of a run.",
          "inputSchema": s(json!({ "run_id": { "type": "string" } }), &["run_id"]) },
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
