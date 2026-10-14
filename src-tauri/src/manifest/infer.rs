//! Load homemade.toml from an app folder, or infer one (ast_inspector.py) when absent.
//! OWNER: agent B.
use crate::error::Result;
use crate::manifest::schema::HomemadeManifest;
use std::path::Path;

/// Returns (manifest, manifest_hash, was_inferred).
/// 1. If `<root>/homemade.toml` exists: parse it; hash = sha256(file bytes).
/// 2. Else: find the entry script (main.py / app.py / cli.py / sole *.py / package.json "main"), run
///    `python3 -I <resources>/ast_inspector.py <script>` with a 10s timeout, build a manifest with
///    exec = "uv run python <script>" (or "python3 <script>" if no pyproject/uv), inputs from inspector JSON.
///    hash = sha256(script bytes). Never execute the user's script.
pub fn load_or_infer(root: &Path, resources_dir: &Path) -> Result<(HomemadeManifest, String, bool)> {
    let _ = (root, resources_dir);
    todo!("agent B")
}

/// Write an inferred manifest to `<root>/homemade.toml` so the agent/user can edit it later.
pub fn write_manifest(root: &Path, m: &HomemadeManifest) -> Result<()> {
    std::fs::write(root.join("homemade.toml"), m.to_toml())?;
    Ok(())
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(bytes))
}
