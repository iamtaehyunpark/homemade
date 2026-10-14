//! git init / checkpoint / revert (spec §10C). OWNER: agent D. Shell out to `git`.
use crate::error::{err, Result};
use crate::types::{GitLogEntry, GitStatus};
use std::path::Path;
use std::process::Command;

pub const DEFAULT_GITIGNORE: &str = ".venv/\nvenv/\n__pycache__/\nnode_modules/\n*.pt\n*.pth\n*.safetensors\n*.gguf\n*.bin\ntmp_*\noutputs/\n.homemade/\n";

fn git(root: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|e| crate::error::Error::Msg(format!("failed to run git: {e}")))?;
    if !out.status.success() {
        return err(format!("git {} failed: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn is_repo(root: &Path) -> bool {
    git(root, &["rev-parse", "--is-inside-work-tree"]).map(|s| s.trim() == "true").unwrap_or(false)
}

/// Commit with a fallback identity so commits work even without user.name/email configured.
fn commit(root: &Path, message: &str) -> Result<String> {
    let has_identity = git(root, &["config", "user.email"]).map(|s| !s.trim().is_empty()).unwrap_or(false);
    if has_identity {
        git(root, &["commit", "-q", "--no-verify", "-m", message])
    } else {
        git(root, &["-c", "user.name=Homemade", "-c", "user.email=homemade@localhost", "commit", "-q", "--no-verify", "-m", message])
    }
}

pub fn head_sha(root: &Path) -> Option<String> {
    git(root, &["rev-parse", "HEAD"]).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

pub fn is_dirty(root: &Path) -> bool {
    git(root, &["status", "--porcelain"]).map(|s| !s.trim().is_empty()).unwrap_or(false)
}

pub fn status(root: &Path) -> Result<GitStatus> {
    if !is_repo(root) {
        return Ok(GitStatus { initialized: false, dirty: false, head: None, log: vec![], dirty_files: vec![] });
    }
    let head = head_sha(root);
    let log = if head.is_some() {
        git(root, &["log", "--format=%H%x1f%s%x1f%ct%x1f%P%x1f%D%x1f%an", "-n", "100"])
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let mut p = l.split('\x1f');
                let sha = p.next()?.to_string();
                let message = p.next().unwrap_or("").to_string();
                let timestamp = p.next().and_then(|t| t.trim().parse::<i64>().ok()).unwrap_or(0) * 1000;
                let parents = p.next().unwrap_or("").split_whitespace().map(String::from).collect();
                // %D looks like "HEAD -> main, tag: v1, origin/main"
                let refs = p.next().unwrap_or("").split(',').flat_map(|r| r.split("->")).map(|r| r.trim().trim_start_matches("tag: ").to_string())
                    .filter(|r| !r.is_empty()).collect();
                let author = p.next().unwrap_or("").trim().to_string();
                Some(GitLogEntry { sha, message, timestamp, parents, refs, author })
            })
            .collect()
    } else {
        vec![]
    };
    let dirty_files = git(root, &["status", "--porcelain"]).unwrap_or_default().lines()
        .filter(|l| l.len() > 3)
        .map(|l| crate::types::GitDirtyFile { status: l[..2].trim().to_string(), path: l[3..].trim().to_string() })
        .take(200).collect::<Vec<_>>();
    Ok(GitStatus { initialized: true, dirty: !dirty_files.is_empty(), head, log, dirty_files })
}

/// git init + write .gitignore (if absent) + initial commit "homemade: initial checkpoint".
pub fn init(root: &Path) -> Result<()> {
    if !root.join(".git").exists() {
        git(root, &["init", "-q"])?;
    }
    let gi = root.join(".gitignore");
    if !gi.exists() {
        std::fs::write(&gi, DEFAULT_GITIGNORE)?;
    }
    git(root, &["add", "-A"])?;
    if is_dirty(root) || head_sha(root).is_none() {
        commit(root, "homemade: initial checkpoint")?;
    }
    Ok(())
}

/// git add -A && git commit -m <message>; returns new sha (or current HEAD if nothing to commit).
pub fn checkpoint(root: &Path, message: &str) -> Result<String> {
    if !is_repo(root) {
        return err("not a git repository");
    }
    git(root, &["add", "-A"])?;
    if is_dirty(root) {
        commit(root, message)?;
    }
    head_sha(root).ok_or_else(|| "repository has no commits".into())
}

/// Safe revert: checkpoint current dirty state first ("homemade: pre-revert"), then `git reset --hard <sha>`.
pub fn revert_to(root: &Path, sha: &str) -> Result<()> {
    if sha.starts_with('-') {
        return err("invalid sha");
    }
    git(root, &["cat-file", "-e", &format!("{sha}^{{commit}}")])?;
    if is_dirty(root) {
        checkpoint(root, "homemade: pre-revert")?;
    }
    git(root, &["reset", "--hard", "-q", sha])?;
    Ok(())
}
