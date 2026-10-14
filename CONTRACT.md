# Homemade — Implementation Contract (v1)

This file is the single source of truth that lets several agents implement disjoint modules in parallel.
Read `spec.md` for the *why* and the detailed behavior; read this for the *interfaces*. **Do not rename anything listed here.**

## Layout & ownership

| Owner | Files (exclusive) |
|---|---|
| **A supervisor** | `supervisor/spawner.rs`, `supervisor/stdout_parser.rs`, `supervisor/reaper.rs`, `commands/runs.rs` (except `build_cli_command`), `resources/homemade_worker.py` |
| **B manifest** | `manifest/infer.rs`, `manifest/cli.rs`, `manifest/watcher.rs`, `commands/apps.rs`, `commands/presets.rs`, `build_cli_command` in `commands/runs.rs`, `resources/ast_inspector.py` |
| **C storage+scheduler** | `supervisor/scheduler.rs`, `supervisor/telemetry.rs`, `storage/*.rs`, `commands/artifacts.rs`, `commands/storage.rs`, `commands/resources.rs` |
| **D agents+git+pty** | `supervisor/pty_manager.rs`, `integrations/*.rs`, `commands/pty.rs`, `commands/agents.rs`, `commands/git.rs`, `resources/SKILL.md` |
| **E protocols+bridge** | `protocols/*.rs`, `resources/homemade-bridge.js`, `examples/` sample apps |
| **F frontend** | everything under `src/` (React) |

Shared, already written, **read-only for agents** (if you truly need a change, add new items, never rename/remove): `state.rs`, `types.rs`, `error.rs`, `db/*.rs`, `manifest/schema.rs`, `commands/mod.rs`, `commands/system.rs`, `lib.rs`, `src/lib/types.ts`, `src/lib/ipc.ts`.
Exception: the owner of a struct may add fields to it (A: `ActiveRun`, D: `ActivePty`) and must update every constructor.

Rules for every agent:
- `cd src-tauri && cargo check` must pass when you finish (warnings OK). Frontend: `pnpm typecheck`.
- Replace every `todo!("agent X")` in your files. Leave other agents' `todo!`s alone.
- Lock order: never hold `state.db` (std Mutex) across an `.await`. Lock, query, drop, then await.
- Emit events with `state.emit(name, payload)`; payloads are `serde` structs/`serde_json::json!` with **camelCase** keys.
- Errors: return `crate::error::Result`; `crate::error::err("message")` for ad-hoc errors.
- No new crates unless unavoidable (then add to `Cargo.toml` and say so in your report).
- Don't spend time on tests. Make it work, make it compile.

## Tauri commands (frontend ⇄ Rust)

All commands are listed in `src/lib/ipc.ts` with exact argument names (camelCase in TS, snake_case in Rust; Tauri converts). The Rust signatures live in `src-tauri/src/commands/*.rs`.

## Events (Rust → frontend)

| Event | Payload (camelCase JSON) | Emitted by |
|---|---|---|
| `run://status` | `RunRecord` | A (spawn, stop, exit), C (queued) |
| `run://queued` | `{runId, appId, requiredVramMb, reason}` | C |
| `run://stdout` / `run://stderr` | `{runId, line}` | A |
| `run://progress` | `{runId, percent, message}` | A |
| `run://metric` | `{runId, data}` | A |
| `run://artifact` | `ArtifactRecord` | A (directive + post-run diff) |
| `run://port` | `{runId, port}` | A |
| `run://worker_ready` | `{runId}` | A |
| `pty://data` | `{ptyId, data}` | D |
| `pty://exit` | `{ptyId, exitCode}` | D |
| `app://changed` | `{appId}` | B (homemade.toml changed) |
| `app://ui_reload` | `{appId}` | B (ui/ changed) |
| `app://confirm-exit` | `number` (active count) | lib.rs close guard |
| `resources://tick` | `ResourceSnapshot` | C (1 Hz) |
| `storage://progress` | `{appId, message}` | C (venv migration) |

## Run lifecycle (A ⇄ C)

