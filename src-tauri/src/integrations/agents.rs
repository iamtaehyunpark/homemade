//! Agent session discovery (spec §10A, all formats verified on the dev machine). OWNER: agent D.
use crate::error::Result;
use crate::types::{AgentSessionRecord, DetectedAgent};
use std::path::Path;

/// Which of claude / codex / agy are on PATH (use login_env PATH). display_name: "Claude Code", "Codex CLI", "Antigravity CLI".
pub fn detect(login_path: &str) -> Vec<DetectedAgent> { let _ = login_path; vec![] }

/// Claude Code: ~/.claude/projects/<root with '/' -> '-'>/*.jsonl (skip dirs). title = first line with type=="user"
/// -> message.content (string, or first text block). updated_at = file mtime ms.
/// Codex: ~/.codex/sessions/**/rollout-*.jsonl; line 1 session_meta.payload.{session_id,cwd}; match cwd == root. title = first
/// response_item/user message text if cheap, else filename timestamp.
/// Antigravity: ~/.gemini/antigravity-cli/conversation_summaries.db (sqlite): conversation_id, title, preview,
/// last_modified_time, workspace_uris (JSON array of file:// URIs) where any URI == file://<root>. Fallback: cache/conversation_metadata.json.
/// Merge with DB rows (upsert so git_commit_* survive). Sorted by updated_at desc.
pub fn discover(root: &Path, app_id: &str) -> Result<Vec<AgentSessionRecord>> { let _ = (root, app_id); Ok(vec![]) }

/// Context appended to a new Claude Code session: points at homemade.toml, the skill, @homemade: directives, and last failed run tail.
pub fn system_prompt(app_name: &str, root: &Path, last_failed_tail: Option<&str>) -> String {
    let _ = (app_name, root, last_failed_tail);
    String::from("This project is registered in Homemade (the personal-software launcher). Read homemade.toml and the `homemade` skill before editing.")
}
