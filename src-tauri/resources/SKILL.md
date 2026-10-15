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

## 6. Driving the launcher (MCP + CLI) — full control, same supervisor as the GUI
Homemade's binary is also an MCP stdio server and a CLI. While the GUI is running, both talk to it over a local control
socket, so an agent can do everything the user can: register folders, start/stop/wait for runs, read logs and outputs,
save presets, checkpoint with git, and open programs as tabs or standalone windows. Without the GUI only the read-only
tools work; call `launch_gui` (or run `homemade launch`) first.

Register the MCP server once:
```sh
claude mcp add homemade -- /Applications/Homemade.app/Contents/MacOS/homemade --mcp
```
Tools (arguments in parentheses): `status`, `launch_gui`, `list_apps`, `get_app(app_id)`, `import_app(path)`, `create_project(name, parent_dir?)` (default `~/homemade/<name>`),
`remove_app(app_id)`, `draft_manifest(path)` → `apply_manifest(path, toml)`, `save_manifest(app_id, toml)`,
`validate_manifest(toml_text)`, `start_run(app_id, inputs)` → `wait_run(run_id)` (returns exit_code + log_tail),
`stop_run(run_id)`, `get_run(run_id)`, `list_runs(app_id)`, `list_active_runs`, `get_run_log(run_id)`, `last_failed_run(app_id)`,
`explain_run(run_id)`, `build_cli_command(app_id, inputs)`, `rpc_call(app_id, method, params)`, `list_artifacts(app_id, run_id?)`,
`list_presets` / `save_preset`, `git_status` / `git_checkpoint(app_id, message)`, `open_app(app_id, window?)`, `close_window(app_id)`,
`show_view(view)`, `resource_snapshot`, `list_listening_ports`, `kill_port_owner(pid)`.

The CLI (`~/.homemade/bin/homemade`; add that dir to PATH, or call the binary inside the .app) mirrors the tools and is handy
from a shell or a script. Inputs are given as `key=value` or `--key value` and coerced by the manifest's declared types:
```sh
homemade status
homemade import ~/code/frame-extractor          # register (infers homemade.toml if missing)
homemade run frame-extractor input_file=a.mp4 --fps 2 --wait   # exit 0 on success; log tail on failure
homemade log <run-id> --follow
homemade open frame-extractor --window          # standalone program window
homemade help                                   # everything else; add --json for machine output
```
Inside anything Homemade spawns (program runs, "Edit with Agent" terminals, shells) this is already set up: `homemade` is on
PATH, `$HOMEMADE_CLI` is the binary, `$HOMEMADE_MCP_COMMAND` is the MCP command, `$HOMEMADE_MCP_CONFIG` is a ready-made
`--mcp-config` file, and Claude Code / Codex sessions started from Homemade have the `homemade` MCP server connected.
So from an agent session you can run and verify the app yourself (`start_run` + `wait_run`, or `homemade run … --wait`)
instead of asking the user to press Run.

While you work inside a Homemade agent session, the launcher keeps you posted: whenever a run of your app starts, finishes
or gets a failure diagnosis (including runs the user starts from the window), a message beginning with `[homemade]` is typed
into your session. Treat it as live ground truth and react (inspect the log, fix, re-run). `app_activity(app_id)` returns the
same picture on demand: current run with live log tail, last result, latest outputs. (`[agents] notify_runs = false` in
`~/.homemade/config.toml` turns the messages off.)

The user can also point at things: with **Select element** (workspace toolbar, ⌘⇧C) they click an element in the launcher and a reference like
`[ui: KoGrammar › Underbar › Logs tab › log-line "Traceback…" (app kogrammar)]` appears in your input, followed by their
request. Resolve it with `app_activity` / `get_run_log` / `list_artifacts`; a `web view … at (x,y)` reference points into the
program's own UI — `capture(app_id)` gives you a screenshot to look at. For deeper inspection of the running program use
`app_processes` (pid tree), `app_usage` (CPU/RAM/GPU over time), `signal_run`, `write_stdin`, and `get_run_log` with `offset`
to tail incrementally.

If you were started outside Homemade, look for the managed `## Homemade` block in the project's `CLAUDE.md` / `AGENTS.md`:
it names the app id and points at the CLI shim and the MCP config.

Typical adapt-and-verify loop for an agent: `draft_manifest` → edit → `apply_manifest` → `start_run` + `wait_run` →
read `log_tail` / `explain_run` → fix the code → `git_checkpoint`.
