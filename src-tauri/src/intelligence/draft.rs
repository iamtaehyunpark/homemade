//! Draft a homemade.toml for a folder: repo facts -> local model JSON -> normalized manifest -> validated TOML.
//! Falls back to the deterministic `load_or_infer` manifest. Never writes to disk.
use super::local_model::LocalModel;
use super::repo_facts::{self, RepoFacts};
use crate::error::Result;
use crate::manifest::schema::{HomemadeManifest, InputFieldSpec};
use serde::Serialize;
use serde_json::{json, Map, Value};
use std::path::Path;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftResult {
    pub toml: String,
    pub agent_prompt: String,
    pub facts_summary: String,
    pub validation_errors: Vec<String>,
    pub used_model: bool,
    pub notes: Vec<String>,
}

/// Compact manifest reference (spec §3 + §6) shared by the model prompt and the agent prompt.
pub const MANIFEST_REFERENCE: &str = r#"homemade.toml tables:
[app] id (slug), name, description, mode ("cli"|"rpc_worker"|"webapp"), exec (command run from the repo root; inputs are appended as flags/positionals), env_manager ("uv"|"pnpm"|"cargo"|"byo"|"system")
[ui] entry (optional "ui/index.html"; omit for an auto form)
[resources] requires_gpu (bool), vram_estimate_mb (int, with requires_gpu)
[environment] secrets (list of env var names from the keychain), vars (table)
[artifacts] output_dir (default "./outputs"; programs get it as HOMEMADE_OUTPUT_DIR)
[inputs.<name>] type ("file"|"directory"|"file_list"|"string"|"int"|"float"|"bool"|"select"|"range"|"args"), cli_flag ("--x") or positional_index (0..), label, description, required, default, options (select), extensions (file), min/max/step (range/int/float), bool_style ("flag_only"|"key_value")
Modes: cli = one-shot program per run; rpc_worker = long-lived process using homemade_worker (@expose + run_worker()); webapp = a server; Homemade embeds http://127.0.0.1:<port> after the program prints "@homemade:port <port>".
Stdout directives (one per line, flushed): @homemade:progress <0-100> "msg" | @homemade:artifact "<abs path>" | @homemade:metric {json} | @homemade:port <n>"#;

const RULES: &str = r#"You write Homemade launcher manifests. Reply with ONE JSON object only, no prose, with keys app, ui, resources, environment, artifacts, inputs (same structure as the TOML tables).
Rules:
- mode: "cli" for one-shot scripts; "rpc_worker" only if the repo uses homemade_worker; "webapp" when the entry starts a server (gradio/streamlit/flask/fastapi/http.server). For webapp, if the server does not print the port itself, exec must be: sh -c 'echo "@homemade:port <PORT>"; exec <cmd> --port <PORT> "$@"' app
- env_manager: "uv" if pyproject.toml/requirements and no .venv; "byo" if an existing .venv; "system" for dependency-free scripts; "pnpm"/"cargo" by lockfile. With uv, exec starts with "uv run".
- resources.requires_gpu = true only when deps include torch/mlx/cuda/tensorflow.
- inputs: one entry per CLI flag of the chosen entry script, with type, cli_flag or positional_index, label, default. Do not invent flags. At most 8 inputs; prefer the ones the README documents.
- Choose ONE entry: if the README presents a GUI/web server or an entry is marked "server", use it with mode "webapp"; otherwise the main CLI (console script or argparse entry).
- Output minified JSON on a single line. Keep descriptions under 60 characters."#;

fn system_prompt() -> String { format!("{RULES}\n\n{MANIFEST_REFERENCE}") }

/// Normalize loose model JSON into something `HomemadeManifest` deserializes, filling gaps from `base`.
pub fn normalize(v: &Value, base: Option<&HomemadeManifest>, folder: &str) -> Value { normalize_with(v, base, folder, false) }

