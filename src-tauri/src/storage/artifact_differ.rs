//! Pre/post run mtime snapshot differ and scratch cleaner (spec §8C). OWNER: agent C.
use crate::manifest::schema::ArtifactConfig;
use crate::types::ArtifactRecord;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct Snapshot { pub root: PathBuf, pub files: HashMap<PathBuf, SystemTime> }

pub const SKIP_DIRS: &[&str] = &[".git", ".venv", "venv", "node_modules", "__pycache__", ".homemade", "target", ".cache"];
pub const OUTPUT_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg", "mp4", "mov", "webm", "mp3", "wav", "flac", "csv", "tsv", "json", "jsonl", "pdf", "md", "txt", "html", "xlsx", "parquet", "zip"];

/// Walk root (skip SKIP_DIRS, max depth 6, max 50k files) recording mtimes.
pub fn take(root: &Path) -> Snapshot { let _ = root; Snapshot { root: root.to_path_buf(), files: HashMap::new() } }

/// New or modified files since `before`, classified: matches scratch_globs (relative to root) => "scratch";
/// inside output_dir or extension in OUTPUT_EXTS => "output"; otherwise ignored. Skips files already
/// announced via @homemade:artifact (pass `already` paths). Build ArtifactRecord (uuid id, mime_guess, size).
pub fn diff(before: &Snapshot, cfg: &ArtifactConfig, run_id: &str, app_id: &str, already: &[String]) -> Vec<ArtifactRecord> {
    let _ = (before, cfg, run_id, app_id, already);
    vec![]
}

/// Delete every file matching scratch_globs under root (and DB rows of kind scratch, done by caller). Returns bytes freed.
pub fn clean_scratch(root: &Path, cfg: &ArtifactConfig) -> u64 { let _ = (root, cfg); 0 }
