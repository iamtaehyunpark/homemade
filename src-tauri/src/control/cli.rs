//! `homemade <command>`: a scriptable CLI over the control socket. Human output by default, `--json` for raw JSON.
use super::client;
use serde_json::{json, Map, Value};

const HELP: &str = "\
homemade — drive the Homemade launcher from a shell or an agent

  homemade status                         GUI status (version, active runs, local model)
  homemade launch                         start the GUI if it is not running
  homemade apps                           registered programs
  homemade app <id>                       one program with its manifest
  homemade import <path>                  register a folder (or script); infers homemade.toml when absent
  homemade new <name> [--in DIR]          create an empty project (default location ~/homemade, or [projects] dir in config) and register it
  homemade remove <id> [--purge-scratch] [--delete-venv]
                                          hide a program (unregister; source files stay)
  homemade delete <id> [--plan] [--keep k1,k2] [--with m1,m2]
                                          clean-delete: folder → Trash, env, scratch, transcripts, packages only it uses;
                                          --plan lists the items ([x] = default), --keep/--with adjust the keys
  homemade draft <path>                   draft a homemade.toml + agent prompt (never writes)
  homemade apply <path> <toml-file|->     write homemade.toml and register the folder
  homemade run <id> [k=v | --k v]... [--wait] [--preset NAME]
                                          start a run; inputs are coerced by the manifest's declared types
  homemade stop <run-id>                  stop a run (SIGINT → SIGTERM → SIGKILL)
  homemade wait <run-id> [--timeout S]    block until the run ends, print its record, exit 0 on success
  homemade runs <id> [--limit N]          run history
  homemade active                         runs in progress
  homemade activity <id>                  what the app is doing now: current run, live log tail, last result, outputs
  homemade log <run-id> [--follow]        run log (live ring buffer while active, stored tail after)
  homemade explain <run-id>               local-model explanation of a failed run
  homemade top <id> [--watch] [--seconds N]
                                          the program's CPU / RAM / GPU: now, peak, and the last N s of samples
  homemade ps <id>                        process tree of the running program (pid, pgid, port, per-process cpu/rss)
  homemade signal <run-id> <SIG> [--leader]
                                          send INT|TERM|KILL|HUP|USR1|USR2|STOP|CONT|QUIT to the process group
  homemade stdin <run-id> <text> [--close] write to the program's stdin (a literal backslash-n becomes a newline)
  homemade capture [id] [--out FILE]      screenshot of the program's window (or the main window) → PNG path
  homemade record <id> [--seconds N] [--out FILE]
                                          record usage samples + new log lines as JSON lines for N s (default 30)
  homemade cli <id> [k=v]...              the shell command a run would execute
  homemade rpc <id> <method> [json]       call a method on a warm rpc_worker
  homemade artifacts <id> [run-id]        output files
  homemade presets <id> | preset-save <id> NAME [k=v]...
  homemade git <id> [status|init|checkpoint MSG|revert SHA]
  homemade open <id> [--window]           show the program in the GUI (tab, or its own solo window)
  homemade close <id>                     close the program's solo window (stops its run)
  homemade view <workbench|launch|monitor|settings>
  homemade ports [--kill PID]             listening TCP ports, attributed to runs; --kill stops a Homemade-owned one
  homemade resources                      live CPU / RAM / GPU snapshot with per-run usage
  homemade mcp                            serve MCP over stdio (same as --mcp)

Options: --json (raw JSON output)   HOMEMADE_HOME (alternate home; default ~/.homemade)
Add `~/.homemade/bin` to PATH to get this command anywhere.";

pub fn run(args: Vec<String>) -> i32 {
    let json_out = args.iter().any(|a| a == "--json");
    let args: Vec<String> = args.into_iter().filter(|a| a != "--json").collect();
    let cmd = args.first().map(String::as_str).unwrap_or("help");
    let rest = &args[1.min(args.len())..];
    match go(cmd, rest, json_out) {
        Ok(code) => code,
        Err(e) => { eprintln!("homemade: {e}"); 1 }
    }
}

fn flag(rest: &[String], name: &str) -> bool { rest.iter().any(|a| a == name) }
fn flag_val(rest: &[String], name: &str) -> Option<String> { rest.iter().position(|a| a == name).and_then(|i| rest.get(i + 1).cloned()) }
fn arg(rest: &[String], i: usize, what: &str) -> Result<String, String> { rest.get(i).cloned().ok_or_else(|| format!("missing {what}. See `homemade help`.")) }

