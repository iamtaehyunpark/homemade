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
- Exit guard: `RunEvent::ExitRequested` with `code == None` (Cmd+Q, Quit menu) is prevented while anything is active and emits `app://confirm-exit`, same as window close. `confirm_quit` → `stop_all` → `app.exit(0)` passes (code is Some). `RunEvent::Exit` SIGKILLs any remaining groups as a last resort.
- `tests/e2e/orphan.sh` plants a detached server + a running row before launch; the suite asserts it was reaped.
