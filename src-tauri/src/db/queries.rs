//! CRUD helpers. Every function takes a `&Connection` (lock `state.db` first) and returns records from types.rs.
use crate::error::Result;
use crate::types::*;
use rusqlite::{params, Connection, OptionalExtension, Row};

fn row_run(r: &Row) -> rusqlite::Result<RunRecord> {
    Ok(RunRecord {
        id: r.get("id")?, app_id: r.get("app_id")?, status: r.get("status")?, inputs_json: r.get("inputs_json")?,
        cli_command: r.get("cli_command")?, exit_code: r.get("exit_code")?, peak_ram_mb: r.get("peak_ram_mb")?,
        peak_vram_mb: r.get("peak_vram_mb")?, started_at: r.get("started_at")?, finished_at: r.get("finished_at")?,
        log_tail: r.get("log_tail")?,
    })
}
fn row_artifact(r: &Row) -> rusqlite::Result<ArtifactRecord> {
    Ok(ArtifactRecord {
        id: r.get("id")?, run_id: r.get("run_id")?, app_id: r.get("app_id")?, file_path: r.get("file_path")?,
        file_size_bytes: r.get("file_size_bytes")?, mime_type: r.get("mime_type")?, kind: r.get("kind")?, created_at: r.get("created_at")?,
    })
}
fn row_preset(r: &Row) -> rusqlite::Result<PresetRecord> {
    Ok(PresetRecord { id: r.get("id")?, app_id: r.get("app_id")?, name: r.get("name")?, inputs_json: r.get("inputs_json")?, created_at: r.get("created_at")? })
}
fn row_session(r: &Row) -> rusqlite::Result<AgentSessionRecord> {
    Ok(AgentSessionRecord {
        session_id: r.get("session_id")?, app_id: r.get("app_id")?, agent_type: r.get("agent_type")?, title: r.get("title")?,
        git_commit_before: r.get("git_commit_before")?, git_commit_after: r.get("git_commit_after")?, updated_at: r.get("updated_at")?,
    })
}

/// Raw app row without the manifest (manifest is loaded from disk by manifest::infer::load_app_manifest).
#[derive(Debug, Clone)]
pub struct AppRow {
    pub id: String, pub name: String, pub root_dir: String, pub manifest_hash: String, pub execution_mode: String,
    pub unmanaged_venv_bytes: i64, pub learned_peak_ram_mb: i64, pub learned_peak_vram_mb: i64, pub created_at: i64, pub last_run_at: Option<i64>,
}
fn row_app(r: &Row) -> rusqlite::Result<AppRow> {
    Ok(AppRow {
        id: r.get("id")?, name: r.get("name")?, root_dir: r.get("root_dir")?, manifest_hash: r.get("manifest_hash")?,
        execution_mode: r.get("execution_mode")?, unmanaged_venv_bytes: r.get("unmanaged_venv_bytes")?,
        learned_peak_ram_mb: r.get("learned_peak_ram_mb")?, learned_peak_vram_mb: r.get("learned_peak_vram_mb")?,
        created_at: r.get("created_at")?, last_run_at: r.get("last_run_at")?,
    })
}

