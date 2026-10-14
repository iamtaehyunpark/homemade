//! Install resources/SKILL.md to ~/.homemade/skills/homemade/SKILL.md and symlink into ~/.claude/skills/homemade. OWNER: agent D.
use crate::error::{err, Result};
use std::path::Path;

pub fn install(resources_dir: &Path, home: &Path) -> Result<String> {
    let src = resources_dir.join("SKILL.md");
    let content = match std::fs::read_to_string(&src) {
        Ok(c) => c,
        Err(_) => {
            // dev fallback: crate-relative resources dir
            let dev = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/SKILL.md");
            std::fs::read_to_string(&dev).or_else(|e| err(format!("SKILL.md not found at {}: {e}", src.display())))?
        }
    };
    let dir = home.join("skills").join("homemade");
    std::fs::create_dir_all(&dir)?;
    let dest = dir.join("SKILL.md");
    std::fs::write(&dest, content)?;

    // Best-effort symlink ~/.claude/skills/homemade -> dir
    if let Some(user_home) = dirs::home_dir() {
        let skills = user_home.join(".claude").join("skills");
        let link = skills.join("homemade");
        let _ = std::fs::create_dir_all(&skills);
        match std::fs::symlink_metadata(&link) {
            Ok(m) if m.file_type().is_symlink() => {
                let _ = std::fs::remove_file(&link);
                #[cfg(unix)]
                let _ = std::os::unix::fs::symlink(&dir, &link);
            }
            Ok(_) => { /* real dir/file: leave it alone */ }
            Err(_) => {
                #[cfg(unix)]
                let _ = std::os::unix::fs::symlink(&dir, &link);
            }
        }
    }
    Ok(dest.to_string_lossy().to_string())
}
