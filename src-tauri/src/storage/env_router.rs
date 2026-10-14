//! Shared store env injection (spec §8A). OWNER: agent C.
use std::collections::HashMap;
use std::path::Path;

/// Always: UV_CACHE_DIR=<home>/store/uv_cache, UV_LINK_MODE=clone, PNPM_HOME/npm_config_store_dir=<home>/store/pnpm_store.
/// If !isolated: HF_HOME, TORCH_HOME, OLLAMA_MODELS, PLAYWRIGHT_BROWSERS_PATH under <home>/store/shared_cache/.
/// Creates the directories if missing.
pub fn inject_shared_storage_env(env: &mut HashMap<String, String>, homemade_home: &Path, isolated: bool) {
    let store = homemade_home.join("store");
    let mut set = |k: &str, p: std::path::PathBuf| {
        let _ = std::fs::create_dir_all(&p);
        env.insert(k.into(), p.to_string_lossy().into_owned());
    };
    set("UV_CACHE_DIR", store.join("uv_cache"));
    set("PNPM_HOME", store.join("pnpm_store"));
    set("npm_config_store_dir", store.join("pnpm_store"));
    if !isolated {
        let shared = store.join("shared_cache");
        set("HF_HOME", shared.join("huggingface"));
        set("TORCH_HOME", shared.join("torch"));
        set("OLLAMA_MODELS", shared.join("ollama"));
        set("PLAYWRIGHT_BROWSERS_PATH", shared.join("playwright"));
    }
    env.insert("UV_LINK_MODE".into(), "clone".into());
}

/// Create ~/.homemade/{store/uv_cache,store/pnpm_store,store/shared_cache/{huggingface,torch,ollama,playwright},sessions,skills}.
pub fn ensure_layout(homemade_home: &Path) -> std::io::Result<()> {
    for p in ["store/uv_cache", "store/pnpm_store", "store/shared_cache/huggingface", "store/shared_cache/torch", "store/shared_cache/ollama", "store/shared_cache/playwright", "sessions", "skills/homemade"] {
        std::fs::create_dir_all(homemade_home.join(p))?;
    }
    Ok(())
}