`commands::runs::start_run` (A) → insert `runs` row with status `running` and `started_at=now` → build `PendingJob` (argv via `manifest::cli::build_argv` (B), `cli_command` via `manifest::cli::display_command`) → `scheduler::submit(state, job)` (C).
C either calls `spawner::launch(state, job)` (A) immediately, or sets DB status `queued_vram`, pushes to `state.vram_queue`, emits `run://status` + `run://queued`.
A's `launch` inserts `ActiveRun`, streams lines through `stdout_parser::handle_line`, and on exit calls `scheduler::on_run_finished(state)` so C can admit the next job.
`stop_run` (A): if run in `active_runs` → reaper; else `scheduler::remove_queued` then `finish_run(..., "killed")`.
`force_run` (A command) → `scheduler::force(state, run_id)` (C).

## `@homemade:` stdout directives (A parses; E's worker + skill emit them)

```
@homemade:progress 42 "Extracting frames..."
@homemade:artifact "/abs/path/chart.png"
@homemade:metric {"fps": 58.4}
@homemade:port 8421
@homemade:worker_ready
@homemade:rpc_response {"id":"...","ok":true,"result":...}
```
RPC request written to worker stdin (A): `{"id":"<uuid>","method":"name","params":{...}}\n`. Shutdown: `{"method":"__shutdown__"}`.

## Child environment (A assembles, C provides pieces)

`login_env` + `storage::env_router::inject_shared_storage_env` + `manifest.environment.vars` + `storage::secrets::resolve(secrets)` + `HOMEMADE_RUN_ID`, `HOMEMADE_APP_ID`, `HOMEMADE_ROOT`, `HOMEMADE_OUTPUT_DIR` (absolute). cwd = `root_dir/manifest.app.cwd`.

## Protocols (E) and the iframe bridge (E ⇄ F)

- `homemade-asset://localhost/<encodeURIComponent(absPath)>` streams any file with Range support. Frontend helper: `assetUrl()` in `src/lib/ipc.ts`.
- `homemade-app://<app_id>.localhost/<rel>` serves the directory containing `manifest.ui.entry`; HTML gets `<script src="/__homemade/bridge.js">` injected; `/__homemade/bridge.js` serves `resources/homemade-bridge.js`. Frontend helper: `appUiUrl()`.
  On macOS Tauri rewrites custom schemes as `homemade-app://<app_id>.localhost/...`; on Linux as `http://homemade-app.<app_id>.localhost/`? **No** — on Linux the host is `homemade-app.localhost` and the app id must be passed as the first path segment. E must handle both: derive app_id from the host's first label if it isn't `localhost`/`homemade-app`, else from the first path segment, and the bridge/`appUiUrl` must build `homemade-app://localhost/<app_id>/index.html` on Linux (F: detect via `navigator.platform`/`type()` from `@tauri-apps/plugin-os`; v1 can hardcode the macOS form and note Linux as follow-up).
- Bridge ⇄ host postMessage protocol (E implements the iframe side in `homemade-bridge.js`, F implements the host side in `AppSandbox.tsx`):
  - iframe → host: `{ homemade: 1, type: "rpc", id, method, params }` where method ∈ `run | call | stop | pick_file | pick_folder | get_schema | get_presets | save_preset | list_artifacts | reveal | log`.
  - host → iframe: `{ homemade: 1, type: "rpc_result", id, ok, result | error }`.
  - host → iframe events: `{ homemade: 1, type: "event", name, payload }` with name ∈ `stdout | stderr | progress | metric | artifact | status | file_drop | worker_ready`.
  - Bridge sends `{ homemade: 1, type: "ready" }` on load; host replies with `{ homemade:1, type:"init", appId, schema, platform }`.
  - `window.homemade` API (spec §7B, renamed): `run(inputs)→Promise<RunRecord(final)>`, `call(method, params)`, `stop()`, `pickFile(opts)`, `pickFolder()`, `assetUrl(path)`, `getSchema()`, `getPresets()`, `savePreset(name, inputs)`, `listArtifacts()`, `reveal(path)`, `onStdout/onStderr/onProgress/onMetric/onArtifact/onStatus/onFileDrop(cb)` each returning an unsubscribe fn.
  - `run()` resolves when the host sees a `run://status` with a terminal status for the run it started.

