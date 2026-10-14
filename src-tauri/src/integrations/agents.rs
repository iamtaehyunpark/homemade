//! Agent session discovery (spec §10A, all formats verified on the dev machine). OWNER: agent D.
use crate::error::Result;
use crate::types::{AgentSessionRecord, DetectedAgent};
use serde_json::Value;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Find an executable on a PATH string.
pub fn which(bin: &str, login_path: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let mut dirs: Vec<PathBuf> = std::env::split_paths(login_path).collect();
    if let Some(h) = dirs::home_dir() {
        dirs.push(h.join(".local/bin"));
    }
    dirs.push("/opt/homebrew/bin".into());
    dirs.push("/usr/local/bin".into());
    dirs.into_iter().map(|d| d.join(bin)).find(|p| {
        std::fs::metadata(p).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
    })
}

/// Which of claude / codex / agy are on PATH (use login_env PATH). display_name: "Claude Code", "Codex CLI", "Antigravity CLI".
pub fn detect(login_path: &str) -> Vec<DetectedAgent> {
    [("claude_code", "claude", "Claude Code"), ("codex", "codex", "Codex CLI"), ("antigravity", "agy", "Antigravity CLI")]
        .iter()
        .filter_map(|(t, bin, name)| {
            which(bin, login_path).map(|p| DetectedAgent {
                agent_type: t.to_string(),
                binary_path: p.to_string_lossy().to_string(),
                display_name: name.to_string(),
            })
        })
        .collect()
}

fn truncate(s: &str, n: usize) -> String {
    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    if s.chars().count() <= n { s } else { format!("{}…", s.chars().take(n - 1).collect::<String>()) }
}

fn usable_prompt(s: &str) -> Option<String> {
    let t = s.trim();
    if t.is_empty() || t.starts_with('<') || t.starts_with('/') { None } else { Some(truncate(t, 80)) }
}

fn mtime_ms(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Extract text from a content value: string or array of {type:"text"|"input_text", text}.
fn content_texts(c: &Value) -> Vec<String> {
    match c {
        Value::String(s) => vec![s.clone()],
        Value::Array(a) => a
            .iter()
            .filter(|b| matches!(b.get("type").and_then(|t| t.as_str()), Some("text") | Some("input_text")))
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()).map(String::from))
            .collect(),
        _ => vec![],
    }
}

fn claude_sessions(root: &Path, app_id: &str) -> Vec<AgentSessionRecord> {
    let Some(home) = dirs::home_dir() else { return vec![] };
    let encoded = root.to_string_lossy().replace('/', "-");
    let dir = home.join(".claude/projects").join(encoded);
    let Ok(rd) = std::fs::read_dir(&dir) else { return vec![] };
    let mut out = vec![];
    for e in rd.flatten() {
        let p = e.path();
        if !p.is_file() || p.extension().and_then(|x| x.to_str()) != Some("jsonl") { continue; }
        let Some(id) = p.file_stem().and_then(|s| s.to_str()).map(String::from) else { continue };
        let mut title = None;
        if let Ok(f) = std::fs::File::open(&p) {
            for line in BufReader::new(f).lines().map_while(|l| l.ok()).take(400) {
                if !line.contains("\"user\"") { continue; }
                let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
                if v.get("type").and_then(|t| t.as_str()) != Some("user") { continue; }
                if let Some(c) = v.pointer("/message/content") {
                    if let Some(t) = content_texts(c).iter().find_map(|t| usable_prompt(t)) {
                        title = Some(t);
                        break;
                    }
                }
            }
        }
        out.push(AgentSessionRecord {
            session_id: id, app_id: app_id.into(), agent_type: "claude_code".into(), title,
            git_commit_before: None, git_commit_after: None, updated_at: mtime_ms(&p),
        });
    }
    out
}

