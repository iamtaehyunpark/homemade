//! ~/.homemade/config.toml access. Missing/invalid file => empty table.
use std::path::Path;

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
