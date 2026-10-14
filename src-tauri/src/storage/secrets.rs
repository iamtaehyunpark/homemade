//! OS keychain vault via `keyring` (spec §8A2). OWNER: agent C.
//! Service name "homemade"; secret *names* are listed in <home>/config.toml under [secrets] names = [...].
use crate::error::Result;
use std::collections::HashMap;
use std::path::Path;

pub fn list_names(home: &Path) -> Result<Vec<String>> { let _ = home; Ok(vec![]) }
pub fn set(home: &Path, name: &str, value: &str) -> Result<()> { let _ = (home, name, value); todo!("agent C") }
pub fn delete(home: &Path, name: &str) -> Result<()> { let _ = (home, name); todo!("agent C") }
/// Resolve the requested names; Err("Missing secret: X") on the first missing one.
pub fn resolve(names: &[String]) -> Result<HashMap<String, String>> { let _ = names; Ok(HashMap::new()) }
/// Replace any secret value occurrences in `text` with "***".
pub fn redact(text: &str, values: &HashMap<String, String>) -> String {
    let mut out = text.to_string();
    for v in values.values() { if v.len() >= 4 { out = out.replace(v, "***"); } }
    out
}