fn codex_sessions(root: &Path, app_id: &str) -> Vec<AgentSessionRecord> {
    let Some(home) = dirs::home_dir() else { return vec![] };
    let base = home.join(".codex/sessions");
    let root_s = root.to_string_lossy().trim_end_matches('/').to_string();
    let mut out = vec![];
    for e in walkdir::WalkDir::new(&base).max_depth(4).into_iter().flatten() {
        let p = e.path();
        let name = e.file_name().to_string_lossy();
        if !e.file_type().is_file() || !name.starts_with("rollout-") || !name.ends_with(".jsonl") { continue; }
        let Ok(f) = std::fs::File::open(p) else { continue };
        let mut lines = BufReader::new(f).lines().map_while(|l| l.ok());
        let Some(first) = lines.next() else { continue };
        let Ok(meta) = serde_json::from_str::<Value>(&first) else { continue };
        let payload = meta.get("payload").cloned().unwrap_or(Value::Null);
        let cwd = payload.get("cwd").and_then(|c| c.as_str()).unwrap_or("").trim_end_matches('/');
        if cwd != root_s { continue; }
        let Some(sid) = payload.get("session_id").or_else(|| payload.get("id")).and_then(|s| s.as_str()).map(String::from) else { continue };
        let mut title = None;
        for line in lines.take(200) {
            if !line.contains("\"user\"") { continue; }
            let Ok(v) = serde_json::from_str::<Value>(&line) else { continue };
            let pl = v.get("payload").unwrap_or(&Value::Null);
            let is_user_msg = pl.get("role").and_then(|r| r.as_str()) == Some("user");
            let texts = if is_user_msg {
                pl.get("content").map(content_texts).unwrap_or_default()
            } else if pl.get("type").and_then(|t| t.as_str()) == Some("user_message") {
                pl.get("message").and_then(|m| m.as_str()).map(|s| vec![s.to_string()]).unwrap_or_default()
            } else { vec![] };
            if let Some(t) = texts.iter().find_map(|t| usable_prompt(t)) { title = Some(t); break; }
        }
        if title.is_none() {
            // rollout-<ts>-<id>.jsonl -> ts
            title = name.strip_prefix("rollout-").and_then(|r| r.get(..19)).map(|ts| format!("Codex session {ts}"));
        }
        out.push(AgentSessionRecord {
            session_id: sid, app_id: app_id.into(), agent_type: "codex".into(), title,
            git_commit_before: None, git_commit_after: None, updated_at: mtime_ms(p),
        });
    }
    out
}

fn parse_sqlite_time(s: &str) -> Option<i64> {
    for fmt in ["%Y-%m-%d %H:%M:%S%.f%:z", "%Y-%m-%dT%H:%M:%S%.f%:z", "%Y-%m-%d %H:%M:%S%.f%#z"] {
        if let Ok(d) = chrono::DateTime::parse_from_str(s, fmt) { return Some(d.timestamp_millis()); }
    }
    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f").ok().map(|d| d.and_utc().timestamp_millis())
}

fn antigravity_query(db: &Path, root_uri: &str, app_id: &str) -> rusqlite::Result<Vec<AgentSessionRecord>> {
    let c = rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX)?;
    c.busy_timeout(std::time::Duration::from_millis(500))?;
    let mut st = c.prepare("SELECT conversation_id, title, preview, CAST(last_modified_time AS TEXT), workspace_uris FROM conversation_summaries")?;
    let rows = st.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, Option<String>>(4)?))
    })?;
    let mut out = vec![];
    for (id, title, preview, lmt, uris) in rows.flatten() {
        let matches = uris
            .as_deref()
            .and_then(|u| serde_json::from_str::<Vec<String>>(u).ok())
            .map(|v| v.iter().any(|u| u.trim_end_matches('/') == root_uri))
            .unwrap_or(false);
        if !matches { continue; }
        let title = title.as_deref().and_then(usable_prompt).or_else(|| preview.as_deref().and_then(usable_prompt));
        out.push(AgentSessionRecord {
            session_id: id, app_id: app_id.into(), agent_type: "antigravity".into(), title,
            git_commit_before: None, git_commit_after: None,
            updated_at: lmt.as_deref().and_then(parse_sqlite_time).unwrap_or(0),
        });
    }
    Ok(out)
}

