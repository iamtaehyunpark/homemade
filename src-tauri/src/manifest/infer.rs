//! Load homemade.toml from an app folder, or infer one (ast_inspector.py) when absent.
//! OWNER: agent B.
use crate::error::{err, Result};
use crate::manifest::schema::*;
use indexmap::IndexMap;
use std::path::{Path, PathBuf};

/// Returns (manifest, manifest_hash, was_inferred).
/// 1. If `<root>/homemade.toml` exists: parse it; hash = sha256(file bytes).
/// 2. Else: find the entry script (main.py / app.py / cli.py / sole *.py / package.json "main"), run
///    `python3 -I <resources>/ast_inspector.py <script>` with a 10s timeout, build a manifest with
///    exec = "uv run python <script>" (or "python3 <script>" if no pyproject/uv), inputs from inspector JSON.
///    hash = sha256(script bytes). Never execute the user's script.
pub fn load_or_infer(root: &Path, resources_dir: &Path) -> Result<(HomemadeManifest, String, bool)> {
    let toml_path = root.join("homemade.toml");
    if toml_path.is_file() {
        let bytes = std::fs::read(&toml_path)?;
        let src = String::from_utf8_lossy(&bytes);
        let m = HomemadeManifest::parse(&src)
            .map_err(|e| crate::error::Error::Msg(format!("Invalid homemade.toml: {e}")))?;
        return Ok((m, sha256_hex(&bytes), false));
    }

    let folder = root.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "app".into());
    let id = slugify(&folder);
    let name = title_case(&folder);

    let mut inputs: IndexMap<String, InputFieldSpec> = IndexMap::new();
    let (exec, env_manager, hash);
    if let Some(script) = find_entry_script(root) {
        let rel = script.file_name().unwrap().to_string_lossy().to_string();
        let bytes = std::fs::read(&script)?;
        hash = sha256_hex(&bytes);
        inputs = run_inspector(&script, resources_dir).unwrap_or_default();
        let has_uv = ["pyproject.toml", "uv.lock", "requirements.txt"].iter().any(|f| root.join(f).exists());
        let q = shell_words::quote(&rel).to_string();
        if has_uv {
            exec = format!("uv run python {q}");
            env_manager = "uv".to_string();
        } else {
            exec = format!("python3 {q}");
            env_manager = "system".to_string();
        }
    } else if root.join("package.json").is_file() {
        let bytes = std::fs::read(root.join("package.json"))?;
        let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
        let has_start = v.pointer("/scripts/start").and_then(|x| x.as_str()).is_some();
        let main = v.get("main").and_then(|x| x.as_str());
        if !has_start && main.is_none() {
            return err("No runnable entry found");
        }
        hash = sha256_hex(&bytes);
        exec = if has_start { "pnpm start".to_string() } else { format!("node {}", shell_words::quote(main.unwrap())) };
        env_manager = "pnpm".to_string();
    } else {
        return err("No runnable entry found");
    }

    if inputs.is_empty() {
        inputs.insert("extra".into(), InputFieldSpec {
            field_type: "args".into(), cli_flag: None, positional_index: None,
            label: Some("Extra arguments".into()), description: None, required: Some(false),
            default: None, options: None, extensions: None, min: None, max: None, step: None, bool_style: None,
        });
    }

    let m = HomemadeManifest {
        app: AppConfig {
            id, name, version: "0.1.0".into(), description: None, mode: ExecutionMode::Cli, exec,
            cwd: ".".into(), env_manager,
        },
        ui: UiConfig { entry: None, default_layout: "tab".into() },
        resources: Default::default(),
        environment: Default::default(),
        artifacts: ArtifactConfig { output_dir: "./outputs".into(), scratch_globs: vec![] },
        inputs,
    };
    Ok((m, hash, true))
}

fn find_entry_script(root: &Path) -> Option<PathBuf> {
    for n in ["main.py", "app.py", "cli.py", "__main__.py"] {
        let p = root.join(n);
        if p.is_file() { return Some(p); }
    }
    let pys: Vec<PathBuf> = std::fs::read_dir(root).ok()?
        .filter_map(|e| e.ok()).map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().map(|x| x == "py").unwrap_or(false))
        .collect();
    if pys.len() == 1 { pys.into_iter().next() } else { None }
}

fn run_inspector(script: &Path, resources_dir: &Path) -> Option<IndexMap<String, InputFieldSpec>> {
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};
    let inspector = resources_dir.join("ast_inspector.py");
    let mut child = Command::new("python3").arg("-I").arg(&inspector).arg(script)
        .stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().ok()?;
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if start.elapsed() > Duration::from_secs(10) => { let _ = child.kill(); let _ = child.wait(); return None; }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(_) => return None,
        }
    }
    let out = child.wait_with_output().ok()?;
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).ok()?;
    serde_json::from_value(v.get("inputs")?.clone()).ok()
}

/// Lowercase `[a-z0-9-]` slug.
pub fn slugify(s: &str) -> String {
    let mut out = String::new();
    for c in s.chars() {
        let c = c.to_ascii_lowercase();
        if c.is_ascii_alphanumeric() { out.push(c); } else if !out.ends_with('-') { out.push('-'); }
    }
    let t = out.trim_matches('-').to_string();
    if t.is_empty() { "app".into() } else { t }
}

fn title_case(s: &str) -> String {
    s.split(|c: char| c == '-' || c == '_' || c.is_whitespace()).filter(|w| !w.is_empty())
        .map(|w| { let mut c = w.chars(); match c.next() { Some(f) => f.to_uppercase().collect::<String>() + c.as_str(), None => String::new() } })
        .collect::<Vec<_>>().join(" ")
}

/// Write an inferred manifest to `<root>/homemade.toml` so the agent/user can edit it later.
pub fn write_manifest(root: &Path, m: &HomemadeManifest) -> Result<()> {
    std::fs::write(root.join("homemade.toml"), m.to_toml())?;
    Ok(())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(bytes))
}
