# Homemade

**GUI Everything for your self-made software.** A lightweight launcher and runtime workspace for the scripts and tools you build with coding agents. See `concept.md` (why) and `spec.md` (how); `CONTRACT.md` is the module/interface map.

## Run (dev)

```sh
export PATH="$HOME/.cargo/bin:$PATH"   # rustup toolchain
pnpm install
pnpm tauri dev
```

Then drop a folder (or a `.py` file) onto the sidebar, or use **Add → Add folder…**. Try `examples/hello-cli`, `examples/hello-ui`, `examples/hello-worker`, `examples/no-manifest`, `examples/webapp-app`.

Use **Add → Create project…** to start a new project. It lands in `~/homemade/<name>` by default (change the location in the dialog, or set `dir` under `[projects]` in `~/.homemade/config.toml`); Homemade creates the folder and opens a shell inside it. Set `app.exec` in the Manifest tab when the project is ready to run. Blank projects stay out of Launch until a run command is configured.

The sidebar can collapse to an icon rail using its top-right button. **Select element** lives in the workspace toolbar (⌘⇧C).

## Headless smoke test

```sh
cd src-tauri && cargo build
HOMEMADE_SMOKE=hello-cli HOMEMADE_SMOKE_INPUTS='{"name":"You","count":2}' ./target/debug/homemade
```
Runs one registered app end to end and prints the status, log tail and artifacts.

## Layout

- `src-tauri/` Rust supervisor: process groups + 3-stage reaper, `@homemade:` directive parser, VRAM/unified-memory admission queue, shared `uv`/HF/Torch store, keychain secrets, agent-session discovery (Claude Code, Codex, Antigravity), git checkpoints, `homemade-asset://` and `homemade-app://` protocols.
- `src/` React workbench: one dockview workspace per opened program (program panel plus any number of shells; the sidebar switches workspaces, **⌘T** or the **+** in a tab bar adds a shell), AutoForm, sandboxed custom UI iframe with `window.homemade`, xterm agent terminal, output shelf, resource bar.
- `src-tauri/resources/` files shipped with the app: `ast_inspector.py`, `homemade_worker.py`, `homemade-bridge.js`, `SKILL.md` (auto-installed to `~/.claude/skills/homemade`).
- State lives in `~/.homemade/`.

`HOMEMADE_HOME=<dir>` overrides `~/.homemade` (separate profiles, tests).

## Dedicated windows

Any app can run as if it were a program of its own: open it and press the **⧉ Window** button in its header, drag its tab out of the dock area, or use the **Launch** page in the sidebar, a grid of program tiles that opens any program in its own window with one click. Put an `icon.png` in the project root to use it as the tile. That window contains only the program (its web UI, custom UI, or input form with a Run button), with no sidebar, header, or Underbar. Web apps start their server as soon as the window opens. A program that is open in its own window disappears from the Launch grid until that window closes. Closing the window exits the program: its run is stopped cleanly first. Press **⌘⇧D** in the window to bring the app back into the main window as a tab instead. Logs, runs and git stay available in the main window. To make an app always open this way, set `default_layout = "window"` under `[ui]` in its `homemade.toml`.

## Select mode: point at anything, tell the agent

Press **Select element** (workspace toolbar, or ⌘⇧C), then click any element in the launcher: a run row, a log line, an output card, a form field, a tile, or any element **inside the program's own web UI** (a picker is injected into every program frame, like a browser devtools cursor: it reports tag, id/classes/attributes, text, a CSS selector, and the React/Vue component and source file when the app runs in dev mode). Homemade types a reference like `[ui: KoGrammar › Underbar › Logs tab › log-line "Traceback …" (app kogrammar)]` (or `[ui: KoGrammar › /settings › button#save.btn "Save" ‹SettingsForm› @ src/Settings.tsx:41 — selector: form > button:nth-of-type(2) (app kogrammar)]`) into the app's open agent session, without pressing Enter, so you can continue with what you want done about it. With no agent session open, the reference goes to the clipboard. Esc leaves select mode.

## CLI and MCP: driving Homemade from a shell or an agent

The same binary is a CLI and an MCP server, and both control the running app through a local socket, so an agent or a script can do everything the window can. Add `~/.homemade/bin` to your PATH (the app writes a `homemade` shim there on launch), then:

```sh
homemade status                         # is the GUI up, what is running
homemade import ~/code/thing            # register a folder
homemade new thing                      # create ~/homemade/thing with a blank manifest and register it (--in DIR elsewhere)
homemade run thing input=a.mp4 --wait   # run and block; exit 0 on success
homemade log <run-id> --follow
homemade open thing --window            # open as a standalone program window
homemade activity thing                 # what it is doing now: run, live log tail, last result, outputs
homemade top thing --watch              # live CPU / RAM / GPU of the program (history with --seconds)
homemade ps thing                       # its process tree with pid, pgid, port
homemade signal <run-id> USR1           # any POSIX signal; stdin <run-id> "text" writes to its stdin
homemade capture thing                  # PNG of the program's window; record thing --seconds 30 → JSONL of usage + log
homemade help                           # the rest; --json for scripts
```