fn normalize_with(v: &Value, base: Option<&HomemadeManifest>, folder: &str, prefer_positional: bool) -> Value {
    let mut v = match v { Value::Object(_) => v.clone(), Value::Array(a) => a.iter().find(|x| x.is_object()).cloned().unwrap_or(json!({})), _ => json!({}) };
    // Some models wrap the whole thing: {"manifest": {...}} / {"homemade": {...}}
    for k in ["manifest", "homemade", "homemade.toml"] {
        if let Some(inner) = v.get(k).filter(|x| x.is_object()).cloned() { if inner.get("app").is_some() { v = inner; } }
    }
    let mut o = v.as_object().cloned().unwrap_or_default();
    // Small models often nest the other tables inside "app": hoist them.
    if let Some(app) = o.get("app").and_then(|x| x.as_object()).cloned() {
        for k in ["ui", "resources", "environment", "artifacts", "inputs"] {
            if !o.contains_key(k) { if let Some(x) = app.get(k) { o.insert(k.into(), x.clone()); } }
        }
    }
    let base_v = base.map(|b| serde_json::to_value(b).unwrap_or_default()).unwrap_or(json!({}));
    let bget = |sec: &str, k: &str| base_v.get(sec).and_then(|s| s.get(k)).cloned();
    // A runnable base (existing manifest, static inference, or the facts-derived fallback) decides HOW the program runs
    // (exec/mode/env_manager/env vars); the small model only contributes descriptive fields. It is not reliable at commands.
    let base_runs = base.map(|b| !b.app.exec.trim().is_empty()).unwrap_or(false);

    // app (top-level app keys are tolerated)
    let mut app_in = o.get("app").and_then(|x| x.as_object()).cloned().unwrap_or_default();
    for k in ["id", "name", "description", "mode", "exec", "env_manager", "version", "cwd"] {
        if !app_in.contains_key(k) { if let Some(x) = o.get(k) { app_in.insert(k.into(), x.clone()); } }
    }
    let mut app = Map::new();
    let s = |x: Option<&Value>| x.and_then(as_string).filter(|s| !s.trim().is_empty());
    let name = s(app_in.get("name")).or_else(|| bget("app", "name").and_then(|x| as_string(&x))).unwrap_or_else(|| folder.to_string());
    let id = s(app_in.get("id")).map(|x| crate::manifest::infer::slugify(&x))
        .or_else(|| bget("app", "id").and_then(|x| as_string(&x))).unwrap_or_else(|| crate::manifest::infer::slugify(folder));
    app.insert("id".into(), json!(id));
    app.insert("name".into(), json!(name));
    if let Some(d) = s(app_in.get("description")) { app.insert("description".into(), json!(d)); }
    let mode = if base_runs { bget("app", "mode").and_then(|x| as_string(&x)).unwrap_or_else(|| "cli".into()) } else {
        s(app_in.get("mode")).map(|m| m.to_lowercase().replace(['-', ' '], "_")).map(|m| match m.as_str() {
            "rpc_worker" | "rpc" | "worker" => "rpc_worker", "webapp" | "web" | "server" | "web_app" => "webapp", _ => "cli",
        }.to_string()).unwrap_or_else(|| "cli".into())
    };
    app.insert("mode".into(), json!(mode));
    let mut exec = if base_runs { bget("app", "exec").and_then(|x| as_string(&x)).unwrap_or_default() }
        else { s(app_in.get("exec")).or_else(|| bget("app", "exec").and_then(|x| as_string(&x))).unwrap_or_default() };
    // macOS has no bare `python`; outside uv/byo envs use python3.
    if exec.starts_with("python ") && !app_in.get("env_manager").and_then(as_string).map(|e| e == "byo").unwrap_or(false) {
        exec = format!("python3{}", &exec["python".len()..]);
    }
    app.insert("exec".into(), json!(exec));
    let em = if base_runs { bget("app", "env_manager").and_then(|x| as_string(&x)).unwrap_or_else(|| "system".into()) } else {
        s(app_in.get("env_manager")).map(|x| x.to_lowercase()).filter(|x| ["uv", "pnpm", "cargo", "byo", "system"].contains(&x.as_str()))
            .or_else(|| bget("app", "env_manager").and_then(|x| as_string(&x))).unwrap_or_else(|| "system".into())
    };
    app.insert("env_manager".into(), json!(em));
    if let Some(c) = s(app_in.get("cwd")) { app.insert("cwd".into(), json!(c)); }

    // ui
    let mut ui = Map::new();
    if let Some(u) = o.get("ui").and_then(|x| x.as_object()) {
        if let Some(e) = s(u.get("entry")) { ui.insert("entry".into(), json!(e)); }
        if let Some(l) = s(u.get("default_layout")).filter(|l| ["tab", "split_right", "window"].contains(&l.as_str())) { ui.insert("default_layout".into(), json!(l)); }
    }

    // resources
    let mut res = Map::new();
    if let Some(r) = o.get("resources").and_then(|x| x.as_object()) {
        if let Some(b) = r.get("requires_gpu").and_then(as_bool) { res.insert("requires_gpu".into(), json!(b)); }
        if let Some(b) = r.get("isolated_cache").and_then(as_bool) { res.insert("isolated_cache".into(), json!(b)); }
        for k in ["vram_estimate_mb", "max_ram_mb", "graceful_timeout_ms", "kill_timeout_ms"] {
            let positive_only = k == "vram_estimate_mb" || k == "max_ram_mb";
            if let Some(n) = r.get(k).and_then(as_f64).filter(|n| if positive_only { *n > 0.0 } else { *n >= 0.0 }) { res.insert(k.into(), json!(n as u64)); }
        }
    }

    // resources.requires_gpu: never let the model turn off what the deps say
    if bget("resources", "requires_gpu").and_then(|x| x.as_bool()) == Some(true) { res.insert("requires_gpu".into(), json!(true)); }

    // environment: env vars are hallucination-prone, so with a runnable base they come from the base only
    let mut env = Map::new();
    if base_runs {
        if let Some(e) = base_v.get("environment").and_then(|x| x.as_object()) {
            env.insert("secrets".into(), e.get("secrets").cloned().unwrap_or(json!([])));
            if let Some(vs) = e.get("vars").filter(|x| x.is_object()) { env.insert("vars".into(), vs.clone()); }
        }
    } else if let Some(e) = o.get("environment").and_then(|x| x.as_object()) {
        env.insert("secrets".into(), json!(str_list(e.get("secrets"))));
        if let Some(vs) = e.get("vars").and_then(|x| x.as_object()) {
            let m: Map<String, Value> = vs.iter().filter_map(|(k, x)| as_string(x).map(|s| (k.clone(), json!(s)))).collect();
            env.insert("vars".into(), Value::Object(m));
        }
    }

    // artifacts
    let mut art = Map::new();
    if let Some(a) = o.get("artifacts").and_then(|x| x.as_object()) {
        if let Some(d) = s(a.get("output_dir")) { art.insert("output_dir".into(), json!(d)); }
        art.insert("scratch_globs".into(), json!(str_list(a.get("scratch_globs"))));
    }

    // inputs: object, or array of {name, ...}
    let mut inputs = Map::new();
    let raw_inputs: Vec<(String, Value)> = match o.get("inputs") {
        Some(Value::Object(m)) => m.iter().map(|(k, x)| (k.clone(), x.clone())).collect(),
        Some(Value::Array(a)) => a.iter().filter_map(|x| {
            let n = x.get("name").or_else(|| x.get("key")).and_then(as_string)
                .or_else(|| x.get("cli_flag").or_else(|| x.get("flag")).and_then(as_string))
                .or_else(|| x.get("label").and_then(as_string).map(|l| l.to_lowercase().replace(' ', "_")))?;
            Some((n, x.clone()))
        }).collect(),
        _ => vec![],
    };
    for (k, x) in raw_inputs {
        if let Some(spec) = normalize_input(&x, prefer_positional) {
            let key = k.trim().trim_start_matches('-').replace('-', "_");
            if !key.is_empty() { inputs.insert(key, spec); }
        }
    }
    // Inputs found by the static inspector (or an existing manifest) are authoritative for how values reach the program;
    // the model only contributes labels/descriptions/ranges there, and cannot add flags the inspector did not see.
    if let Some(b) = base_v.get("inputs").and_then(|x| x.as_object()).filter(|b| b.keys().any(|k| k != "extra")) {
        let mut merged = Map::new();
        for (k, bs) in b {
            let mut spec = bs.as_object().cloned().unwrap_or_default();
            if let Some(ms) = inputs.get(k).and_then(|x| x.as_object()) {
                for f in ["label", "description", "min", "max", "step"] {
                    if !spec.get(f).map(|x| !x.is_null()).unwrap_or(false) { if let Some(x) = ms.get(f) { spec.insert(f.into(), x.clone()); } }
                }
            }
            spec.retain(|_, x| !x.is_null());
            merged.insert(k.clone(), Value::Object(spec));
        }
        inputs = merged;
    } else if inputs.is_empty() {
        if let Some(b) = base_v.get("inputs").and_then(|x| x.as_object()) { inputs = b.clone(); }
    }

    json!({ "app": app, "ui": ui, "resources": res, "environment": env, "artifacts": art, "inputs": inputs })
}

