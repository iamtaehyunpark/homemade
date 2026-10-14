//! git init / checkpoint / revert (spec §10C). OWNER: agent D. Shell out to `git`.
use crate::error::Result;
use crate::types::GitStatus;
use std::path::Path;

pub const DEFAULT_GITIGNORE: &str = ".venv/\nvenv/\n__pycache__/\nnode_modules/\n*.pt\n*.pth\n*.safetensors\n*.gguf\n*.bin\ntmp_*\noutputs/\n.homemade/\n";

pub fn head_sha(root: &Path) -> Option<String> { let _ = root; None }
pub fn status(root: &Path) -> Result<GitStatus> { let _ = root; Ok(GitStatus { initialized: false, dirty: false, head: None, log: vec![] }) }
/// git init + write .gitignore (if absent) + initial commit "homemade: initial checkpoint".
pub fn init(root: &Path) -> Result<()> { let _ = root; todo!("agent D") }
/// git add -A && git commit -m <message>; returns new sha (or current HEAD if nothing to commit).
pub fn checkpoint(root: &Path, message: &str) -> Result<String> { let _ = (root, message); todo!("agent D") }
/// Safe revert: checkpoint current dirty state first ("homemade: pre-revert"), then `git reset --hard <sha>`.
pub fn revert_to(root: &Path, sha: &str) -> Result<()> { let _ = (root, sha); todo!("agent D") }