Registering a folder also writes a managed block (between `<!-- homemade:start -->` and `<!-- homemade:end -->`) into its `CLAUDE.md` and `AGENTS.md`, so an agent you start in that folder by hand learns the app id, manifest, CLI and MCP server. The block is refreshed when the manifest changes and removed when you unregister the app; your own text in those files is left alone. Set `write_docs = false` under `[agents]` in `~/.homemade/config.toml` to opt out. Anything Homemade starts already knows this: program runs, agent terminals and shells get `homemade` on PATH and `HOMEMADE_MCP_COMMAND` in their environment, and Claude Code or Codex sessions opened with Edit with Agent have the `homemade` MCP server connected automatically. Agents started from Homemade also stay aware of the program they work on: when a run of that app starts, finishes, or gets a failure diagnosis, Homemade shows a `[homemade] …` status line above the agent session with a **Send** button, so you decide whether it goes to the agent (the select-mode mention is the only thing typed directly), and the `app_activity` tool or `homemade activity <id>` returns the current run, live log tail, last result and outputs on demand. Set `notify_runs = false` under `[agents]` in `~/.homemade/config.toml` to turn the messages off. For agents you start yourself, register the MCP server once with `claude mcp add homemade -- /Applications/Homemade.app/Contents/MacOS/homemade --mcp`. It exposes the same operations as tools (import, run, wait, stop, logs, artifacts, presets, git checkpoint, open windows, ports, resources). When the GUI is not running, the read-only tools still answer from the database and `launch_gui` starts it.

## Dependencies and deleting programs

`env_manager` in `homemade.toml` is enforced, not advisory. Before a run, Homemade compares the app's lockfile with the environment it built last time and runs `uv sync` (Python) or `pnpm install` (Node) when they differ, so a program never runs against stale dependencies. Packages live in a shared store (`uv`'s cache, `pnpm`'s content-addressable store): a package two programs both need is downloaded once and linked into each environment, not copied. Shells and agent terminals opened from Homemade have the app's environment on `PATH`, so `python`, `node` and the installed tools resolve to the app's own versions.

Removing a program has two levels, both reached from **Remove…** in the sidebar row menu, the Launch page's edit mode or an app header's `⋯` menu:

- **Hide from Homemade** unregisters the app. The folder, its environment and its outputs stay on disk untouched; only Homemade's records go (run history, presets, artifact records, and the managed block in `CLAUDE.md` / `AGENTS.md`).
- **Delete program…** first shows a plan of what the program occupies and what is only used by it, with a size next to each line: the folder, the environment (`.venv` / `node_modules`), scratch files, agent sessions, packages in the shared store that no other registered program references, and any downloaded models it owns. You tick what goes. Defaults: the folder, scratch, sessions and unique packages are ticked; models are not, and an environment that already existed before Homemade registered the folder is not either. Anything inside the folder is covered when the folder is ticked, and run history, presets and artifact records are always removed. The folder is moved to the Trash, not erased, so a mistaken delete can be undone from there. If a run or agent session of that program is still active, the plan says so and nothing is deleted until it stops.

The same plan is available from the CLI: `homemade delete <app> --plan` prints it (keys `folder`, `env`, `scratch`, `sessions`, `packages`, `model:<path>`) without touching anything; `homemade delete <app>` applies the defaults.

## Local model (macOS 26+)

Homemade uses Apple's on-device Foundation Model (the one behind Apple Intelligence) through a 60 KB helper. Nothing is downloaded and nothing runs while idle. It powers:

- **Draft manifest** (`✨ Draft manifest` in an app header, or *Auto-import folder…*): reads the README, docs, `pyproject.toml` / `package.json`, and the entry scripts' CLI definitions, and proposes a `homemade.toml` plus a self-contained *agent prompt* you can paste into Claude Code, Codex or `agy` to finish the adaptation. How the program runs (exec, mode, env manager) is decided deterministically from the repo; the model fills in names, descriptions, labels and input types the README documents. Also available to agents as the MCP tool `draft_manifest`.
- **Explain a failed run**: a one-line diagnosis above the log, automatically after a failure or via *Explain*.
- **Input inference for scripts without argparse** at import time, and **one-line titles for agent sessions** after a PTY session exits.

Settings → Local model shows availability; if Apple Intelligence is off, enable it in System Settings. On Linux the feature is reported unavailable and everything else works unchanged.