fn normalize_input(x: &Value, prefer_positional: bool) -> Option<Value> {
    let o = x.as_object()?;
    let mut m = Map::new();
    let t = o.get("type").and_then(as_string).unwrap_or_else(|| "string".into()).to_lowercase();
    let t = match t.as_str() {
        "file" | "path" | "filepath" | "file_path" => "file",
        "directory" | "dir" | "folder" => "directory",
        "file_list" | "files" | "list[file]" => "file_list",
        "int" | "integer" => "int",
        "float" | "number" | "double" => "float",
        "bool" | "boolean" | "flag" => "bool",
        "select" | "choice" | "enum" | "choices" => "select",
        "range" | "slider" => "range",
        "args" => "args",
        _ => "string",
    };
    m.insert("type".into(), json!(t));
    let pos = o.get("positional_index").and_then(as_f64).filter(|n| *n >= 0.0);
    if let (true, Some(i)) = (prefer_positional, pos) {
        m.insert("positional_index".into(), json!(i as usize));
    } else if let Some(f) = o.get("cli_flag").or_else(|| o.get("flag")).and_then(as_string).filter(|f| !f.trim().is_empty()) {
        let f = f.trim().to_string();
        m.insert("cli_flag".into(), json!(if f.starts_with('-') { f } else { format!("--{f}") }));
    } else if let Some(i) = pos {
        m.insert("positional_index".into(), json!(i as usize));
    }
    for k in ["label", "description"] {
        if let Some(sv) = o.get(k).and_then(as_string).filter(|s| !s.is_empty()) { m.insert(k.into(), json!(sv)); }
    }
    if let Some(b) = o.get("required").and_then(as_bool) { m.insert("required".into(), json!(b)); }
    if let Some(d) = o.get("default").filter(|d| !d.is_null() && d.as_str() != Some("")) {
        let d = match t {
            "int" => as_f64(d).map(|n| json!(n as i64)),
            "float" | "range" => as_f64(d).map(|n| json!(n)),
            "bool" => as_bool(d).map(|b| json!(b)),
            _ => match d { Value::Array(_) | Value::Object(_) => Some(d.clone()), _ => as_string(d).map(|s| json!(s)) },
        };
        if let Some(d) = d { m.insert("default".into(), d); }
    }
    let opts = str_list(o.get("options").or_else(|| o.get("choices")));
    if !opts.is_empty() { m.insert("options".into(), json!(opts)); }
    let exts = str_list(o.get("extensions"));
    if !exts.is_empty() { m.insert("extensions".into(), json!(exts.into_iter().map(|e| if e.starts_with('.') { e } else { format!(".{e}") }).collect::<Vec<_>>())); }
    if matches!(t, "int" | "float" | "range") {
        for k in ["min", "max", "step"] {
            if let Some(n) = o.get(k).and_then(as_f64) { m.insert(k.into(), json!(n)); }
        }
    }
    if let Some(bs) = o.get("bool_style").and_then(as_string).filter(|b| t == "bool" && (b == "flag_only" || b == "key_value")) { m.insert("bool_style".into(), json!(bs)); }
    Some(Value::Object(m))
}

