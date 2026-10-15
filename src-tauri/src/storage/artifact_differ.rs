//! Pre/post run mtime snapshot differ and scratch cleaner (spec §8C). OWNER: agent C.
use crate::manifest::schema::ArtifactConfig;
use crate::types::ArtifactRecord;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct Snapshot { pub root: PathBuf, pub files: HashMap<PathBuf, SystemTime> }

pub const SKIP_DIRS: &[&str] = &[".git", ".venv", "venv", "node_modules", "__pycache__", ".homemade", "target", ".cache"];
pub const OUTPUT_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "svg", "mp4", "mov", "webm", "mp3", "wav", "flac", "csv", "tsv", "json", "jsonl", "pdf", "md", "txt", "html", "xlsx", "parquet", "zip"];
const MAX_FILES: usize = 50_000;

fn walk(root: &Path) -> impl Iterator<Item = (PathBuf, std::fs::Metadata)> {
    walkdir::WalkDir::new(root).max_depth(6).follow_links(false).into_iter()
        .filter_entry(|e| !(e.depth() > 0 && e.file_type().is_dir() && SKIP_DIRS.contains(&e.file_name().to_string_lossy().as_ref())))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok().map(|m| (e.into_path(), m)))
        .take(MAX_FILES)
}

/// Walk root (skip SKIP_DIRS, max depth 6, max 50k files) recording mtimes.
pub fn take(root: &Path) -> Snapshot {
    let files = walk(root).filter_map(|(p, m)| m.modified().ok().map(|t| (p, t))).collect();
    Snapshot { root: root.to_path_buf(), files }
}

fn patterns(cfg: &ArtifactConfig) -> Vec<glob::Pattern> {
    cfg.scratch_globs.iter().filter_map(|g| glob::Pattern::new(g.trim_start_matches("./")).ok()).collect()
}

fn is_scratch(pats: &[glob::Pattern], root: &Path, path: &Path) -> bool {
    let rel = path.strip_prefix(root).unwrap_or(path);
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    pats.iter().any(|p| p.matches_path(rel) || p.matches(&name))
}

fn output_dir(root: &Path, cfg: &ArtifactConfig) -> PathBuf {
    let d = Path::new(&cfg.output_dir);
    if d.is_absolute() { d.to_path_buf() } else { root.join(cfg.output_dir.trim_start_matches("./")) }
}

/// New or modified files since `before`, classified: matches scratch_globs (relative to root) => "scratch";
/// inside output_dir or extension in OUTPUT_EXTS => "output"; otherwise ignored. Skips files already
/// announced via @homemade:artifact (pass `already` paths). Build ArtifactRecord (uuid id, mime_guess, size).
pub fn diff(before: &Snapshot, cfg: &ArtifactConfig, run_id: &str, app_id: &str, already: &[String]) -> Vec<ArtifactRecord> {
    let root = &before.root;
    let pats = patterns(cfg);
    let out_dir = output_dir(root, cfg);
    let mut files: Vec<(PathBuf, std::fs::Metadata)> = walk(root).collect();
    // output_dir may live outside root; scan it too.
    if out_dir.exists() && !out_dir.starts_with(root) { files.extend(walk(&out_dir)); }
    let mut out = Vec::new();
    for (path, meta) in files {
        let Ok(mtime) = meta.modified() else { continue };
        if before.files.get(&path).is_some_and(|t| *t >= mtime) { continue; }
        let ps = path.to_string_lossy().into_owned();
        if already.iter().any(|a| a == &ps) { continue; }
        let kind = if is_scratch(&pats, root, &path) { "scratch" }
            else if path.starts_with(&out_dir) || path.extension().map(|e| OUTPUT_EXTS.contains(&e.to_string_lossy().to_lowercase().as_str())).unwrap_or(false) { "output" }
            else { continue };
        let mtime_ms = mtime.duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or_else(|_| crate::types::now_ms());
        out.push(ArtifactRecord {
            id: uuid::Uuid::new_v4().to_string(), run_id: run_id.into(), app_id: app_id.into(),
            mime_type: mime_guess::from_path(&path).first_or_octet_stream().to_string(),
            file_path: ps, file_size_bytes: meta.len() as i64, kind: kind.into(), created_at: mtime_ms,
        });
    }
    out
}

/// Delete every file matching scratch_globs under root (and DB rows of kind scratch, done by caller). Returns bytes freed.
pub fn clean_scratch(root: &Path, cfg: &ArtifactConfig) -> u64 {
    let pats = patterns(cfg);
    if pats.is_empty() { return 0; }
    let mut freed = 0;
    for (path, meta) in walk(root).collect::<Vec<_>>() {
        if is_scratch(&pats, root, &path) && std::fs::remove_file(&path).is_ok() { freed += meta.len(); }
    }
    freed
}

/// Total size of files matching scratch_globs under root (what `clean_scratch` would free).
pub fn scratch_bytes(root: &Path, cfg: &ArtifactConfig) -> u64 {
    let pats = patterns(cfg);
    if pats.is_empty() { return 0; }
    walk(root).filter(|(p, _)| is_scratch(&pats, root, p)).map(|(_, m)| m.len()).sum()
}