// ---------- apps ----------
pub fn list_apps(c: &Connection) -> Result<Vec<AppRow>> {
    let mut st = c.prepare("SELECT * FROM apps ORDER BY COALESCE(last_run_at, created_at) DESC")?;
    Ok(st.query_map([], row_app)?.collect::<rusqlite::Result<_>>()?)
}
pub fn get_app(c: &Connection, id: &str) -> Result<Option<AppRow>> {
    Ok(c.query_row("SELECT * FROM apps WHERE id = ?1", params![id], row_app).optional()?)
}
pub fn get_app_by_root(c: &Connection, root_dir: &str) -> Result<Option<AppRow>> {
    Ok(c.query_row("SELECT * FROM apps WHERE root_dir = ?1", params![root_dir], row_app).optional()?)
}
pub fn upsert_app(c: &Connection, a: &AppRow) -> Result<()> {
    c.execute(
        "INSERT INTO apps (id,name,root_dir,manifest_hash,execution_mode,unmanaged_venv_bytes,learned_peak_ram_mb,learned_peak_vram_mb,created_at,last_run_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
         ON CONFLICT(id) DO UPDATE SET name=excluded.name, root_dir=excluded.root_dir, manifest_hash=excluded.manifest_hash,
           execution_mode=excluded.execution_mode, unmanaged_venv_bytes=excluded.unmanaged_venv_bytes",
        params![a.id, a.name, a.root_dir, a.manifest_hash, a.execution_mode, a.unmanaged_venv_bytes, a.learned_peak_ram_mb, a.learned_peak_vram_mb, a.created_at, a.last_run_at],
    )?;
    Ok(())
}
pub fn delete_app(c: &Connection, id: &str) -> Result<()> { c.execute("DELETE FROM apps WHERE id = ?1", params![id])?; Ok(()) }
pub fn set_app_venv_bytes(c: &Connection, id: &str, bytes: i64) -> Result<()> {
    c.execute("UPDATE apps SET unmanaged_venv_bytes = ?2 WHERE id = ?1", params![id, bytes])?; Ok(())
}
pub fn update_app_learned_peaks(c: &Connection, id: &str, ram_mb: i64, vram_mb: i64, last_run_at: i64) -> Result<()> {
    c.execute("UPDATE apps SET learned_peak_ram_mb = MAX(learned_peak_ram_mb, ?2), learned_peak_vram_mb = MAX(learned_peak_vram_mb, ?3), last_run_at = ?4 WHERE id = ?1",
        params![id, ram_mb, vram_mb, last_run_at])?;
    Ok(())
}

