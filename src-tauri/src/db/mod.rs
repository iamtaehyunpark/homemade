//! SQLite connection + migrations. Schema per spec.md §2.
pub mod queries;

use crate::error::Result;
use rusqlite::Connection;
use std::path::Path;

pub const DDL: &str = r#"
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS apps (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    root_dir TEXT NOT NULL UNIQUE,
    manifest_hash TEXT NOT NULL,
    execution_mode TEXT NOT NULL DEFAULT 'cli',
    unmanaged_venv_bytes INTEGER DEFAULT 0,
    learned_peak_ram_mb INTEGER DEFAULT 0,
    learned_peak_vram_mb INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL,
    last_run_at INTEGER
);

CREATE TABLE IF NOT EXISTS presets (
    id TEXT PRIMARY KEY,
    app_id TEXT NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    inputs_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS runs (
    id TEXT PRIMARY KEY,
    app_id TEXT NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    inputs_json TEXT NOT NULL,
    cli_command TEXT NOT NULL,
    exit_code INTEGER,
    peak_ram_mb INTEGER DEFAULT 0,
    peak_vram_mb INTEGER DEFAULT 0,
    started_at INTEGER NOT NULL,
    finished_at INTEGER,
    log_tail TEXT
);

CREATE TABLE IF NOT EXISTS artifacts (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    app_id TEXT NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
    file_path TEXT NOT NULL,
    file_size_bytes INTEGER NOT NULL,
    mime_type TEXT NOT NULL,
    kind TEXT NOT NULL,
    created_at INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS storage_refs (
    asset_hash TEXT NOT NULL,
    asset_path TEXT NOT NULL,
    app_id TEXT NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
    size_bytes INTEGER NOT NULL,
    PRIMARY KEY (asset_hash, app_id)
);

CREATE TABLE IF NOT EXISTS agent_sessions (
    session_id TEXT PRIMARY KEY,
    app_id TEXT NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
    agent_type TEXT NOT NULL,
    title TEXT,
    git_commit_before TEXT,
    git_commit_after TEXT,
    updated_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS deps (
    app_id TEXT NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    key TEXT NOT NULL,
    bytes INTEGER NOT NULL DEFAULT 0,
    source TEXT NOT NULL,
    first_seen INTEGER NOT NULL,
    PRIMARY KEY (app_id, kind, key)
);
CREATE INDEX IF NOT EXISTS idx_runs_app ON runs(app_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_artifacts_app ON artifacts(app_id, created_at DESC);
"#;

pub fn open(path: &Path) -> Result<Connection> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let conn = Connection::open(path)?;
    conn.execute_batch(DDL)?;
    // Additive migrations: process group + announced port per run, so orphans from a previous
    // session can be found and reaped, and ports can be attributed to runs.
    for col in ["pgid INTEGER", "port INTEGER"] {
        let _ = conn.execute(&format!("ALTER TABLE runs ADD COLUMN {col}"), []);
    }
    Ok(conn)
}
