//! Build argv / display command from manifest + inputs. OWNER: agent B.
use crate::error::Result;
use crate::manifest::schema::HomemadeManifest;
use serde_json::Value;

/// Build the full argv: `shell_words::split(manifest.app.exec)` followed by input flags.
/// Rules (spec §3): positional_index fields in index order; `cli_flag` fields as `--flag value`
/// (int/float/string/select/range/file/directory); bool: flag_only => `--flag` only when true,
/// key_value => `--flag true|false`; file_list => repeat `--flag path` per item. Skip None/empty
/// values unless `required` (then Err with the field label). Use `default` when the input is absent.
pub fn build_argv(manifest: &HomemadeManifest, inputs: &Value) -> Result<Vec<String>> {
    let _ = (manifest, inputs);
    todo!("agent B")
}

/// Shell-escaped single-line string, prefixed with `cd <root> && ` for "Copy as CLI".
pub fn display_command(root_dir: &str, argv: &[String]) -> String {
    format!("cd {} && {}", shell_words::quote(root_dir), shell_words::join(argv))
}
