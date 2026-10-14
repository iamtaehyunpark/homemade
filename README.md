# Homemade

**GUI Everything for your self-made software.** A lightweight launcher and runtime workspace for the scripts and tools you build with coding agents. See `concept.md` (why) and `spec.md` (how); `CONTRACT.md` is the module/interface map.

## Run (dev)

```sh
export PATH="$HOME/.cargo/bin:$PATH"   # rustup toolchain
pnpm install
pnpm tauri dev
```

Then drop a folder (or a `.py` file) onto the sidebar, or use **Import app…**. Try `examples/hello-cli`, `examples/hello-ui`, `examples/hello-worker`, `examples/no-manifest`, `examples/webapp-app`.

## Headless smoke test

```sh
cd src-tauri && cargo build
HOMEMADE_SMOKE=hello-cli HOMEMADE_SMOKE_INPUTS='{"name":"You","count":2}' ./target/debug/homemade
```
Runs one registered app end to end and prints the status, log tail and artifacts.

## Layout

- `src-tauri/` Rust supervisor: process groups + 3-stage reaper, `@homemade:` directive parser, VRAM/unified-memory admission queue, shared `uv`/HF/Torch store, keychain secrets, agent-session discovery (Claude Code, Codex, Antigravity), git checkpoints, `homemade-asset://` and `homemade-app://` protocols.
- `src/` React workbench: dockview tabs, AutoForm, sandboxed custom UI iframe with `window.homemade`, xterm agent terminal, output shelf, resource bar.
- `src-tauri/resources/` files shipped with the app: `ast_inspector.py`, `homemade_worker.py`, `homemade-bridge.js`, `SKILL.md` (auto-installed to `~/.claude/skills/homemade`).
- State lives in `~/.homemade/`.

`HOMEMADE_HOME=<dir>` overrides `~/.homemade` (separate profiles, tests).

## Dedicated windows

Any app can run as if it were a program of its own: open it and press the **⧉ Window** button in its header, drag its tab out of the dock area, or use the **Launch** page in the sidebar, a grid of program tiles that opens any program in its own window with one click. Put an `icon.png` in the project root to use it as the tile. That window contains only the program (its web UI, custom UI, or input form with a Run button), with no sidebar, header, or Underbar. Web apps start their server as soon as the window opens. A program that is open in its own window disappears from the Launch grid until that window closes. Closing the window exits the program: its run is stopped cleanly first. Press **⌘⇧D** in the window to bring the app back into the main window as a tab instead. Logs, runs and git stay available in the main window. To make an app always open this way, set `default_layout = "window"` under `[ui]` in its `homemade.toml`.

## CLI and MCP: driving Homemade from a shell or an agent

The same binary is a CLI and an MCP server, and both control the running app through a local socket, so an agent or a script can do everything the window can. Add `~/.homemade/bin` to your PATH (the app writes a `homemade` shim there on launch), then:

```sh
homemade status                         # is the GUI up, what is running
homemade import ~/code/thing            # register a folder
homemade run thing input=a.mp4 --wait   # run and block; exit 0 on success
homemade log <run-id> --follow
homemade open thing --window            # open as a standalone program window
homemade help                           # the rest; --json for scripts
```

For agents, register the MCP server once with `claude mcp add homemade -- /Applications/Homemade.app/Contents/MacOS/homemade --mcp`. It exposes the same operations as tools (import, run, wait, stop, logs, artifacts, presets, git checkpoint, open windows, ports, resources). When the GUI is not running, the read-only tools still answer from the database and `launch_gui` starts it.

## Local model (macOS 26+)

Homemade uses Apple's on-device Foundation Model (the one behind Apple Intelligence) through a 60 KB helper. Nothing is downloaded and nothing runs while idle. It powers:

- **Draft manifest** (`✨ Draft manifest` in an app header, or *Import folder & draft manifest…*): reads the README, docs, `pyproject.toml` / `package.json`, and the entry scripts' CLI definitions, and proposes a `homemade.toml` plus a self-contained *agent prompt* you can paste into Claude Code, Codex or `agy` to finish the adaptation. How the program runs (exec, mode, env manager) is decided deterministically from the repo; the model fills in names, descriptions, labels and input types the README documents. Also available to agents as the MCP tool `draft_manifest`.
- **Explain a failed run**: a one-line diagnosis above the log, automatically after a failure or via *Explain*.
- **Input inference for scripts without argparse** at import time, and **one-line titles for agent sessions** after a PTY session exits.

Settings → Local model shows availability; if Apple Intelligence is off, enable it in System Settings. On Linux the feature is reported unavailable and everything else works unchanged.