fn as_string(v: &Value) -> Option<String> {
    match v { Value::String(s) => Some(s.clone()), Value::Number(n) => Some(n.to_string()), Value::Bool(b) => Some(b.to_string()), _ => None }
}
fn as_f64(v: &Value) -> Option<f64> {
    match v { Value::Number(n) => n.as_f64(), Value::String(s) => s.trim().parse().ok(), _ => None }
}
fn as_bool(v: &Value) -> Option<bool> {
    match v { Value::Bool(b) => Some(*b), Value::String(s) => match s.trim().to_lowercase().as_str() { "true" | "yes" | "1" => Some(true), "false" | "no" | "0" => Some(false), _ => None }, Value::Number(n) => n.as_f64().map(|x| x != 0.0), _ => None }
}
fn str_list(v: Option<&Value>) -> Vec<String> {
    match v { Some(Value::Array(a)) => a.iter().filter_map(as_string).collect(), Some(Value::String(s)) if !s.is_empty() => s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect(), _ => vec![] }
}

/// Inputs-only normalization for `infer_schema_from_raw_code`.
pub fn normalize_inputs(v: &Value) -> indexmap::IndexMap<String, InputFieldSpec> {
    // Raw code without a CLI framework reads sys.argv, so positional indices win over invented flags.
    let n = normalize_with(&json!({ "app": { "id": "x", "exec": "x" }, "inputs": v.get("inputs").cloned().unwrap_or_else(|| v.clone()) }), None, "x", true);
    serde_json::from_value(n["inputs"].clone()).unwrap_or_default()
}

/// JSON parse that tolerates small-model slips: Python literals (None/True/False) and trailing commas.
pub fn parse_lenient(text: &str) -> std::result::Result<Value, serde_json::Error> {
    let t = text.trim();
    if let Ok(v) = serde_json::from_str(t) { return Ok(v); }
    match parse_lenient_inner(t) {
        Ok(v) => Ok(v),
        Err(e) => match repair_truncated(t) { Some(r) => parse_lenient_inner(&r).map_err(|_| e), None => Err(e) },
    }
}

