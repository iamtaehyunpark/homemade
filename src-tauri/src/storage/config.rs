//! ~/.homemade/config.toml access. Missing/invalid file => empty table.
use std::path::{Path, PathBuf};

/// `$HOMEMADE_HOME` if set (tests, parallel profiles), else `~/.homemade`.
pub fn homemade_home() -> PathBuf {
    if let Some(h) = std::env::var_os("HOMEMADE_HOME").filter(|v| !v.is_empty()) { return PathBuf::from(h); }
    dirs::home_dir().expect("home dir").join(".homemade")
}

/// Load `<home>/config.toml` as a TOML table (empty on missing or parse error).
pub fn load(home: &Path) -> toml::Table {
    std::fs::read_to_string(home.join("config.toml"))
        .ok()
        .and_then(|s| s.parse::<toml::Table>().ok())
        .unwrap_or_default()
}

/// `[section] key = bool`, else `default`.
pub fn get_bool(table: &toml::Table, section: &str, key: &str, default: bool) -> bool {
    table.get(section).and_then(|v| v.as_table()).and_then(|t| t.get(key)).and_then(|v| v.as_bool()).unwrap_or(default)
}

/// String-valued entries of `[name]` (non-string values are skipped).
pub fn get_table(table: &toml::Table, name: &str) -> std::collections::HashMap<String, String> {
    table.get(name).and_then(|v| v.as_table())
        .map(|t| t.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect())
        .unwrap_or_default()
}

/// Where new projects go when the user does not pick a folder: `[projects] dir` in config.toml (with `~` expanded),
/// else `~/homemade`. Created on first use, not here.
pub fn projects_dir(home: &Path) -> PathBuf {
    let cfg = load(home);
    let configured = cfg.get("projects").and_then(|t| t.get("dir")).and_then(|v| v.as_str()).map(str::trim).filter(|s| !s.is_empty());
    let user_home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/"));
    match configured {
        Some(d) if d == "~" => user_home,
        Some(d) if d.starts_with("~/") => user_home.join(&d[2..]),
        Some(d) => PathBuf::from(d),
        None => user_home.join("homemade"),
    }
}
