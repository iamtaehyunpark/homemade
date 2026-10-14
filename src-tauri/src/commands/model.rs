//! Local model commands (CONTRACT.md "Local model (Phase 2)"). Names are the frontend contract.
use super::St;
use crate::error::{err, Result};
use crate::intelligence::draft::{self, DraftResult};
use crate::intelligence::local_model::ModelStatus;
use crate::intelligence::traits::SemanticHealthReport;
use crate::manifest::schema::HomemadeManifest;
use crate::types::AppRecord;
use std::path::{Path, PathBuf};

fn folder_of(path: &str) -> Result<PathBuf> {
    let p = PathBuf::from(path).canonicalize().map_err(|e| crate::error::Error::Msg(format!("Cannot open {path}: {e}")))?;
    Ok(if p.is_file() { p.parent().map(Path::to_path_buf).unwrap_or(p) } else { p })
}

#[tauri::command]
pub async fn model_status(state: St<'_>) -> Result<ModelStatus> { Ok(state.model.status().await) }

/// Draft a manifest for a folder (never writes to disk).
#[tauri::command]
pub async fn draft_manifest(state: St<'_>, path: String) -> Result<DraftResult> {
    let root = folder_of(&path)?;
    draft::draft_manifest(&state.model, &root, &super::resources_dir(&state.app_handle)).await
}

/// Validate, write `<path>/homemade.toml`, then import (upsert + watcher + app://changed).
#[tauri::command]
pub async fn apply_manifest(state: St<'_>, path: String, toml: String) -> Result<AppRecord> {
    let m = HomemadeManifest::parse(&toml).map_err(|e| crate::error::Error::Msg(format!("Invalid homemade.toml: {e}")))?;
    let errs = draft::validate(&m);
    if !errs.is_empty() { return err(format!("Invalid homemade.toml: {}", errs.join("; "))); }
    let root = folder_of(&path)?;
    std::fs::write(root.join("homemade.toml"), &toml)?;
    let rec = super::apps::import_path(state.inner(), &super::resources_dir(&state.app_handle), &root.to_string_lossy()).await?;
    state.emit("app://changed", serde_json::json!({ "appId": rec.id }));
    Ok(rec)
}

/// Explain a run from its live log or stored log_tail.
#[tauri::command]
pub async fn explain_run(state: St<'_>, run_id: String) -> Result<SemanticHealthReport> {
    let lines: Vec<String> = if let Some(r) = state.active_runs.get(&run_id) {
        r.log.lock().unwrap().iter().cloned().collect()
    } else {
        let rec = { let c = state.db.lock().unwrap(); crate::db::queries::get_run(&c, &run_id)? };
        let Some(rec) = rec else { return err(format!("Unknown run: {run_id}")) };
        rec.log_tail.unwrap_or_default().lines().map(String::from).collect()
    };
    let s = state.model.status().await;
    if !s.available { return err(format!("Local model unavailable: {}", s.reason.unwrap_or_default())); }
    Ok(state.assistor.analyze_log_window(&lines).await
        .unwrap_or(SemanticHealthReport { severity: "info".into(), message: "Nothing notable in this log.".into() }))
}

/// Raw completion (tests / power users), 512 tokens, plain text.
#[tauri::command]
pub async fn model_complete(state: St<'_>, system: String, prompt: String) -> Result<String> {
    state.model.complete(&system, &prompt, false, 512).await
}