/// The model ran out of tokens mid-object: cut back to the last completed value at nesting depth <= 2
/// (i.e. a whole `inputs.<name>` entry or a whole top-level section) and close what is still open.
pub fn repair_truncated(t: &str) -> Option<String> {
    let chars: Vec<char> = t.chars().collect();
    let start = chars.iter().position(|c| *c == '{')?;
    let (mut in_str, mut esc) = (false, false);
    let mut stack: Vec<char> = vec![];
    let mut last_good: Option<(usize, Vec<char>)> = None; // (cut index, open stack at that point)
    for (i, &c) in chars.iter().enumerate().skip(start) {
        if in_str {
            if esc { esc = false } else if c == '\\' { esc = true } else if c == '"' { in_str = false }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' | '[' => stack.push(c),
            '}' | ']' => {
                stack.pop();
                if stack.len() <= 2 { last_good = Some((i + 1, stack.clone())); }
                if stack.is_empty() { return None; } // it was complete; nothing to repair
            }
            _ => {}
        }
    }
    let (cut, open) = last_good?;
    let mut out: String = chars[..cut].iter().collect();
    let trimmed = out.trim_end().trim_end_matches(',').to_string();
    out = trimmed;
    for c in open.iter().rev() { out.push(if *c == '{' { '}' } else { ']' }); }
    Some(out)
}

fn parse_lenient_inner(t: &str) -> std::result::Result<Value, serde_json::Error> {
    let mut out = String::with_capacity(t.len());
    let (mut in_str, mut esc) = (false, false);
    let chars: Vec<char> = t.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if in_str {
            out.push(c);
            if esc { esc = false } else if c == '\\' { esc = true } else if c == '"' { in_str = false }
            i += 1; continue;
        }
        if c == '"' { in_str = true; out.push(c); i += 1; continue; }
        if c == ',' {
            let next = chars[i + 1..].iter().find(|x| !x.is_whitespace());
            if matches!(next, Some('}') | Some(']')) { i += 1; continue; }
        }
        let word_start = i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_');
        let mut replaced = false;
        if word_start {
            for (w, r) in [("None", "null"), ("True", "true"), ("False", "false")] {
                let wl = w.len();
                if chars[i..].iter().take(wl).collect::<String>() == w && chars.get(i + wl).map(|x| !(x.is_alphanumeric() || *x == '_')).unwrap_or(true) {
                    out.push_str(r); i += wl; replaced = true; break;
                }
            }
        }
        if !replaced { out.push(c); i += 1; }
    }
    serde_json::from_str(&out)
}

/// Semantic checks beyond serde.
pub fn validate(m: &HomemadeManifest) -> Vec<String> {
    let mut e = vec![];
    if m.app.id.trim().is_empty() { e.push("app.id is empty".into()); }
    if m.app.exec.trim().is_empty() { e.push("app.exec is empty".into()); }
    else if let Err(x) = shell_words::split(&m.app.exec) { e.push(format!("app.exec is not a valid command line: {x}")); }
    const TYPES: [&str; 10] = ["file", "directory", "file_list", "string", "int", "float", "bool", "select", "range", "args"];
    for (k, i) in &m.inputs {
        if !TYPES.contains(&i.field_type.as_str()) { e.push(format!("inputs.{k}.type '{}' is not one of {}", i.field_type, TYPES.join("|"))); }
        if i.field_type == "select" && i.options.as_ref().map(|o| o.is_empty()).unwrap_or(true) { e.push(format!("inputs.{k}: select needs options")); }
        if i.field_type != "args" && i.cli_flag.is_none() && i.positional_index.is_none() { e.push(format!("inputs.{k}: needs cli_flag or positional_index")); }
    }
    e
}

/// JSON text from the model -> (toml, errors).
pub fn json_to_toml(text: &str, base: Option<&HomemadeManifest>, folder: &str) -> std::result::Result<String, String> {
    let v: Value = parse_lenient(text).map_err(|e| format!("not valid JSON: {e}"))?;
    let n = normalize(&v, base, folder);
    let m: HomemadeManifest = serde_json::from_value(n).map_err(|e| format!("does not match the manifest structure: {e}"))?;
    let errs = validate(&m);
    if !errs.is_empty() { return Err(errs.join("; ")); }
    let t = m.to_toml();
    HomemadeManifest::parse(&t).map_err(|e| format!("TOML round-trip failed: {e}"))?;
    Ok(t)
}

