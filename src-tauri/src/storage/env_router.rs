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

/// Expand `$NAME` / `${NAME}`: aliases first, then login env; unknown names are left literal.
pub fn expand_vars(value: &str, aliases: &HashMap<String, String>, login_env: &HashMap<String, String>) -> String {
    let look = |n: &str| aliases.get(n).or_else(|| login_env.get(n)).cloned();
    let chars: Vec<char> = value.chars().collect();
    let mut out = String::with_capacity(value.len());
    let mut i = 0;
    let is_start = |c: char| c == '_' || c.is_ascii_alphabetic();
    let is_body = |c: char| c == '_' || c.is_ascii_alphanumeric();
    while i < chars.len() {
        let c = chars[i];
        if c == '$' && i + 1 < chars.len() {
            if chars[i + 1] == '{' {
                if let Some(end) = chars[i + 2..].iter().position(|&x| x == '}') {
                    let name: String = chars[i + 2..i + 2 + end].iter().collect();
                    if !name.is_empty() && name.chars().next().map(is_start).unwrap_or(false) && name.chars().all(is_body) {
                        if let Some(v) = look(&name) { out.push_str(&v); i += end + 3; continue; }
                    }
                }
            } else if is_start(chars[i + 1]) {
                let mut j = i + 1;
                while j < chars.len() && is_body(chars[j]) { j += 1; }
                let name: String = chars[i + 1..j].iter().collect();
                if let Some(v) = look(&name) { out.push_str(&v); i = j; continue; }
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Collect shared_cache entries modified at/after `since_ms` (dir mtime or any file within depth 2).
/// Returns (asset_hash "<cache>/<name>", abs path, size bytes).
pub fn scan_shared_cache_changes(homemade_home: &Path, since_ms: i64) -> Vec<(String, String, i64)> {
    fn mtime_ms(p: &Path) -> i64 {
        std::fs::symlink_metadata(p).and_then(|m| m.modified()).ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_millis() as i64).unwrap_or(0)
    }
    fn touched(p: &Path, since: i64, depth: u32) -> bool {
        if mtime_ms(p) >= since { return true; }
        if depth == 0 { return false; }
        let Ok(rd) = std::fs::read_dir(p) else { return false };
        rd.flatten().any(|e| {
            let ft = e.file_type().ok();
            if ft.map(|f| f.is_dir()).unwrap_or(false) { touched(&e.path(), since, depth - 1) } else { mtime_ms(&e.path()) >= since }
        })
    }
    fn dir_size(p: &Path) -> i64 {
        let Ok(m) = std::fs::symlink_metadata(p) else { return 0 };
        if !m.is_dir() { return m.len() as i64; }
        std::fs::read_dir(p).map(|rd| rd.flatten().map(|e| dir_size(&e.path())).sum()).unwrap_or(0)
    }
    let mut out = vec![];
    let shared = homemade_home.join("store").join("shared_cache");
    let Ok(caches) = std::fs::read_dir(&shared) else { return out };
    for cache in caches.flatten() {
        if !cache.file_type().map(|f| f.is_dir()).unwrap_or(false) { continue; }
        let cname = cache.file_name().to_string_lossy().to_string();
        let Ok(entries) = std::fs::read_dir(cache.path()) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if touched(&p, since_ms, 2) {
                out.push((format!("{cname}/{}", e.file_name().to_string_lossy()), p.to_string_lossy().to_string(), dir_size(&p)));
            }
        }
    }
    out
}
