//! Phase 2 plugin boundary (spec §12). v1 ships the no-op implementation only.
use crate::manifest::schema::InputFieldSpec;
use indexmap::IndexMap;

#[derive(Debug, Clone, serde::Serialize)]
pub struct SemanticHealthReport { pub severity: String, pub message: String }

#[async_trait::async_trait]
pub trait SemanticAssistor: Send + Sync {
    async fn analyze_log_window(&self, recent_lines: &[String]) -> Option<SemanticHealthReport>;
    async fn infer_schema_from_raw_code(&self, source_code: &str) -> Option<IndexMap<String, InputFieldSpec>>;
    async fn summarize_agent_session(&self, git_diff: &str, transcript_tail: &str) -> Option<String>;
}

pub struct RuleBasedOnlyAssistor;
#[async_trait::async_trait]
impl SemanticAssistor for RuleBasedOnlyAssistor {
    async fn analyze_log_window(&self, _: &[String]) -> Option<SemanticHealthReport> { None }
    async fn infer_schema_from_raw_code(&self, _: &str) -> Option<IndexMap<String, InputFieldSpec>> { None }
    async fn summarize_agent_session(&self, _: &str, _: &str) -> Option<String> { None }
}