pub async fn draft_manifest(model: &LocalModel, root: &Path, res_dir: &Path) -> Result<DraftResult> {
    let (r2, d2) = (root.to_path_buf(), res_dir.to_path_buf());
    let (facts, base) = tokio::task::spawn_blocking(move || {
        let facts = repo_facts::collect(&r2, &d2);
        // Base = existing homemade.toml when present, else the deterministic inference.
        let base = crate::manifest::infer::load_or_infer(&r2, &d2).map(|x| x.0).map_err(|e| e.to_string());
        (facts, base)
    }).await.map_err(|e| crate::error::Error::Msg(e.to_string()))?;
    let folder = root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "app".into());
    let mut notes = vec![];
    let mut base_m = base.as_ref().ok().cloned();
    if let Err(e) = &base {
        notes.push(format!("Deterministic inference: {e}"));
        if let Some(m) = fallback_from_facts(&facts, &folder) {
            notes.push(format!("Fallback entry chosen from repository facts: {}", m.app.exec));
            base_m = Some(m);
        }
    }

    let mut toml_out: Option<String> = None;
    let mut used_model = false;
    let status = model.status().await;
    if status.available {
        let sys = system_prompt();
        let mut prompt = format!("Repository facts:\n{}\n\nReturn the JSON manifest.", facts.to_prompt_text_max(6000));
        for attempt in 0..2 {
            match model.complete(&sys, &prompt, true, 2048).await {
                Ok(text) => match { if std::env::var_os("HOMEMADE_DEBUG_MODEL").is_some() { eprintln!("[draft] model output:\n{text}"); } json_to_toml(&text, base_m.as_ref(), &folder) } {
                    Ok(t) => { toml_out = Some(t); used_model = true; break; }
                    Err(e) => {
                        notes.push(format!("Model attempt {} rejected: {e}", attempt + 1));
                        prompt = format!("Repository facts:\n{}\n\nYour previous answer:\n{}\n\nIt was invalid: {e}\nReturn a corrected JSON manifest only.",
                            facts.to_prompt_text_max(3500), text.chars().take(1500).collect::<String>());
                    }
                },
                Err(e) => {
                    notes.push(format!("Model attempt {} failed: {e}", attempt + 1));
                    prompt = format!("Repository facts:\n{}\n\nReturn the JSON manifest.", facts.to_prompt_text_max(3000));
                }
            }
        }
    } else {
        notes.push(format!("Local model unavailable ({}); using deterministic inference.", status.reason.clone().unwrap_or_default()));
    }
    let toml = match toml_out {
        Some(t) => t,
        None => {
            if status.available { notes.push("Falling back to deterministic inference.".into()); }
            base_m.as_ref().map(|m| m.to_toml()).unwrap_or_else(|| format!("[app]\nid = \"{}\"\nname = \"{folder}\"\nmode = \"cli\"\nexec = \"\"\nenv_manager = \"system\"\n", crate::manifest::infer::slugify(&folder)))
        }
    };
    let validation_errors = match HomemadeManifest::parse(&toml) { Ok(m) => validate(&m), Err(e) => vec![e.to_string()] };
    Ok(DraftResult { agent_prompt: agent_prompt(root, &facts, &toml), facts_summary: facts.summary(), toml, validation_errors, used_model, notes })
}

