//! Shared store env injection (spec §8A). OWNER: agent C.
use std::collections::HashMap;
use std::path::Path;

/// Always: UV_CACHE_DIR=<home>/store/uv_cache, UV_LINK_MODE=clone, PNPM_HOME/npm_config_store_dir=<home>/store/pnpm_store.
/// If !isolated: HF_HOME, TORCH_HOME, OLLAMA_MODELS, PLAYWRIGHT_BROWSERS_PATH under <home>/store/shared_cache/.
/// Creates the directories if missing.
pub fn inject_shared_storage_env(env: &mut HashMap<String, String>, homemade_home: &Path, isolated: bool) {
    let _ = (env, homemade_home, isolated);
    // agent C
}

/// Create ~/.homemade/{store/uv_cache,store/pnpm_store,store/shared_cache/{huggingface,torch,ollama,playwright},sessions,skills}.
pub fn ensure_layout(homemade_home: &Path) -> std::io::Result<()> {
    for p in ["store/uv_cache", "store/pnpm_store", "store/shared_cache/huggingface", "store/shared_cache/torch", "store/shared_cache/ollama", "store/shared_cache/playwright", "sessions", "skills/homemade"] {
        std::fs::create_dir_all(homemade_home.join(p))?;
    }
    Ok(())
}
