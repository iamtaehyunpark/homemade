//! OS keychain vault via `keyring` (spec §8A2). OWNER: agent C.
//! Service name "homemade"; secret *names* are listed in <home>/config.toml under [secrets] names = [...].
use crate::error::{err, Result};
use std::collections::HashMap;
use std::path::Path;

const SERVICE: &str = "homemade";

fn read_config(home: &Path) -> Result<toml::Table> {
    match std::fs::read_to_string(home.join("config.toml")) {
        Ok(s) => Ok(s.parse::<toml::Table>()?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(toml::Table::new()),
        Err(e) => Err(e.into()),
    }
}

fn write_names(home: &Path, names: &[String]) -> Result<()> {
    let mut cfg = read_config(home)?;
    let secrets = cfg.entry("secrets").or_insert_with(|| toml::Value::Table(toml::Table::new()));
    if !secrets.is_table() { *secrets = toml::Value::Table(toml::Table::new()); }
    secrets.as_table_mut().unwrap().insert("names".into(), toml::Value::Array(names.iter().map(|n| toml::Value::String(n.clone())).collect()));
    std::fs::create_dir_all(home)?;
    let out = toml::to_string_pretty(&cfg).map_err(|e| crate::error::Error::Msg(e.to_string()))?;
    std::fs::write(home.join("config.toml"), out)?;
    Ok(())
}

fn entry(name: &str) -> Result<keyring::Entry> {
    keyring::Entry::new(SERVICE, name).map_err(|e| crate::error::Error::Msg(format!("Keychain error: {e}")))
}

pub fn list_names(home: &Path) -> Result<Vec<String>> {
    let cfg = read_config(home)?;
    Ok(cfg.get("secrets").and_then(|s| s.get("names")).and_then(|n| n.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default())
}

pub fn set(home: &Path, name: &str, value: &str) -> Result<()> {
    let name = name.trim();
    if name.is_empty() { return err("Secret name is empty"); }
    entry(name)?.set_password(value).map_err(|e| crate::error::Error::Msg(format!("Keychain error: {e}")))?;
    let mut names = list_names(home)?;
    if !names.iter().any(|n| n == name) { names.push(name.to_string()); write_names(home, &names)?; }
    Ok(())
}

pub fn delete(home: &Path, name: &str) -> Result<()> {
    match entry(name)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => {}
        Err(e) => return err(format!("Keychain error: {e}")),
    }
    let names: Vec<String> = list_names(home)?.into_iter().filter(|n| n != name).collect();
    write_names(home, &names)
}

/// Resolve the requested names; Err("Missing secret: X") on the first missing one.
pub fn resolve(names: &[String]) -> Result<HashMap<String, String>> {
    let mut out = HashMap::new();
    for n in names {
        match entry(n).and_then(|e| e.get_password().map_err(|e| e.to_string().into())) {
            Ok(v) => { out.insert(n.clone(), v); }
            Err(_) => return err(format!("Missing secret: {n}")),
        }
    }
    Ok(out)
}
/// Replace any secret value occurrences in `text` with "***".
pub fn redact(text: &str, values: &HashMap<String, String>) -> String {
    let mut out = text.to_string();
    for v in values.values() { if v.len() >= 4 { out = out.replace(v, "***"); } }
    out
}