/// When `load_or_infer` finds no obvious entry (no main.py, several scripts), build a base manifest from the
/// collected facts: a server entry (webapp with the port wrapper) > an argparse/typer/click CLI > a console script.
pub fn fallback_from_facts(facts: &RepoFacts, folder: &str) -> Option<HomemadeManifest> {
    let s = &facts.signals;
    let (py, env_manager) = if s.has_venv { (".venv/bin/python".to_string(), "byo") }
        else if s.uses_uv { ("uv run python".to_string(), "uv") } else { ("python3".to_string(), "system") };
    let gpu = !s.gpu_hints.is_empty();
    let mut v = json!({ "app": { "id": crate::manifest::infer::slugify(folder), "name": crate::manifest::infer::title_case(folder), "env_manager": env_manager },
                        "resources": { "requires_gpu": gpu }, "inputs": {} });
    let inputs_json = |e: &crate::intelligence::repo_facts::EntryCandidate, skip_port: bool| -> Value {
        let mut m = serde_json::Map::new();
        for (k, i) in &e.inputs {
            if skip_port && matches!(i.cli_flag.as_deref(), Some("--port") | Some("--host")) { continue; }
            if let Ok(x) = serde_json::to_value(i) { m.insert(k.clone(), x); }
        }
        Value::Object(m)
    };
    if let Some(e) = facts.entry_candidates.iter().find(|e| e.server) {
        let has_port_flag = e.inputs.values().any(|i| i.cli_flag.as_deref() == Some("--port"));
        let port = 8765;
        let exec = if has_port_flag {
            format!("sh -c 'echo \"@homemade:port {port}\"; exec {py} {} --port {port} \"$@\"' app", e.path)
        } else {
            format!("sh -c 'echo \"@homemade:port {port}\"; exec {py} {} \"$@\"' app", e.path)
        };
        v["app"]["mode"] = json!("webapp"); v["app"]["exec"] = json!(exec);
        v["inputs"] = inputs_json(e, true);
    } else if let Some(e) = facts.entry_candidates.iter().find(|e| matches!(e.framework.as_str(), "argparse" | "typer" | "click")) {
        v["app"]["mode"] = json!("cli"); v["app"]["exec"] = json!(format!("{py} {}", e.path));
        v["inputs"] = inputs_json(e, false);
    } else if let Some((name, _)) = facts.console_scripts.first() {
        let exec = if s.has_venv { format!(".venv/bin/{name}") } else if s.uses_uv { format!("uv run {name}") } else { name.clone() };
        v["app"]["mode"] = json!("cli"); v["app"]["exec"] = json!(exec);
        v["inputs"] = json!({ "extra": { "type": "args", "label": "Arguments" } });
    } else if let Some(e) = facts.entry_candidates.first() {
        v["app"]["mode"] = json!("cli"); v["app"]["exec"] = json!(format!("{py} {}", e.path));
        v["inputs"] = json!({ "extra": { "type": "args", "label": "Arguments" } });
    } else { return None; }
    let n = normalize(&v, None, folder);
    serde_json::from_value(n).ok()
}