fn antigravity_sessions(root: &Path, app_id: &str) -> Vec<AgentSessionRecord> {
    let Some(home) = dirs::home_dir() else { return vec![] };
    let db = home.join(".gemini/antigravity-cli/conversation_summaries.db");
    if !db.exists() { return vec![]; }
    let root_uri = format!("file://{}", root.to_string_lossy().trim_end_matches('/'));
    match antigravity_query(&db, &root_uri, app_id) {
        Ok(v) => v,
        Err(_) => {
            // Locked: copy to a temp file (plus WAL if present) and retry.
            let tmp = std::env::temp_dir().join(format!("homemade-agy-{}.db", uuid::Uuid::new_v4()));
            let mut res = vec![];
            if std::fs::copy(&db, &tmp).is_ok() {
                let wal = db.with_extension("db-wal");
                if wal.exists() { let _ = std::fs::copy(&wal, tmp.with_extension("db-wal")); }
                res = antigravity_query(&tmp, &root_uri, app_id).unwrap_or_default();
            }
            let _ = std::fs::remove_file(&tmp);
            let _ = std::fs::remove_file(tmp.with_extension("db-wal"));
            res
        }
    }
}

/// Filesystem-only discovery across all three agents.
pub fn discover_fs(root: &Path, app_id: &str) -> Vec<AgentSessionRecord> {
    let mut v = claude_sessions(root, app_id);
    v.extend(codex_sessions(root, app_id));
    v.extend(antigravity_sessions(root, app_id));
    v
}

/// Claude Code: ~/.claude/projects/<root with '/' -> '-'>/*.jsonl (skip dirs). title = first line with type=="user"
/// -> message.content (string, or first text block). updated_at = file mtime ms.
/// Codex: ~/.codex/sessions/**/rollout-*.jsonl; line 1 session_meta.payload.{session_id,cwd}; match cwd == root.
/// Antigravity: ~/.gemini/antigravity-cli/conversation_summaries.db (sqlite).
/// Merge with DB rows (upsert so git_commit_* survive). Sorted by updated_at desc.
/// NOTE: opens its own connection-free path; callers pass in the DB via `merge_with_db`.
pub fn discover(root: &Path, app_id: &str) -> Result<Vec<AgentSessionRecord>> {
    Ok(discover_fs(root, app_id))
}

/// Upsert discovered sessions into the DB and return all DB rows for the app, newest first.
pub fn merge_with_db(c: &rusqlite::Connection, app_id: &str, found: &[AgentSessionRecord]) -> Result<Vec<AgentSessionRecord>> {
    for s in found {
        // Don't let a mtime of 0 clobber a real timestamp.
        if s.updated_at == 0 { continue; }
        crate::db::queries::upsert_agent_session(c, s)?;
    }
    let mut all = crate::db::queries::list_agent_sessions(c, app_id)?;
    all.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(all)
}

/// Context appended to a new Claude Code session: points at homemade.toml, the skill, @homemade: directives, and last failed run tail.
pub fn system_prompt(app_name: &str, root: &Path, last_failed_tail: Option<&str>) -> String {
    let mut s = format!(
        "This project (\"{app_name}\", at {root}) is registered in Homemade, the personal-software launcher. \
Before editing, read `{root}/homemade.toml` (the app manifest: exec command, [inputs], [ui], [resources], [artifacts]) \
and the `homemade` skill. Keep homemade.toml in sync with any CLI arguments you add or change. \
Report progress/artifacts by printing `@homemade:progress <pct> \"msg\"` and `@homemade:artifact \"/abs/path\"` to stdout (flush=True); \
write outputs to $HOMEMADE_OUTPUT_DIR. Use uv; never create a standalone venv or download duplicate model weights into the project. \
For a custom UI, use the injected `window.homemade` bridge instead of a web server.",
        root = root.display()
    );
    if let Some(tail) = last_failed_tail.filter(|t| !t.trim().is_empty()) {
        let lines: Vec<&str> = tail.lines().collect();
        let start = lines.len().saturating_sub(60);
        s.push_str("\n\nThe most recent run of this app FAILED. Last log lines:\n```\n");
        s.push_str(&lines[start..].join("\n"));
        s.push_str("\n```");
    }
    s
}
