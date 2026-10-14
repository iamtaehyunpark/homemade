//! `SemanticAssistor` backed by the Apple on-device model. Each call re-checks availability (cached 60 s),
//! so enabling Apple Intelligence later starts working without a restart.
use super::local_model::LocalModel;
use super::traits::{RuleBasedOnlyAssistor, SemanticAssistor, SemanticHealthReport};
use crate::manifest::schema::InputFieldSpec;
use indexmap::IndexMap;
use std::sync::Arc;

pub struct AppleAssistor { pub model: Arc<LocalModel> }

/// Apple-backed when the helper exists and the provider is enabled, else the no-op assistor.
pub fn make(model: Arc<LocalModel>) -> Arc<dyn SemanticAssistor> {
    let s = model.base_status_public();
    if s.provider == "apple" && s.helper_path.is_some() { Arc::new(AppleAssistor { model }) } else { Arc::new(RuleBasedOnlyAssistor) }
}

fn tail_chars(s: &str, n: usize) -> String {
    let c = s.chars().count();
    if c <= n { s.to_string() } else { s.chars().skip(c - n).collect() }
}

impl AppleAssistor {
    async fn json(&self, system: &str, prompt: &str, max_tokens: u32) -> Option<serde_json::Value> {
        if !self.model.status().await.available { return None; }
        let t = self.model.complete(system, prompt, true, max_tokens).await.map_err(|e| eprintln!("[assistor] {e}")).ok()?;
        if std::env::var_os("HOMEMADE_DEBUG_MODEL").is_some() { eprintln!("[assistor] model output: {t}"); }
        super::draft::parse_lenient(&t).map_err(|e| eprintln!("[assistor] non-JSON model output ({e})")).ok()
    }
}

#[async_trait::async_trait]
impl SemanticAssistor for AppleAssistor {
    async fn analyze_log_window(&self, recent_lines: &[String]) -> Option<SemanticHealthReport> {
        let start = recent_lines.len().saturating_sub(120);
        let log = tail_chars(&recent_lines[start..].join("\n"), 7000);
        let v = self.json(
            "You diagnose failed program runs from their log. Reply with JSON only: {\"severity\":\"info|warn|error\",\"message\":\"<one or two sentences: the root cause and the concrete fix>\"}.",
            &format!("Log tail:\n{log}"), 256).await?;
        let severity = v.get("severity").and_then(|x| x.as_str()).unwrap_or("info").to_lowercase();
        let severity = if ["info", "warn", "error"].contains(&severity.as_str()) { severity } else { "warn".into() };
        let message = v.get("message").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        if message.is_empty() && severity == "info" { return None; }
        if message.is_empty() { return None; }
        Some(SemanticHealthReport { severity, message })
    }

    async fn infer_schema_from_raw_code(&self, source_code: &str) -> Option<IndexMap<String, InputFieldSpec>> {
        let src: String = source_code.chars().take(8000).collect();
        let v = self.json(
            "You extract the command-line parameters a Python script reads (sys.argv, input(), env, hard-coded paths meant to be changed). Reply with JSON only: {\"inputs\":{\"<name>\":{\"type\":\"file|directory|string|int|float|bool|select\",\"cli_flag\":\"--name\" or \"positional_index\":0,\"label\":\"..\",\"default\":..,\"required\":true|false,\"options\":[..],\"min\":..,\"max\":..}}}. Use positional_index for sys.argv[n] (index n-1). Return {\"inputs\":{}} if it takes none.",
            &format!("```python\n{src}\n```"), 700).await?;
        let m = super::draft::normalize_inputs(&v);
        if m.is_empty() { None } else { Some(m) }
    }

    async fn summarize_agent_session(&self, git_diff: &str, transcript_tail: &str) -> Option<String> {
        if !self.model.status().await.available { return None; }
        let prompt = format!("git diff --stat:\n{}\n\nTerminal transcript tail:\n{}", tail_chars(git_diff, 3000), tail_chars(transcript_tail, 3500));
        let t = self.model.complete(
            "Summarize what this coding-agent session changed as ONE short title line (max 80 characters, imperative mood, no quotes, no trailing period).",
            &prompt, false, 60).await.ok()?;
        let line = t.lines().map(|l| l.trim().trim_matches(['"', '\'', '*', '#', ' ']).trim_end_matches('.')).find(|l| !l.is_empty())?.to_string();
        Some(line.chars().take(80).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Live check against the real helper: `cargo test -- --ignored live_model` (needs the built helper + Apple Intelligence).
    #[tokio::test]
    #[ignore]
    async fn live_model() {
        let res = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources");
        let model = Arc::new(LocalModel::new(&res, &toml::Table::new()));
        println!("status: {:?}", model.status().await);
        let a = AppleAssistor { model };
        let src = "import sys\nfrom PIL import Image\nimg = Image.open(sys.argv[1])\nscale = float(sys.argv[2]) if len(sys.argv) > 2 else 0.5\nimg.resize((int(img.width*scale), int(img.height*scale))).save('out.png')\n";
        println!("schema: {:?}", a.infer_schema_from_raw_code(src).await);
        let log: Vec<String> = ["Traceback (most recent call last):", "  File \"main.py\", line 3, in <module>", "    import numpy as np", "ModuleNotFoundError: No module named 'numpy'"].iter().map(|s| s.to_string()).collect();
        println!("health: {:?}", a.analyze_log_window(&log).await);
        println!("summary: {:?}", a.summarize_agent_session(" main.py | 12 ++++++++----\n README.md | 3 +++\n 2 files changed", "> add a --scale flag to main.py and document it\nEdited main.py\nEdited README.md").await);
    }
}