pub fn agent_prompt(root: &Path, facts: &RepoFacts, toml: &str) -> String {
    format!(r#"# Adapt `{root}` to Homemade

Homemade is a local launcher: it reads `homemade.toml` in the project root, renders a form from `[inputs]`, runs `exec` from the root,
streams stdout, and shows files written to `HOMEMADE_OUTPUT_DIR`. Make this repository run well under it.

## Manifest reference
{MANIFEST_REFERENCE}

## Repository facts
{facts}

## Draft homemade.toml
```toml
{toml}
```

## Checklist
- [ ] Verify `exec` runs from the repo root as written (dependencies install with the chosen env_manager; no new standalone venv).
- [ ] Confirm every `[inputs]` entry matches a real CLI flag/positional (types, defaults, required) and nothing is missing.
- [ ] Decide `mode` (cli / rpc_worker / webapp); for webapp make sure `@homemade:port <n>` is printed.
- [ ] Write outputs to `$HOMEMADE_OUTPUT_DIR` and add `@homemade:progress` / `@homemade:artifact` prints (flush=True) where useful.
- [ ] Save `homemade.toml`, then validate it with Homemade's MCP tool `validate_manifest` (`homemade --mcp`) and run it once from Homemade.
"#, root = root.display(), facts = facts.to_prompt_text())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repairs_truncated_json() {
        let t = r#"{"app":{"id":"x","exec":"python3 a.py"},"inputs":{"a":{"type":"int","cli_flag":"--a"},"b":{"type":"string","cli_flag":"--b","label":"unterminated"#;
        let r = repair_truncated(t).expect("repairable");
        let v: Value = serde_json::from_str(&r).expect("valid after repair");
        assert!(v["inputs"]["a"].is_object());
        assert!(v["inputs"].get("b").is_none());
        assert!(repair_truncated(r#"{"a":1}"#).is_none());
    }

    #[test]
    fn model_json_to_valid_toml() {
        let text = r#"{
          "app": {"name": "Transcriber", "mode": "CLI", "exec": "uv run python transcribe.py", "env_manager": "UV", "bogus": 1},
          "resources": {"requires_gpu": "true", "vram_estimate_mb": "4000"},
          "artifacts": {"output_dir": "./outputs"},
          "environment": {"secrets": "OPENAI_API_KEY"},
          "inputs": {
            "input": {"type": "path", "positional_index": "0", "required": "yes", "extensions": ["wav", ".mp3"]},
            "model": {"type": "choice", "flag": "model", "choices": ["tiny", "base"], "default": "base"},
            "beam-size": {"type": "integer", "cli_flag": "--beam-size", "default": "5", "min": "1", "max": 10},
            "verbose": {"type": "boolean", "cli_flag": "--verbose", "bool_style": "flag_only", "default": "false"}
          },
          "extra_top_level": true
        }"#;
        let t = json_to_toml(text, None, "my-tool").expect("valid");
        let m = HomemadeManifest::parse(&t).unwrap();
        assert!(validate(&m).is_empty());
        assert_eq!(m.app.id, "my-tool");
        assert_eq!(m.app.env_manager, "uv");
        assert!(m.resources.requires_gpu);
        assert_eq!(m.resources.vram_estimate_mb, Some(4000));
        assert_eq!(m.environment.secrets, vec!["OPENAI_API_KEY"]);
        let i = &m.inputs["input"];
        assert_eq!((i.field_type.as_str(), i.positional_index, i.required), ("file", Some(0), Some(true)));
        assert_eq!(i.extensions.as_ref().unwrap(), &vec![".wav".to_string(), ".mp3".to_string()]);
        assert_eq!(m.inputs["model"].cli_flag.as_deref(), Some("--model"));
        assert_eq!(m.inputs["model"].options.as_ref().unwrap().len(), 2);
        assert_eq!(m.inputs["beam_size"].default, Some(json!(5)));
        assert_eq!(m.inputs["verbose"].default, Some(json!(false)));
    }

    #[test]
    fn fills_from_base_and_rejects_garbage() {
        let base = HomemadeManifest::parse("[app]\nid = \"hello\"\nname = \"Hello\"\nexec = \"python3 main.py\"\nenv_manager = \"system\"\n").unwrap();
        let t = json_to_toml(r#"{"name": "Hi", "mode": "server"}"#, Some(&base), "hello").unwrap();
        let m = HomemadeManifest::parse(&t).unwrap();
        assert_eq!(m.app.exec, "python3 main.py");
        assert_eq!(m.app.name, "Hi");
        assert_eq!(m.app.mode.as_str(), "cli", "a runnable base decides mode/exec; the model only adds descriptive fields");
        let no_base = HomemadeManifest::parse(&json_to_toml(r#"{"name": "Hi", "mode": "server", "exec": "python3 serve.py"}"#, None, "hello").unwrap()).unwrap();
        assert_eq!(no_base.app.mode.as_str(), "webapp");
        assert!(json_to_toml("not json", None, "x").is_err());
        let py = r#"{"app": {"exec": "python3 a.py"}, "inputs": {"n": {"type": "int", "cli_flag": "--n", "max": None, "required": True,},},}"#;
        let m = HomemadeManifest::parse(&json_to_toml(py, None, "a").unwrap()).unwrap();
        assert_eq!(m.inputs["n"].required, Some(true));
        assert_eq!(m.app.exec, "python3 a.py");
        let i = normalize_inputs(&json!({"inputs": {"img": {"type": "file", "cli_flag": "--img", "positional_index": 0}}}));
        assert_eq!((i["img"].positional_index, i["img"].cli_flag.clone()), (Some(0), None));
        assert!(json_to_toml(r#"{"app": {"name": "x"}}"#, None, "x").unwrap_err().contains("exec"));
    }

    #[test]
    fn hoists_nested_tables_and_names_array_inputs() {
        let text = r#"{"app": {"name": "greet", "exec": "python greet.py", "resources": {"requires_gpu": false},
            "inputs": [{"type": "int", "cli_flag": "--limit", "default": 10, "bool_style": "key_value"}, {"type": "bool", "cli_flag": "--verbose", "default": "", "bool_style": "flag_only"}]}}"#;
        let m = HomemadeManifest::parse(&json_to_toml(text, None, "greet").unwrap()).unwrap();
        assert_eq!(m.inputs["limit"].default, Some(json!(10)));
        assert_eq!(m.inputs["limit"].bool_style, None);
        assert_eq!(m.inputs["verbose"].bool_style.as_deref(), Some("flag_only"));
        assert_eq!(m.inputs["verbose"].default, None);
        // inspector inputs win over model guesses
        let base = HomemadeManifest::parse("[app]\nid = \"g\"\nname = \"G\"\nexec = \"python3 g.py\"\n[inputs.limit]\ntype = \"int\"\ncli_flag = \"--limit\"\ndefault = 5\n").unwrap();
        let m = HomemadeManifest::parse(&json_to_toml(text, Some(&base), "g").unwrap()).unwrap();
        assert_eq!(m.inputs.len(), 1);
        assert_eq!(m.inputs["limit"].default, Some(json!(5)));
    }

    #[tokio::test]
    async fn draft_without_model_uses_inference() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../examples/hello-cli");
        let res = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources");
        let d = draft_manifest(&LocalModel::disabled(), &root, &res).await.unwrap();
        assert!(!d.used_model);
        assert!(HomemadeManifest::parse(&d.toml).is_ok());
        assert!(d.agent_prompt.contains("Adapt") && d.agent_prompt.contains("validate_manifest"));
    }
}
