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
