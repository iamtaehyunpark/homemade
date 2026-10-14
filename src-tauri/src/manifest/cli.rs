//! Build argv / display command from manifest + inputs. OWNER: agent B.
use crate::error::{err, Result};
use crate::manifest::schema::HomemadeManifest;
use serde_json::Value;

/// Build the full argv: `shell_words::split(manifest.app.exec)` followed by input flags.
/// Rules (spec §3): positional_index fields in index order; `cli_flag` fields as `--flag value`
/// (int/float/string/select/range/file/directory); bool: flag_only => `--flag` only when true,
/// key_value => `--flag true|false`; file_list => repeat `--flag path` per item. Skip None/empty
/// values unless `required` (then Err with the field label). Use `default` when the input is absent.
pub fn build_argv(manifest: &HomemadeManifest, inputs: &Value) -> Result<Vec<String>> {
    let mut argv = shell_words::split(&manifest.app.exec)
        .map_err(|e| crate::error::Error::Msg(format!("Invalid exec string: {e}")))?;
    if argv.is_empty() { return err("Manifest exec is empty"); }
    let mut positionals: Vec<(usize, Vec<String>)> = vec![];
    let mut flags: Vec<String> = vec![];
    for (key, spec) in &manifest.inputs {
        let label = spec.label.clone().unwrap_or_else(|| key.clone());
        let raw = inputs.get(key).filter(|v| !v.is_null()).or(spec.default.as_ref()).cloned().unwrap_or(Value::Null);
        let empty = match &raw {
            Value::Null => true,
            Value::String(s) => s.is_empty(),
            Value::Array(a) => a.is_empty(),
            _ => false,
        };
        let required = spec.required.unwrap_or(false);
        if empty {
            if required { return err(format!("Missing required input: {label}")); }
            continue;
        }
        let ft = spec.field_type.as_str();
        if ft == "bool" {
            let b = match &raw { Value::Bool(b) => *b, Value::String(s) => s == "true", Value::Number(n) => n.as_f64() != Some(0.0), _ => false };
            let Some(flag) = &spec.cli_flag else {
                if let Some(i) = spec.positional_index { positionals.push((i, vec![b.to_string()])); }
                continue;
            };
            if spec.bool_style.as_deref() == Some("key_value") {
                flags.push(flag.clone()); flags.push(b.to_string());
            } else if b {
                flags.push(flag.clone());
            }
            continue;
        }
        let values: Vec<String> = if ft == "file_list" {
            match &raw { Value::Array(a) => a.iter().map(scalar).filter(|s| !s.is_empty()).collect(), other => vec![scalar(other)] }
        } else {
            let v = scalar(&raw);
            if ft == "select" {
                if let Some(opts) = &spec.options {
                    if !opts.iter().any(|o| o == &v) {
                        return err(format!("Invalid value for {label}: '{v}' (expected one of {})", opts.join(", ")));
                    }
                }
            }
            vec![v]
        };
        if let Some(flag) = &spec.cli_flag {
            for v in values { flags.push(flag.clone()); flags.push(v); }
        } else if let Some(i) = spec.positional_index {
            positionals.push((i, values));
        }
    }
    positionals.sort_by_key(|(i, _)| *i);
    for (_, vs) in positionals { argv.extend(vs); }
    argv.extend(flags);
    Ok(argv)
}

fn scalar(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() { i.to_string() }
            else if let Some(f) = n.as_f64() { if f.fract() == 0.0 && f.abs() < 1e15 { (f as i64).to_string() } else { f.to_string() } }
            else { n.to_string() }
        }
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// Shell-escaped single-line string, prefixed with `cd <root> && ` for "Copy as CLI".
pub fn display_command(root_dir: &str, argv: &[String]) -> String {
    format!("cd {} && {}", shell_words::quote(root_dir), shell_words::join(argv))
}
