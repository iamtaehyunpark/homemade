---
name: homemade
description: Build, configure, and register personal tools for the Homemade launcher. Use when creating or editing a project that has (or should have) a homemade.toml, adding a custom ui/index.html frontend via window.homemade, emitting @homemade: progress/artifact directives, or writing a warm homemade_worker RPC app.
---

# Homemade — Agent Integration Skill

Homemade is a local launcher for personal, AI-generated software. It reads `homemade.toml` in the project root,
renders a form from `[inputs]`, runs the program, streams progress, and shows output files. Follow this spec exactly.

## 1. Environment & dependency rules (CRITICAL)
- **NEVER** create a standalone `python -m venv .venv` or download duplicate model weights into the project folder.
- Use `uv` (`pyproject.toml` or inline script metadata). Homemade routes `UV_CACHE_DIR`, `HF_HOME`, `TORCH_HOME` to a shared Copy-on-Write store.
- Always support plain CLI execution (`argparse` or `typer`), OR `homemade_worker` warm RPC mode.
- Write final outputs to the directory in the **`HOMEMADE_OUTPUT_DIR`** env var (absolute path; falls back to `./outputs` when run outside Homemade).
  Other env vars provided: `HOMEMADE_RUN_ID`, `HOMEMADE_APP_ID`, `HOMEMADE_ROOT`.

## 2. `homemade.toml`
Place it in the project root. Declare every CLI parameter under `[inputs.<name>]` so Homemade can validate, save presets, and
build "Copy as CLI" strings. Input `type` ∈ `file | directory | file_list | string | int | float | bool | select | range | args`.
`type = "args"` is a free-text field whose value is shell-split and appended verbatim (use it for pass-through extra arguments).

```toml
[app]
id = "frame-extractor"
name = "Frame Extractor"
description = "Extract frames from a video"
mode = "cli"                     # "cli" | "rpc_worker" | "webapp"
exec = "uv run python main.py"   # inputs are appended as flags / positionals
env_manager = "uv"               # "uv" | "pnpm" | "cargo" | "byo" | "system"

[inputs.input_file]
type = "file"
positional_index = 0
required = true
extensions = [".mp4", ".mov"]

[inputs.fps]
type = "float"
cli_flag = "--fps"
default = 1.0
min = 0.1
max = 60

[inputs.grayscale]
type = "bool"
cli_flag = "--grayscale"
bool_style = "flag_only"

[ui]
entry = "ui/index.html"          # omit for an auto-generated form

[resources]
requires_gpu = false
# vram_estimate_mb = 4000        # set with requires_gpu = true for PyTorch/CUDA/Metal

[inputs.extra]
type = "args"                    # free text, shell-split and appended as-is

[environment]
secrets = ["OPENAI_API_KEY"]     # pulled from the Homemade keychain vault at run time
vars = { MODEL_DIR = "$MODELS/whisper" }   # $NAME / ${NAME} expand from [aliases] in ~/.homemade/config.toml, then the login env

[artifacts]
output_dir = "./outputs"
```

## 3. Custom web frontend (`ui/index.html`)
Only when the tool benefits from it (charts, image comparison, canvases, dashboards). Set `[ui] entry = "ui/index.html"`.
**Do NOT spin up a Flask/FastAPI server** to talk to Python. Use the pre-injected `window.homemade` bridge:
`run(inputs)` (resolves with the final run record), `call(method, params)`, `stop()`, `pickFile({extensions})`, `pickFolder()`,
`assetUrl(path)`, `getSchema()`, `getPresets()`, `savePreset(name, inputs)`, `listArtifacts()`, `reveal(path)`, and
`onStdout / onStderr / onProgress / onMetric / onArtifact / onStatus / onFileDrop(cb)` (each returns an unsubscribe fn).

```html
<!doctype html>
<html><body style="font-family:system-ui;padding:16px">
  <button id="pick">Pick video</button> <button id="go" disabled>Run</button>
  <progress id="bar" max="100" value="0" style="width:100%"></progress>
  <p id="msg"></p><img id="out" style="max-width:100%">
<script>
  let file = null; const $ = (id) => document.getElementById(id);
  $("pick").onclick = async () => { file = await window.homemade.pickFile({ extensions: [".mp4"] }); $("go").disabled = !file; };
  window.homemade.onProgress(({ percent, message }) => { $("bar").value = percent; $("msg").textContent = message; });
  window.homemade.onArtifact((a) => { $("out").src = window.homemade.assetUrl(a.filePath); });
  $("go").onclick = async () => { const r = await window.homemade.run({ input_file: file, fps: 1 }); $("msg").textContent = r.status; };
</script>
</body></html>
```

## 4. Live progress & artifacts from Python
Print `@homemade:` directives to stdout, one per line, with `flush=True`:
- `print('@homemade:progress 50 "Halfway done..."', flush=True)`
- `print(f'@homemade:artifact "{abs_output_path}"', flush=True)`
- `print('@homemade:metric {"accuracy": 0.98}', flush=True)`
- `print('@homemade:port 8421', flush=True)` (webapp mode: announce the port you listen on)

## 5. Warm RPC worker mode (`mode = "rpc_worker"`)
For expensive-to-load models, keep the process alive and expose functions. `homemade_worker` is importable directly
(Homemade puts its resources dir on `PYTHONPATH`; do not pip-install it):

```python
from homemade_worker import expose, run_worker, progress

@expose
def add(a: float, b: float) -> float:
    return a + b

@expose
def slow(n: int) -> str:
    for i in range(n):
        progress(100 * (i + 1) / n, f"step {i + 1}/{n}")
    return "done"

if __name__ == "__main__":
    run_worker()   # prints @homemade:worker_ready, then serves JSON RPC on stdin/stdout
```
Call from the UI with `await window.homemade.call("add", { a: 1, b: 2 })`.
Helpers: `progress(pct, msg)`, `artifact(path)`, `metric(dict)`. Do not run the script with `python -I` (it hides PYTHONPATH).

## 6. MCP server (inspect Homemade state from your agent)
Homemade's binary doubles as a read-only MCP stdio server. Register it once:

```sh
claude mcp add homemade -- /Applications/Homemade.app/Contents/MacOS/homemade --mcp
```

Tools: `list_apps`, `get_app(app_id)` (parsed manifest + root_dir + has_custom_ui), `list_runs(app_id, limit?)`,
`get_run_log(run_id)`, `last_failed_run(app_id)` (use this first when fixing a broken tool), `list_artifacts(app_id, run_id?)`,
`validate_manifest(toml_text)` (returns `{ok, errors[], normalized_toml}`; run it after editing `homemade.toml`).
Runs cannot be started over MCP; the user starts them from the Homemade window.