## Sample apps (E writes under `examples/`, used by everyone for manual testing)

1. `examples/hello-cli/` — `main.py` argparse (`--name`, `--count int`, `--shout store_true`, positional `input_file`), prints `@homemade:progress` lines and writes `outputs/greeting.txt` + `@homemade:artifact`. `homemade.toml` fully declared. No deps (`exec = "python3 main.py"`, `env_manager = "system"`).
2. `examples/hello-ui/` — same script but with `ui/index.html` using `window.homemade.run`, `onProgress`, `assetUrl` to show the output.
3. `examples/hello-worker/` — `mode = "rpc_worker"`, imports `homemade_worker` (copy the file into the example or add `resources/` to `PYTHONPATH` — A: add `HOMEMADE_RESOURCES` to the child env and put the resources dir on `PYTHONPATH`), exposes `add(a,b)` and `slow(n)` emitting progress; UI calls them.
4. `examples/no-manifest/` — a bare argparse script with no `homemade.toml`, to exercise inference.

## Gap-closing pass (2026-10-13, round 2)

### E2E harness (owned by the lead; agents must keep it working)
- Rust: `HOMEMADE_E2E=<script.js>` evals the script in the main webview ~1.5s after setup. Commands `e2e_report(msg)` prints `[e2e] msg` to stderr; `e2e_done(code)` exits the process.
- Frontend exposes `window.__hm` (always, it's harmless):
  ```ts
  window.__hm = {
    ipc, events, on,                       // from src/lib/ipc.ts
    store: useStore, live: useLiveStore,   // zustand hooks; use .getState()
    openApp(appId): Promise<void>,         // select in sidebar + open panel; resolves when the panel's DOM exists
    click(sel): void,                      // querySelector + .click()
    setInput(sel, value): void,            // React-safe value set (native setter + input event)
    waitFor(fn: () => any, timeoutMs?): Promise<any>, // polls every 100ms until truthy
    sandboxEval(appId, code: string): Promise<any>,   // runs `code` inside the app's custom-UI iframe via bridge `e2e_eval`; resolves with its result
    termText(ptyId): string,               // current xterm buffer text for an open AgentTerminal
    report(msg): Promise<void>, done(code): Promise<void>,
  }
  ```
- Bridge: accepts `{homemade:1, type:"e2e_eval", id, code}` from `window.parent` **only** after `init` carried `e2e: true`; replies `{homemade:1, type:"e2e_result", id, ok, result|error}`. Host sets `e2e: true` in `init` always (the host is trusted).
- `data-testid` attributes required on: `sidebar-app-<id>`, `import-app`, `run-button`, `stop-button`, `copy-cli`, `preset-save`, `preset-select`, `edit-with-agent`, `git-init`, `git-checkpoint`, `git-log`, `git-revert-<sha>`, `output-shelf`, `shelf-item`, `clean-scratch`, `remove-app`, `log-console`, `log-line` (+ `data-stream="stdout|stderr"`), `progress-bar`, `queue-banner`, `queue-run-anyway`, `venv-dedupe`, `app-sandbox` (iframe; set `data-bridge="ready"` once init was sent), `agent-terminal` (+ `data-pty-id`), `terminal-agent-select`, `terminal-session-select`, `terminal-new`, `confirm-exit-modal`, `settings`, `settings-secrets`, `run-history`, `run-history-item`.

### Backend additions
- `InputFieldSpec.field_type == "args"`: free-text, `build_argv` appends `shell_words::split(value)`; inference fallback emits a single `extra` field of this type when no CLI is detected.
- Env var values: `$NAME`/`${NAME}` expansion from `[aliases]` in `~/.homemade/config.toml`, then login env.
- Log ring: lines from stderr are stored with a `[stderr] ` prefix (frontend strips + colors); directive lines are NOT stored in the ring/log_tail.
- `storage_refs`: after each run, every top-level directory under `~/.homemade/store/shared_cache/*/` that was modified during the run (mtime >= run start) is upserted with its size for that app.
- `--mcp` flag: `main.rs` dispatches to `integrations::mcp_server::run_stdio()` before Tauri starts.

## Gap-closing pass (2026-10-14, round 3)
- `mode = "webapp"` UI: `run://port` is stored in `useStore.ports[runId]`; `AppPanel` renders `WebappFrame` (`data-testid="webapp-frame"`, `data-port`) with an iframe at `http://127.0.0.1:<port>/` while the run is active. No bridge injection.
- Venv badge threshold: Sidebar shows `venv-dedupe` only when `unmanagedVenvBytes > 50 MB` (`VENV_BADGE_MIN_BYTES`).
- `max_ram_mb`: header badge `data-testid="ram-limit"`, labelled "(advisory)" on macOS.
- `venv_scanner::scan_unmanaged_venv` returns 0 for a *managed* venv: one carrying the `.venv/.homemade-managed` marker (written by `migrate_venv`), or a uv-created venv whose `lib/` files are mostly hardlinked (`st_nlink > 1`).
- Test API: `openApp` refreshes the app row from the backend before opening.
- Webapp readiness: after `@homemade:port`, the backend TCP-probes 127.0.0.1:<port> (250 ms, up to 5 min, stops if the run ends) and emits `run://webapp_ready {runId, port}`. `WebappFrame` shows a spinner (`data-phase="waiting-port" | "waiting-server"`) and only mounts the iframe at `data-phase="ready"`; `webapp-reload` button re-mounts it.
- Monitor pane: `useStore.view = "monitor"` (sidebar `monitor-nav`, or click the footer meters). `useStore.resourceHistory` keeps the last 300 `resources://tick` snapshots; `Monitor.tsx` draws SVG graphs (`monitor-cpu|ram|gpu`), a per-run table (`monitor-run` rows with CPU/RAM/GPU mem, peak RAM, sparklines, Stop) and the queue (`monitor-queue`). Footer `resource-current` shows the selected app's active run with CPU %, RAM and GPU memory.
- Process/port tracking: `runs.pgid` and `runs.port` columns (additive migration in `db::open`). `reaper::reap_orphans` runs at startup: any run left running/stopping/queued whose process group is alive is terminated and recorded `killed` with an "orphaned process group … stopped at startup" log note; the UI gets `app://notice {message}` (toast). `list_listening_ports` (lsof LISTEN) attributes ports to runs by pgid (`runId`, `appId`, `active`); `kill_port_owner(pid)` stops only Homemade-launched groups. Monitor shows a "Listening ports" table (`monitor-port` rows, `data-run`).
- Exit guard (macOS): `macos_quit.rs` adds `applicationShouldTerminate:` to tao's NSApp delegate at runtime; Cmd+Q / Dock Quit / AppleScript `quit` / logout answer NSTerminateCancel while anything is active and emit `app://confirm-exit`. Tauri never sees those as `ExitRequested` (tao only implements `applicationWillTerminate`). `RunEvent::ExitRequested` with `code == None` (last window closed) is also prevented while anything is active and emits `app://confirm-exit`, same as window close. `confirm_quit` → `stop_all` → `app.exit(0)` passes (code is Some). `RunEvent::Exit` SIGKILLs any remaining groups as a last resort.
- `tests/e2e/orphan.sh` plants a detached server + a running row before launch; the suite asserts it was reaped.

## Local model (Phase 2, 2026-10-14) — Apple on-device Foundation Model

Goal: ship the spec §12 `SemanticAssistor` for real, with **zero download and zero idle cost**: on macOS 26+ we use Apple's on-device
Foundation Model (already resident in the OS, used by Apple Intelligence) through a tiny Swift helper. Linux / older macOS fall back to
`RuleBasedOnlyAssistor` (feature shows as unavailable; nothing else changes). No GGUF, no llama.cpp, no extra RAM while idle.

### Ownership
- **Agent A (backend):** `src-tauri/swift/homemade-fm.swift`, `src-tauri/scripts/build-fm.sh`, `src-tauri/src/intelligence/{local_model.rs, assistor.rs, repo_facts.rs, draft.rs}`, new commands in `src-tauri/src/commands/model.rs`, wiring into `commands/apps.rs` (import), `supervisor/spawner.rs` (run failure), `supervisor/pty_manager.rs` (session summary), `integrations/mcp_server.rs` (tool), `tauri.conf.json` (bundle the helper), `SKILL.md` (document), unit tests for facts/draft parsing.
- **Agent B (frontend):** `src/components/{Settings.tsx, AppPanel.tsx, Sidebar.tsx, LogConsole.tsx, AgentTerminal.tsx}`, new `src/components/DraftManifestModal.tsx`, `src/lib/{ipc.ts, types.ts, useTauriEvents.ts, testApi.ts}`, `src/state/store.ts`.
- Lead: E2E tests, release build, docs.

### Swift helper `homemade-fm` (Agent A)
- Source `src-tauri/swift/homemade-fm.swift`, built by `src-tauri/scripts/build-fm.sh` with
  `swiftc -O -parse-as-library -framework FoundationModels -o src-tauri/resources/bin/homemade-fm` (arm64, macOS 26+ SDK). The built binary is
  git-ignored; `tauri.conf.json` `build.beforeDevCommand` / `beforeBuildCommand` run the script first on macOS (skip silently when `swiftc` is
  missing or on Linux; the app must still build without it). `bundle.resources` includes `resources/bin/homemade-fm`.
- Protocol: **JSON Lines over stdin/stdout**, one request per line, one response per line, strictly in order:
  - `{"id":"1","op":"status"}` → `{"id":"1","ok":true,"available":true}` or `{"id":"1","ok":true,"available":false,"reason":"appleIntelligenceNotEnabled|deviceNotEligible|modelNotReady|<text>"}`
  - `{"id":"2","op":"complete","system":"<instructions>","prompt":"<user>","max_tokens":1024,"temperature":0.2,"json":true}` →
    `{"id":"2","ok":true,"text":"..."}` or `{"id":"2","ok":false,"error":"..."}`. With `"json":true` the helper strips ``` fences and returns the
    first balanced JSON object/array if it can find one, else the raw text. Each `complete` uses a **fresh** `LanguageModelSession(instructions:)`
    (no cross-request memory). Guardrail refusals and context-window errors come back as `ok:false` with the error text.
  - stderr is free-form logging. Exit when stdin closes.
- The probe that proved this works is at the lead's scratchpad; model answers ~4 s cold, ~1 s warm, JSON-capable.

### Rust `intelligence::local_model` (Agent A)
```rust
pub struct LocalModel { /* Mutex<Option<Child + stdin/stdout>>, last_used, helper path */ }
pub struct ModelStatus { pub provider: String /* "apple" | "none" */, pub available: bool, pub reason: Option<String>, pub helper_path: Option<String> }
impl LocalModel {
    pub fn new(res_dir: &Path, config: &toml::Table) -> Self;   // provider from [model] provider = "apple" (default on macos) | "off"
    pub async fn status(&self) -> ModelStatus;                   // cached for 60 s
    pub async fn complete(&self, system: &str, prompt: &str, json: bool, max_tokens: u32) -> Result<String>; // spawns helper on demand, serializes requests, 120 s timeout
}
```
- Helper idle policy: killed after **120 s** without requests (tokio task), respawned on next call. `AppState` gets `pub model: Arc<LocalModel>` and `pub assistor: Arc<dyn SemanticAssistor>` (Apple-backed when status.available at startup, else `RuleBasedOnlyAssistor`; re-evaluated lazily by `AppleAssistor` itself so toggling Apple Intelligence later works).

### `intelligence::repo_facts` (deterministic, no model) (Agent A)
`pub fn collect(root: &Path, res_dir: &Path) -> RepoFacts` with `to_prompt_text(&self) -> String` (≤ 14 000 chars, truncating the README first):
- `readme`: first README*.md / README* (≤ 7 000 chars), `docs`: up to 8 other markdown files (docs/**, *.md at root; title + first 400 chars each),
- `manifests`: pyproject.toml (`[project]` name/description/scripts/dependencies head), package.json (name/scripts), requirements*.txt (first 40 lines), Cargo.toml, Makefile targets,
- `tree`: top-level entries + `scripts/`, `src/`, `app/` listings (≤ 120 paths, skip .git/.venv/node_modules/__pycache__/outputs),
- `entry_candidates`: for each top-level or `scripts/*.py` with `__main__`/argparse/typer/click/gradio/streamlit/fastapi/flask/http.server: path + framework + inspector inputs (reuse `manifest::infer::run_inspector`), `--help`-like docstring,
- `signals`: has_venv, uses_uv/pnpm/cargo, gpu hints (torch/mlx/cuda/tensorflow in deps), web hints (gradio/streamlit/flask/fastapi/uvicorn/http.server), existing `homemade.toml`.

### `intelligence::draft` (Agent A)
```rust
pub struct DraftResult { pub toml: String, pub agent_prompt: String, pub facts_summary: String, pub validation_errors: Vec<String>, pub used_model: bool, pub notes: Vec<String> }
pub async fn draft_manifest(state: &AppState, root: &Path, res_dir: &Path) -> Result<DraftResult>;
```
- Prompt: system = the manifest reference (embed a compact version of spec §3 + the `@homemade:` directive list + modes explanation) with rules:
  pick `mode` (`cli` for one-shot scripts, `rpc_worker` only if the repo already uses `homemade_worker`, `webapp` when there is a server entry —
  then `exec` must be wrapped exactly like `sh -c 'echo "@homemade:port <PORT>"; exec <cmd> --port <PORT> "$@"' app` when the server does not
  print the port itself), `env_manager` (`uv` if pyproject/requirements and no venv; `byo` if an existing `.venv` with an editable install; `system` for
  dependency-free scripts; `pnpm`/`cargo` by lockfile), `requires_gpu` from deps, `[artifacts]`, and `[inputs]` from the entry's CLI flags
  (types/labels/defaults). Output **JSON only** matching the manifest structure (keys exactly as the TOML tables: app, ui, resources, environment, artifacts, inputs).
- Pipeline: facts → model JSON → `serde_json::from_value::<HomemadeManifest>` → `manifest::schema` validation → `toml::to_string_pretty`.
  On parse/validation failure, **one retry** feeding the error back. If the model is unavailable or both attempts fail, return the deterministic
  `load_or_infer` manifest with `used_model=false` and a note. Never write to disk here.
- `agent_prompt` (markdown, for Claude Code / Codex / agy): "Adapt `<root>` to Homemade" + the manifest reference + the repo facts text + the draft
  TOML + a checklist (verify exec runs from root, confirm inputs, decide mode, add `@homemade:progress/artifact` calls, run once with `homemade`'s
  MCP `validate_manifest`). Must stand alone when pasted into another agent.

### `intelligence::assistor::AppleAssistor` (Agent A) — all three spec §12 methods
- `analyze_log_window(lines)`: last ≤ 120 lines → JSON `{severity:"info|warn|error", message}`; return `None` for `info` with empty message.
  Wired: on run finish with status `failed` (spawner `on_run_finished`) → emit `run://health {runId, severity, message}` and append
  `[homemade] <message>` to the run's `log_tail`. Also on demand via command `explain_run(run_id)` (works for finished runs from `log_tail`).
- `infer_schema_from_raw_code(src)`: JSON `{inputs:{name:{type,cli_flag|positional_index,label,default,required,options,min,max}}}` → `IndexMap<String, InputFieldSpec>`.
  Wired: in `commands::apps::import_app` when the manifest was inferred **and** the inspector produced only the `extra` args fallback → replace inputs,
  rewrite `homemade.toml`, note in `AppRecord`? (no new field; just write the better manifest).
- `summarize_agent_session(diff, transcript)`: one line ≤ 80 chars. Wired: in `pty_manager` on PTY exit when the auto-checkpoint produced a
  different `after` SHA (or the tree is dirty): `git diff --stat <before>..<after>` (≤ 60 lines) + last 60 transcript lines → set
  `agent_sessions.title` when the session has no real title ("New session" / null). Emit `pty://session_summary {sessionId, title}`.

### Commands (Agent A; names exact — Agent B calls these)
| command | args | returns |
|---|---|---|
| `model_status` | – | `ModelStatus` (camelCase) |
| `draft_manifest` | `path: String` (folder, or an app's root) | `DraftResult` (camelCase) |
| `apply_manifest` | `path: String, toml: String` | `AppRecord` — validates, writes `<path>/homemade.toml`, then `import_app` semantics (upsert + watcher + `app://changed`) |
| `explain_run` | `run_id: String` | `{ severity, message }` (SemanticHealthReport) |
| `model_complete` | `system: String, prompt: String` | `String` (raw helper for tests / power users; 512 tokens) |
- MCP (`integrations/mcp_server.rs`): new tool `draft_manifest` `{path}` → the `DraftResult` as JSON (runs the helper from the CLI process too; `--mcp` must construct `LocalModel`). Document in SKILL.md: "ask Homemade to draft a manifest: MCP tool `draft_manifest`".
- Events: `run://health {runId, severity, message}`, `pty://session_summary {sessionId, title}`, `model://status {available, reason}` (emit once after startup check).

### Frontend (Agent B)
- `src/lib/types.ts`: `ModelStatus`, `DraftResult`, `HealthReport`. `src/lib/ipc.ts`: `modelStatus`, `draftManifest(path)`, `applyManifest(path, toml)`, `explainRun(runId)`, `modelComplete(system, prompt)`; events `runHealth: "run://health"`, `ptySessionSummary: "pty://session_summary"`, `modelStatus: "model://status"`.
- Store: `modelStatus: ModelStatus | null`, `health: Record<runId, HealthReport>`; `useTauriEvents` fills both; `pty://session_summary` refreshes the agent session list.
- **Settings → "Local model" section** (`data-testid="settings-model"`): provider, availability, reason (with the fix hint "Enable Apple Intelligence in System Settings" when `appleIntelligenceNotEnabled`), helper path, a "Test" button that calls `modelComplete` with a trivial prompt and shows latency (`data-testid="model-test"`, result in `data-testid="model-test-result"`).
- **Draft manifest modal** `DraftManifestModal.tsx` (`data-testid="draft-modal"`): opened from (a) AppPanel header button `✨ Draft manifest` (`data-testid="draft-manifest"`), (b) Sidebar import dropdown item "Import folder & draft manifest…" (`data-testid="import-draft"`, picks a folder, runs `draftManifest`, opens the modal with `path`). Modal: spinner while drafting ("Reading README and docs…"); then two tabs: **Manifest** (editable `<textarea data-testid="draft-toml">`, validation errors list `data-testid="draft-errors"`, notes, `used model` badge) and **Agent prompt** (`<pre data-testid="draft-prompt">` + `Copy` button `data-testid="draft-copy-prompt"`). Footer: `Apply & reload` (`data-testid="draft-apply"`) → `applyManifest` → upsert app, open it, toast; `Cancel`. If `modelStatus.available === false`, still allow drafting (deterministic fallback) but show a one-line notice.
- **LogConsole**: when `health[runId]` exists show a banner (`data-testid="run-health"`, `data-severity`) above the log; add an `Explain` button (`data-testid="explain-run"`) that calls `explainRun` and stores the result; disabled while the run is active.
- **AgentTerminal**: session dropdown already shows titles; make sure `pty://session_summary` triggers a refresh of `listAgentSessions`.
- `testApi.ts`: nothing new required, but keep `window.__hm.ipc` exposing the new ipc functions (it spreads `ipc`).

### Config
`~/.homemade/config.toml`:
```toml
[model]
provider = "apple"   # "apple" (default on macOS 26+) | "off"
```