fn go(cmd: &str, rest: &[String], json_out: bool) -> Result<i32, String> {
    let print = |v: &Value| { println!("{}", serde_json::to_string_pretty(v).unwrap_or_default()); };
    match cmd {
        "help" | "--help" | "-h" => { println!("{HELP}"); Ok(0) }
        "launch" => { client::launch_gui(std::time::Duration::from_secs(20))?; println!("Homemade is running."); Ok(0) }
        "status" => {
            match client::call("status", &json!({})) {
                Ok(v) => { if json_out { print(&v); } else {
                    println!("Homemade {} running (pid {})", v["version"].as_str().unwrap_or("?"), v["pid"]);
                    println!("home: {}  active runs: {}  terminals: {}", v["home"].as_str().unwrap_or("?"), v["active_runs"], v["active_terminals"]);
                    println!("local model: {}", if v["model"]["available"].as_bool().unwrap_or(false) { "available" } else { "unavailable" });
                } Ok(0) }
                Err(e) => { if json_out { print(&json!({ "ok": false, "error": e })); } else { println!("{e}"); } Ok(3) }
            }
        }
        "apps" => {
            let v = client::call("list_apps", &json!({}))?;
            if json_out { print(&v); return Ok(0); }
            let active: Vec<String> = client::call("list_active_runs", &json!({})).ok().and_then(|a| a.as_array().map(|a| a.iter().filter_map(|r| r["appId"].as_str().map(String::from)).collect())).unwrap_or_default();
            println!("{:<22} {:<10} {:<8} {}", "ID", "MODE", "STATE", "ROOT");
            for a in v.as_array().cloned().unwrap_or_default() {
                let id = a["id"].as_str().unwrap_or("");
                println!("{:<22} {:<10} {:<8} {}", id, a["executionMode"].as_str().unwrap_or(""), if active.iter().any(|x| x == id) { "running" } else { "" }, a["rootDir"].as_str().unwrap_or(""));
            }
            Ok(0)
        }
        "app" => { print(&client::call("get_app", &json!({ "app_id": arg(rest, 0, "app id")? }))?); Ok(0) }
        "import" => {
            let p = abs(&arg(rest, 0, "path")?);
            let v = client::call("import_app", &json!({ "path": p }))?;
            if json_out { print(&v); } else { println!("Registered {} ({}) at {}", v["name"].as_str().unwrap_or(""), v["id"].as_str().unwrap_or(""), v["rootDir"].as_str().unwrap_or("")); }
            Ok(0)
        }
        "new" => {
            let v = client::call("create_project", &json!({ "parent_dir": flag_val(rest, "--in").map(|d| abs(&d)), "name": arg(rest, 0, "folder name")? }))?;
            if json_out { print(&v); } else { println!("Created {} ({}). Set app.exec in {}/homemade.toml to make it runnable.", v["rootDir"].as_str().unwrap_or(""), v["id"].as_str().unwrap_or(""), v["rootDir"].as_str().unwrap_or("")); }
            Ok(0)
        }
        "remove" => { client::call("remove_app", &json!({ "app_id": arg(rest, 0, "app id")?, "purge_scratch": flag(rest, "--purge-scratch"), "delete_venv": flag(rest, "--delete-venv") }))?; println!("Removed."); Ok(0) }
        "delete" => {
            let app_id = arg(rest, 0, "app id")?;
            let plan = client::call("delete_plan", &json!({ "app_id": app_id }))?;
            if let Some(b) = plan["blocked"].as_str() { return Err(b.to_string()); }
            let items = plan["items"].as_array().cloned().unwrap_or_default();
            if flag(rest, "--plan") {
                if json_out { print(&plan); return Ok(0); }
                println!("{} ({})", plan["name"].as_str().unwrap_or(""), plan["rootDir"].as_str().unwrap_or(""));
                for it in &items {
                    let b = it["bytes"].as_i64().unwrap_or(-1);
                    let size = if b < 0 { "?".to_string() } else { format!("{:.1} MB", b as f64 / 1048576.0) };
                    println!("  [{}] {:<22} {:>10}  {}", if it["checked"].as_bool().unwrap_or(false) { "x" } else { " " },
                        it["key"].as_str().unwrap_or(""), size, it["label"].as_str().unwrap_or(""));
                }
                let always: Vec<&str> = plan["always"].as_array().map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
                println!("  always removed: {}", always.join(", "));
                return Ok(0);
            }
            let split = |f: &str| -> Vec<String> { flag_val(rest, f).map(|v| v.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()).unwrap_or_default() };
            let (keep, with) = (split("--keep"), split("--with"));
            let mut keys: Vec<String> = items.iter().filter(|i| i["checked"].as_bool().unwrap_or(false))
                .filter_map(|i| i["key"].as_str().map(String::from)).filter(|k| !keep.contains(k)).collect();
            for w in with { if !keys.contains(&w) { keys.push(w); } }
            let v = client::call("delete_app", &json!({ "app_id": app_id, "keys": keys }))?;
            if json_out { print(&v); } else {
                println!("Deleted. Freed {:.1} MB{}{}", v["freedBytes"].as_i64().unwrap_or(0) as f64 / 1048576.0,
                    if v["trashed"].as_bool().unwrap_or(false) { ", folder moved to Trash" } else { "" },
                    match v["prunedPackages"].as_array().map(|a| a.len()).unwrap_or(0) { 0 => String::new(), n => format!(", pruned {n} packages") });
            }
            Ok(0)
        }
        "draft" => {
            let v = client::call("draft_manifest", &json!({ "path": abs(&arg(rest, 0, "path")?) }))?;
            if json_out { print(&v); } else {
                println!("{}", v["toml"].as_str().unwrap_or(""));
                if let Some(errs) = v["validationErrors"].as_array().filter(|a| !a.is_empty()) { eprintln!("# validation: {}", errs.iter().filter_map(|e| e.as_str()).collect::<Vec<_>>().join("; ")); }
                if let Some(p) = v["agentPrompt"].as_str() { eprintln!("\n# Agent prompt:\n{p}"); }
            }
            Ok(0)
        }
        "apply" => {
            let p = abs(&arg(rest, 0, "path")?); let src = arg(rest, 1, "toml file (or - for stdin)")?;
            let toml = if src == "-" { let mut s = String::new(); std::io::Read::read_to_string(&mut std::io::stdin(), &mut s).map_err(|e| e.to_string())?; s } else { std::fs::read_to_string(&src).map_err(|e| format!("{src}: {e}"))? };
            let v = client::call("apply_manifest", &json!({ "path": p, "toml": toml }))?;
            if json_out { print(&v); } else { println!("Applied; registered as {}", v["id"].as_str().unwrap_or("")); }
            Ok(0)
        }
        "run" => {
            let id = arg(rest, 0, "app id")?;
            let app = client::call("get_app", &json!({ "app_id": id }))?;
            let mut inputs = if let Some(name) = flag_val(rest, "--preset") {
                let ps = client::call("list_presets", &json!({ "app_id": id }))?;
                let p = ps.as_array().and_then(|a| a.iter().find(|p| p["name"].as_str() == Some(&name))).ok_or(format!("no preset named {name}"))?;
                serde_json::from_str::<Value>(p["inputsJson"].as_str().unwrap_or("{}")).unwrap_or(json!({}))
            } else { json!({}) };
            merge_inputs(&mut inputs, &rest[1..], &app["manifest"]["inputs"])?;
            let r = client::call("start_run", &json!({ "app_id": id, "inputs": inputs }))?;
            let rid = r["id"].as_str().unwrap_or("").to_string();
            if !flag(rest, "--wait") { if json_out { print(&r); } else { println!("{rid}"); eprintln!("started ({}): {}", r["status"].as_str().unwrap_or(""), r["cliCommand"].as_str().unwrap_or("")); } return Ok(0); }
            if !json_out { eprintln!("started {rid}: {}", r["cliCommand"].as_str().unwrap_or("")); }
            wait_and_report(&rid, None, json_out)
        }
        "stop" => { client::call("stop_run", &json!({ "run_id": arg(rest, 0, "run id")? }))?; println!("Stop requested."); Ok(0) }
        "wait" => { let t = flag_val(rest, "--timeout").and_then(|s| s.parse::<u64>().ok()).map(|s| s * 1000); wait_and_report(&arg(rest, 0, "run id")?, t, json_out) }
        "runs" => {
            let v = client::call("list_runs", &json!({ "app_id": arg(rest, 0, "app id")?, "limit": flag_val(rest, "--limit").and_then(|s| s.parse::<i64>().ok()).unwrap_or(20) }))?;
            if json_out { print(&v); return Ok(0); }
            println!("{:<38} {:<11} {:<5} {:<24} {}", "RUN", "STATUS", "EXIT", "STARTED", "COMMAND");
            for r in v.as_array().cloned().unwrap_or_default() { println!("{:<38} {:<11} {:<5} {:<24} {}", r["id"].as_str().unwrap_or(""), r["status"].as_str().unwrap_or(""), r["exitCode"].as_i64().map(|x| x.to_string()).unwrap_or_default(), ts(r["startedAt"].as_i64()), r["cliCommand"].as_str().unwrap_or("")); }
            Ok(0)
        }
        "active" => {
            let v = client::call("list_active_runs", &json!({}))?;
            if json_out { print(&v); return Ok(0); }
            let a = v.as_array().cloned().unwrap_or_default();
            if a.is_empty() { println!("No runs in progress."); }
            for r in a { println!("{}  {}  {}  {}", r["id"].as_str().unwrap_or(""), r["appId"].as_str().unwrap_or(""), r["status"].as_str().unwrap_or(""), r["cliCommand"].as_str().unwrap_or("")); }
            Ok(0)
        }
        "activity" => {
            let v = client::call("app_activity", &json!({ "app_id": arg(rest, 0, "app id")?, "lines": flag_val(rest, "--lines").and_then(|s| s.parse::<u64>().ok()).unwrap_or(40) }))?;
            if json_out { print(&v); return Ok(0); }
            println!("{}: {}", v["appId"].as_str().unwrap_or(""), v["state"].as_str().unwrap_or(""));
            if let Some(c) = v["current"].as_object() {
                println!("current run {} pid {} elapsed {:.0}s{}", c["runId"].as_str().unwrap_or(""), c["pid"], c["elapsedMs"].as_f64().unwrap_or(0.0) / 1000.0, c["port"].as_u64().map(|p| format!(" port {p}{}", if c["ready"].as_bool().unwrap_or(false) { " (ready)" } else { "" })).unwrap_or_default());
                for l in c["logTail"].as_array().cloned().unwrap_or_default() { println!("  {}", l.as_str().unwrap_or("")); }
            }
            if let Some(l) = v["lastFinished"].as_object() { println!("last finished: {} {} exit {} at {}", l["id"].as_str().unwrap_or(""), l["status"].as_str().unwrap_or(""), l["exitCode"], ts(l["finishedAt"].as_i64())); }
            for a in v["latestOutputs"].as_array().cloned().unwrap_or_default().iter().take(5) { println!("  output: {}", a["filePath"].as_str().unwrap_or("")); }
            Ok(0)
        }
        "top" => {
            let id = arg(rest, 0, "app id")?;
            let secs = flag_val(rest, "--seconds").and_then(|s| s.parse::<u64>().ok()).unwrap_or(60);
            loop {
                let v = client::call("app_usage", &json!({ "app_id": id, "seconds": secs }))?;
                if json_out { print(&v); } else {
                    match v["current"].as_object() {
                        Some(c) => println!("{id}: cpu {:.1}%  ram {} MB  gpu {} MB  (peak ram {} MB){}", c["cpuPct"].as_f64().unwrap_or(0.0), c["ramMb"], c["vramMb"], v["peak"]["ramMb"], if flag(rest, "--watch") { "" } else { "" }),
                        None => println!("{id}: not running"),
                    }
                    let samples = v["samples"].as_array().cloned().unwrap_or_default();
                    if !samples.is_empty() && !flag(rest, "--watch") {
                        println!("{:<24} {:>7} {:>8} {:>8}", "TIME", "CPU%", "RAM MB", "GPU MB");
                        for s in &samples { println!("{:<24} {:>7.1} {:>8} {:>8}", ts(s["t"].as_i64()), s["cpuPct"].as_f64().unwrap_or(0.0), s["ramMb"], s["vramMb"]); }
                    }
                }
                if !flag(rest, "--watch") { return Ok(0); }
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
        }
        "ps" => {
            let v = client::call("app_processes", &json!({ "app_id": arg(rest, 0, "app id")? }))?;
            if json_out { print(&v); return Ok(0); }
            if v["runId"].is_null() { println!("not running"); return Ok(0); }
            println!("run {}  pid {}  pgid {}  port {}", v["runId"].as_str().unwrap_or(""), v["pid"], v["pgid"], v["port"].as_u64().map(|p| p.to_string()).unwrap_or_else(|| "-".into()));
            println!("{:<8} {:<8} {:>6} {:>8} {}", "PID", "PPID", "CPU%", "RSS MB", "COMMAND");
            for p in v["processes"].as_array().cloned().unwrap_or_default() { println!("{:<8} {:<8} {:>6.1} {:>8} {}", p["pid"], p["parentPid"].as_u64().map(|x| x.to_string()).unwrap_or_default(), p["cpuPct"].as_f64().unwrap_or(0.0), p["rssMb"], p["cmd"].as_str().filter(|c| !c.is_empty()).unwrap_or(p["name"].as_str().unwrap_or(""))); }
            Ok(0)
        }
        "signal" => { let v = client::call("signal_run", &json!({ "run_id": arg(rest, 0, "run id")?, "signal": arg(rest, 1, "signal")?, "leader_only": flag(rest, "--leader") }))?; println!("sent {} to pgid {}", v["signal"].as_str().unwrap_or(""), v["pgid"]); Ok(0) }
        "stdin" => {
            let text = arg(rest, 1, "text")?.replace("\\n", "\n").replace("\\t", "\t");
            client::call("write_stdin", &json!({ "run_id": arg(rest, 0, "run id")?, "data": text, "close": flag(rest, "--close") }))?; Ok(0)
        }
        "capture" => {
            let id = rest.first().filter(|a| !a.starts_with("--")).cloned();
            let v = client::call("capture", &json!({ "app_id": id, "path": flag_val(rest, "--out") }))?;
            if json_out { print(&v); } else { println!("{}", v["path"].as_str().unwrap_or("")); }
            Ok(0)
        }
        "record" => {
            let id = arg(rest, 0, "app id")?;
            let secs = flag_val(rest, "--seconds").and_then(|s| s.parse::<u64>().ok()).unwrap_or(30);
            let out_path = flag_val(rest, "--out").unwrap_or_else(|| format!("{}/captures/{id}-{}.jsonl", crate::storage::config::homemade_home().display(), chrono::Local::now().format("%Y%m%d-%H%M%S")));
            if let Some(d) = std::path::Path::new(&out_path).parent() { let _ = std::fs::create_dir_all(d); }
            let mut f = std::fs::File::create(&out_path).map_err(|e| format!("{out_path}: {e}"))?;
            use std::io::Write as _;
            let end = std::time::Instant::now() + std::time::Duration::from_secs(secs);
            let (mut run_id, mut offset) = (None::<String>, 0u64);
            let mut n = 0usize;
            eprintln!("recording {id} for {secs}s → {out_path}");
            while std::time::Instant::now() < end {
                let u = client::call("app_usage", &json!({ "app_id": id, "seconds": 1 }))?;
                let t = crate::types::now_ms();
                writeln!(f, "{}", json!({ "t": t, "kind": "usage", "current": u["current"], "peak": u["peak"] })).ok();
                let cur = client::call("app_process_info", &json!({ "app_id": id }))?;
                let rid = cur["runId"].as_str().map(String::from);
                if rid != run_id { run_id = rid; offset = 0; if let Some(r) = &run_id { writeln!(f, "{}", json!({ "t": t, "kind": "run", "runId": r, "pid": cur["pid"], "port": cur["port"] })).ok(); } }
                if let Some(r) = &run_id {
                    let l = client::call("get_run_log", &json!({ "run_id": r, "offset": offset }))?;
                    for line in l["lines"].as_array().cloned().unwrap_or_default() { writeln!(f, "{}", json!({ "t": t, "kind": "log", "runId": r, "line": line })).ok(); }
                    offset = l["next"].as_u64().unwrap_or(offset);
                }
                n += 1;
                std::thread::sleep(std::time::Duration::from_secs(1));
            }
            if json_out { print(&json!({ "path": out_path, "samples": n })); } else { println!("{out_path}"); }
            Ok(0)
        }
        "log" => {
            let id = arg(rest, 0, "run id")?;
            let mut offset = 0u64;
            loop {
                let v = client::call("get_run_log", &json!({ "run_id": id, "offset": offset }))?;
                for l in v["lines"].as_array().cloned().unwrap_or_default() { println!("{}", l.as_str().unwrap_or("")); }
                offset = v["next"].as_u64().unwrap_or(offset);
                if !flag(rest, "--follow") { return Ok(0); }
                let r = client::call("get_run", &json!({ "run_id": id }))?;
                if !matches!(r["status"].as_str(), Some("running") | Some("queued_vram") | Some("stopping")) { return Ok(if r["status"] == "success" { 0 } else { 2 }); }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
        "explain" => { let v = client::call("explain_run", &json!({ "run_id": arg(rest, 0, "run id")? }))?; if json_out { print(&v); } else { println!("[{}] {}", v["severity"].as_str().unwrap_or(""), v["message"].as_str().unwrap_or("")); } Ok(0) }
        "cli" => {
            let id = arg(rest, 0, "app id")?; let app = client::call("get_app", &json!({ "app_id": id }))?;
            let mut inputs = json!({}); merge_inputs(&mut inputs, &rest[1..], &app["manifest"]["inputs"])?;
            println!("{}", client::call("build_cli_command", &json!({ "app_id": id, "inputs": inputs }))?.as_str().unwrap_or("")); Ok(0)
        }
        "rpc" => {
            let params: Value = rest.get(2).map(|s| serde_json::from_str(s).map_err(|e| format!("params must be JSON: {e}"))).transpose()?.unwrap_or(json!({}));
            print(&client::call("rpc_call", &json!({ "app_id": arg(rest, 0, "app id")?, "method": arg(rest, 1, "method")?, "params": params }))?); Ok(0)
        }
        "artifacts" => {
            let v = client::call("list_artifacts", &json!({ "app_id": arg(rest, 0, "app id")?, "run_id": rest.get(1) }))?;
            if json_out { print(&v); } else { for a in v.as_array().cloned().unwrap_or_default() { println!("{:>10}  {}", a["fileSizeBytes"], a["filePath"].as_str().unwrap_or("")); } }
            Ok(0)
        }
        "presets" => { print(&client::call("list_presets", &json!({ "app_id": arg(rest, 0, "app id")? }))?); Ok(0) }
        "preset-save" => {
            let id = arg(rest, 0, "app id")?; let name = arg(rest, 1, "preset name")?;
            let app = client::call("get_app", &json!({ "app_id": id }))?;
            let mut inputs = json!({}); merge_inputs(&mut inputs, &rest[2..], &app["manifest"]["inputs"])?;
            print(&client::call("save_preset", &json!({ "app_id": id, "name": name, "inputs": inputs }))?); Ok(0)
        }
        "git" => {
            let id = arg(rest, 0, "app id")?;
            let v = match rest.get(1).map(String::as_str).unwrap_or("status") {
                "status" => client::call("git_status", &json!({ "app_id": id }))?,
                "init" => client::call("git_init", &json!({ "app_id": id }))?,
                "checkpoint" => client::call("git_checkpoint", &json!({ "app_id": id, "message": rest.get(2).cloned().unwrap_or_else(|| "Checkpoint".into()) }))?,
                "revert" => client::call("git_revert_to", &json!({ "app_id": id, "sha": arg(rest, 2, "sha")? }))?,
                o => return Err(format!("unknown git action {o}")),
            };
            print(&v); Ok(0)
        }
        "open" => { client::call("open_app", &json!({ "app_id": arg(rest, 0, "app id")?, "window": flag(rest, "--window") }))?; Ok(0) }
        "close" => { client::call("close_window", &json!({ "app_id": arg(rest, 0, "app id")? }))?; Ok(0) }
        "view" => { client::call("show_view", &json!({ "view": arg(rest, 0, "view")? }))?; Ok(0) }
        "ports" => {
            if let Some(pid) = flag_val(rest, "--kill") { client::call("kill_port_owner", &json!({ "pid": pid.parse::<i64>().map_err(|_| "pid must be a number")? }))?; println!("Stopped."); return Ok(0); }
            let v = client::call("list_listening_ports", &json!({}))?;
            if json_out { print(&v); return Ok(0); }
            println!("{:<7} {:<8} {:<20} {:<22} {}", "PORT", "PID", "COMMAND", "APP", "RUN");
            for p in v.as_array().cloned().unwrap_or_default() { println!("{:<7} {:<8} {:<20} {:<22} {}{}", p["port"], p["pid"], p["command"].as_str().unwrap_or(""), p["appId"].as_str().unwrap_or("-"), p["runId"].as_str().unwrap_or("-"), if p["active"].as_bool().unwrap_or(false) { " (active)" } else { "" }); }
            Ok(0)
        }
        "resources" => { print(&client::call("resource_snapshot", &json!({}))?); Ok(0) }
        "mcp" => { crate::integrations::mcp_server::run_stdio().map_err(|e| e.to_string())?; Ok(0) }
        other => Err(format!("unknown command `{other}`. See `homemade help`.")),
    }
}

fn wait_and_report(rid: &str, timeout_ms: Option<u64>, json_out: bool) -> Result<i32, String> {
    let r = client::call("wait_run", &json!({ "run_id": rid, "timeout_ms": timeout_ms }))?;
    let status = r["status"].as_str().unwrap_or("");
    if json_out { println!("{}", serde_json::to_string_pretty(&r).unwrap_or_default()); }
    else {
        println!("{status} (exit {})", r["exitCode"].as_i64().map(|x| x.to_string()).unwrap_or_else(|| "-".into()));
        if status != "success" { if let Some(t) = r["logTail"].as_str() { for l in t.lines().rev().take(15).collect::<Vec<_>>().into_iter().rev() { eprintln!("  {l}"); } } }
    }
    Ok(if status == "success" { 0 } else { 2 })
}

fn abs(p: &str) -> String { std::fs::canonicalize(p).map(|x| x.to_string_lossy().into_owned()).unwrap_or_else(|_| p.to_string()) }

fn ts(ms: Option<i64>) -> String {
    ms.and_then(|m| chrono::DateTime::from_timestamp_millis(m)).map(|d| d.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string()).unwrap_or_default()
}

/// `k=v`, `--k v`, `--k` (bool true), `--no-k` (bool false). Values are coerced by the manifest's declared input type.
fn merge_inputs(inputs: &mut Value, args: &[String], schema: &Value) -> Result<(), String> {
    let skip = ["--wait", "--json"];
    let m = inputs.as_object_mut().ok_or("inputs not an object")?;
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        if skip.contains(&a.as_str()) { i += 1; continue; }
        if a == "--preset" { i += 2; continue; }
        if let Some(k) = a.strip_prefix("--no-") { set(m, k, Value::Bool(false), schema)?; i += 1; continue; }
        if let Some(k) = a.strip_prefix("--") {
            let (k, v) = match k.split_once('=') { Some((k, v)) => (k.to_string(), Some(v.to_string())), None => (k.to_string(), None) };
            let is_bool = schema[&k]["type"].as_str() == Some("bool");
            match v {
                Some(v) => { set(m, &k, coerce(&k, &v, schema), schema)?; i += 1; }
                None if is_bool || args.get(i + 1).map(|n| n.starts_with("--")).unwrap_or(true) => { set(m, &k, Value::Bool(true), schema)?; i += 1; }
                None => { set(m, &k, coerce(&k, &args[i + 1], schema), schema)?; i += 2; }
            }
            continue;
        }
        if let Some((k, v)) = a.split_once('=') { set(m, k, coerce(k, v, schema), schema)?; i += 1; continue; }
        return Err(format!("unexpected argument `{a}` (use key=value or --key value)"));
    }
    Ok(())
}

fn set(m: &mut Map<String, Value>, k: &str, v: Value, schema: &Value) -> Result<(), String> {
    if schema.is_object() && schema.get(k).is_none() {
        let known: Vec<&str> = schema.as_object().map(|o| o.keys().map(String::as_str).collect()).unwrap_or_default();
        return Err(format!("unknown input `{k}` (declared inputs: {})", if known.is_empty() { "none".into() } else { known.join(", ") }));
    }
    m.insert(k.to_string(), v); Ok(())
}

fn coerce(k: &str, v: &str, schema: &Value) -> Value {
    match schema[k]["type"].as_str() {
        Some("int") => v.parse::<i64>().map(Value::from).unwrap_or_else(|_| Value::String(v.into())),
        Some("float") | Some("range") => v.parse::<f64>().map(Value::from).unwrap_or_else(|_| Value::String(v.into())),
        Some("bool") => Value::Bool(matches!(v, "1" | "true" | "yes" | "on")),
        Some("file") | Some("directory") => Value::String(abs(v)),
        Some("file_list") => Value::Array(v.split(',').map(|p| Value::String(abs(p.trim()))).collect()),
        _ => Value::String(v.into()),
    }
}