// ---------- runs ----------
pub fn insert_run(c: &Connection, r: &RunRecord) -> Result<()> {
    c.execute("INSERT INTO runs (id,app_id,status,inputs_json,cli_command,exit_code,peak_ram_mb,peak_vram_mb,started_at,finished_at,log_tail) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![r.id, r.app_id, r.status, r.inputs_json, r.cli_command, r.exit_code, r.peak_ram_mb, r.peak_vram_mb, r.started_at, r.finished_at, r.log_tail])?;
    Ok(())
}
pub fn set_run_status(c: &Connection, id: &str, status: &str) -> Result<()> {
    c.execute("UPDATE runs SET status = ?2 WHERE id = ?1", params![id, status])?; Ok(())
}
pub fn finish_run(c: &Connection, id: &str, status: &str, exit_code: Option<i64>, peak_ram_mb: i64, peak_vram_mb: i64, finished_at: i64, log_tail: &str) -> Result<()> {
    c.execute("UPDATE runs SET status=?2, exit_code=?3, peak_ram_mb=?4, peak_vram_mb=?5, finished_at=?6, log_tail=?7 WHERE id=?1",
        params![id, status, exit_code, peak_ram_mb, peak_vram_mb, finished_at, log_tail])?;
    Ok(())
}
pub fn get_run(c: &Connection, id: &str) -> Result<Option<RunRecord>> {
    Ok(c.query_row("SELECT * FROM runs WHERE id = ?1", params![id], row_run).optional()?)
}
pub fn list_runs(c: &Connection, app_id: &str, limit: i64) -> Result<Vec<RunRecord>> {
    let mut st = c.prepare("SELECT * FROM runs WHERE app_id = ?1 ORDER BY started_at DESC LIMIT ?2")?;
    Ok(st.query_map(params![app_id, limit], row_run)?.collect::<rusqlite::Result<_>>()?)
}

// ---------- artifacts ----------
pub fn insert_artifact(c: &Connection, a: &ArtifactRecord) -> Result<()> {
    c.execute("INSERT OR REPLACE INTO artifacts (id,run_id,app_id,file_path,file_size_bytes,mime_type,kind,created_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![a.id, a.run_id, a.app_id, a.file_path, a.file_size_bytes, a.mime_type, a.kind, a.created_at])?;
    Ok(())
}
pub fn list_artifacts(c: &Connection, app_id: &str, run_id: Option<&str>, kind: Option<&str>) -> Result<Vec<ArtifactRecord>> {
    let mut sql = String::from("SELECT * FROM artifacts WHERE app_id = ?1");
    if run_id.is_some() { sql.push_str(" AND run_id = ?2"); } else { sql.push_str(" AND (?2 IS NULL)"); }
    if kind.is_some() { sql.push_str(" AND kind = ?3"); } else { sql.push_str(" AND (?3 IS NULL)"); }
    sql.push_str(" ORDER BY created_at DESC LIMIT 500");
    let mut st = c.prepare(&sql)?;
    Ok(st.query_map(params![app_id, run_id, kind], row_artifact)?.collect::<rusqlite::Result<_>>()?)
}
pub fn delete_artifacts_by_kind(c: &Connection, app_id: &str, kind: &str) -> Result<()> {
    c.execute("DELETE FROM artifacts WHERE app_id = ?1 AND kind = ?2", params![app_id, kind])?; Ok(())
}
pub fn delete_artifact_by_path(c: &Connection, app_id: &str, path: &str) -> Result<()> {
    c.execute("DELETE FROM artifacts WHERE app_id = ?1 AND file_path = ?2", params![app_id, path])?; Ok(())
}

// ---------- presets ----------
pub fn list_presets(c: &Connection, app_id: &str) -> Result<Vec<PresetRecord>> {
    let mut st = c.prepare("SELECT * FROM presets WHERE app_id = ?1 ORDER BY created_at DESC")?;
    Ok(st.query_map(params![app_id], row_preset)?.collect::<rusqlite::Result<_>>()?)
}
pub fn insert_preset(c: &Connection, p: &PresetRecord) -> Result<()> {
    c.execute("INSERT INTO presets (id,app_id,name,inputs_json,created_at) VALUES (?1,?2,?3,?4,?5)", params![p.id, p.app_id, p.name, p.inputs_json, p.created_at])?;
    Ok(())
}
pub fn delete_preset(c: &Connection, id: &str) -> Result<()> { c.execute("DELETE FROM presets WHERE id = ?1", params![id])?; Ok(()) }

// ---------- storage refs ----------
pub fn upsert_storage_ref(c: &Connection, r: &StorageRef) -> Result<()> {
    c.execute("INSERT OR REPLACE INTO storage_refs (asset_hash,asset_path,app_id,size_bytes) VALUES (?1,?2,?3,?4)", params![r.asset_hash, r.asset_path, r.app_id, r.size_bytes])?;
    Ok(())
}
pub fn list_storage_refs(c: &Connection, app_id: &str) -> Result<Vec<StorageRef>> {
    let mut st = c.prepare("SELECT * FROM storage_refs WHERE app_id = ?1")?;
    Ok(st.query_map(params![app_id], |r| Ok(StorageRef { asset_hash: r.get("asset_hash")?, asset_path: r.get("asset_path")?, app_id: r.get("app_id")?, size_bytes: r.get("size_bytes")? }))?.collect::<rusqlite::Result<_>>()?)
}

// ---------- agent sessions ----------
pub fn upsert_agent_session(c: &Connection, s: &AgentSessionRecord) -> Result<()> {
    c.execute("INSERT INTO agent_sessions (session_id,app_id,agent_type,title,git_commit_before,git_commit_after,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7)
               ON CONFLICT(session_id) DO UPDATE SET title=COALESCE(excluded.title, title), git_commit_before=COALESCE(excluded.git_commit_before, git_commit_before),
               git_commit_after=COALESCE(excluded.git_commit_after, git_commit_after), updated_at=excluded.updated_at",
        params![s.session_id, s.app_id, s.agent_type, s.title, s.git_commit_before, s.git_commit_after, s.updated_at])?;
    Ok(())
}
pub fn list_agent_sessions(c: &Connection, app_id: &str) -> Result<Vec<AgentSessionRecord>> {
    let mut st = c.prepare("SELECT * FROM agent_sessions WHERE app_id = ?1 ORDER BY updated_at DESC")?;
    Ok(st.query_map(params![app_id], row_session)?.collect::<rusqlite::Result<_>>()?)
}
