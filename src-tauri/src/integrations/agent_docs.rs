//! Managed `CLAUDE.md` / `AGENTS.md` block in each registered app's root, so agents started *outside* Homemade (a plain
//! `claude` or `codex` in the folder) still learn that the project is a Homemade app and how to drive the launcher.
//! The block sits between markers and is rewritten idempotently; the rest of the file is the user's. Removing the app
//! strips the block (and deletes the file if nothing else is in it). `[agents] write_docs = false` turns this off.
use crate::manifest::schema::HomemadeManifest;
use std::path::Path;

const START: &str = "<!-- homemade:start -->";
const END: &str = "<!-- homemade:end -->";
pub const FILES: [&str; 2] = ["CLAUDE.md", "AGENTS.md"];

fn block(app_id: &str, m: &HomemadeManifest, home: &Path) -> String {
    let exe = std::env::current_exe().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|_| "homemade".into());
    let inputs: Vec<String> = m.inputs.iter().map(|(k, i)| format!("{k} ({})", i.field_type)).collect();
    format!(
"{START}
## Homemade

This folder is registered in **Homemade** (the personal-software launcher) as app `{app_id}` — mode `{mode}`, exec `{exec}`.
The manifest is `homemade.toml` (keep it in sync with any CLI argument you add or change; declared inputs: {inputs}).
Follow the `homemade` skill (`{skill}`) for conventions: uv, `$HOMEMADE_OUTPUT_DIR`, `@homemade:progress` / `@homemade:artifact`.

You can drive the launcher yourself instead of asking the user to click Run:
- CLI: `{bin}/homemade` (`homemade run {app_id} key=value --wait`, `homemade activity {app_id}`, `homemade log <run-id> --follow`,
  `homemade top|ps|signal|stdin|capture|record`, `homemade help`).
- MCP: `{exe} --mcp` (config file `{mcp}`; tools: start_run, wait_run, app_activity, get_run_log, last_failed_run, explain_run,
  list_artifacts, apply_manifest, git_checkpoint, open_app, capture, …). Register once: `claude mcp add homemade -- {exe} --mcp`.
- Inside Homemade sessions these are already on PATH / connected, and the user can forward `[homemade]` status lines (run started / finished / diagnosis) into the session.
{END}",
        mode = m.app.mode.as_str(), exec = m.app.exec,
        inputs = if inputs.is_empty() { "none".to_string() } else { inputs.join(", ") },
        skill = home.join("skills/homemade/SKILL.md").display(),
        bin = home.join("bin").display(), mcp = home.join("mcp.json").display(),
    )
}

fn enabled(home: &Path) -> bool {
    let cfg = crate::storage::config::load(home);
    crate::storage::config::get_bool(&cfg, "agents", "write_docs", true)
}

/// Write/refresh the managed block in CLAUDE.md and AGENTS.md. Never touches anything outside the markers.
pub fn ensure(root: &Path, app_id: &str, m: &HomemadeManifest, home: &Path) {
    if !enabled(home) || !root.is_dir() { return; }
    let b = block(app_id, m, home);
    for f in FILES {
        let p = root.join(f);
        let cur = std::fs::read_to_string(&p).unwrap_or_default();
        let next = match (cur.find(START), cur.find(END)) {
            (Some(s), Some(e)) if e > s => format!("{}{}{}", &cur[..s], b, &cur[e + END.len()..]),
            _ if cur.trim().is_empty() => format!("{b}\n"),
            _ => format!("{}\n\n{b}\n", cur.trim_end()),
        };
        if next != cur { let _ = std::fs::write(&p, next); }
    }
}

/// Strip the managed block; delete the file when it was ours alone.
pub fn remove(root: &Path) {
    for f in FILES {
        let p = root.join(f);
        let Ok(cur) = std::fs::read_to_string(&p) else { continue };
        let (Some(s), Some(e)) = (cur.find(START), cur.find(END)) else { continue };
        if e < s { continue; }
        let rest = format!("{}{}", cur[..s].trim_end(), &cur[e + END.len()..]);
        if rest.trim().is_empty() { let _ = std::fs::remove_file(&p); } else { let _ = std::fs::write(&p, format!("{}\n", rest.trim_end())); }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn block_is_idempotent_and_preserves_user_text() {
        let dir = tempdir();
        let m = HomemadeManifest::parse("[app]\nid='x'\nname='X'\nexec='python3 main.py'\n").unwrap();
        std::fs::write(dir.join("CLAUDE.md"), "# Mine\nkeep this\n").unwrap();
        ensure(&dir, "x", &m, &dir);
        let a = std::fs::read_to_string(dir.join("CLAUDE.md")).unwrap();
        assert!(a.starts_with("# Mine\nkeep this\n") && a.contains("app `x`") && a.contains(END));
        ensure(&dir, "x", &m, &dir);
        assert_eq!(a, std::fs::read_to_string(dir.join("CLAUDE.md")).unwrap());
        assert!(dir.join("AGENTS.md").is_file());
        remove(&dir);
        assert_eq!(std::fs::read_to_string(dir.join("CLAUDE.md")).unwrap(), "# Mine\nkeep this\n");
        assert!(!dir.join("AGENTS.md").exists());
    }
    fn tempdir() -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("hm-agentdocs-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&d).unwrap(); d
    }
}
