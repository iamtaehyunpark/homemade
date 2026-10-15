# Live end-to-end suite

`run.js` is evaluated inside the running app's webview (`HOMEMADE_E2E=<path> pnpm tauri dev`) and drives the real backend through `window.__hm` (see CONTRACT.md). `runner.sh` shows the fixture layout expected under `F` (copies of `examples/*` plus `gpu-app`, `env-app`, `venv-app`; see the session notes in git history for their contents). Results print as `[e2e] PASS|FAIL ...` on stderr.

`orphan.sh` must run before the app launches (see `runner.sh`): it plants a detached HTTP server and a `runs` row marked running so the "orphan reaped at startup" test has something to find.

Local-model tests need two more fixtures under `F`: `readme-app/` (a README documenting `thumbs.py --src --size --out --grayscale`, script without argparse, no manifest) and `fail-app/` (manifest + `main.py` that prints a FileNotFoundError traceback and exits 1). They require macOS 26+ with Apple Intelligence enabled.

The runner sets `HOMEMADE_HOME=<fixtures>/home` so the suite never touches `~/.homemade`; `orphan.sh` bootstraps that home (schema cloned from the real DB, config with the `E2E_ALIAS` alias, a fake Claude session under `~/.claude/projects`, and the planted orphan).

## UI workflow checks

`ui-workflows.js` exercises Add → Create project → a real shell in the new folder, collision protection, the collapsible sidebar, element selection, and Launch search and standalone windows. Run it through the dev server with a fresh disposable profile (the debug binary needs Vite):

```sh
profile=$(mktemp -d /tmp/homemade-ui-check.XXXXXX)
HOMEMADE_HOME="$profile" HOMEMADE_E2E="$PWD/tests/e2e/ui-workflows.js" pnpm tauri dev
```

The test creates projects only within that profile and exits on completion.
