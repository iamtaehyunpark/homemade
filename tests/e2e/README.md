# Live end-to-end suite

`run.js` is evaluated inside the running app's webview (`HOMEMADE_E2E=<path> pnpm tauri dev`) and drives the real backend through `window.__hm` (see CONTRACT.md). `runner.sh` shows the fixture layout expected under `F` (copies of `examples/*` plus `gpu-app`, `env-app`, `venv-app`; see the session notes in git history for their contents). Results print as `[e2e] PASS|FAIL ...` on stderr.

`orphan.sh` must run before the app launches (see `runner.sh`): it plants a detached HTTP server and a `runs` row marked running so the "orphan reaped at startup" test has something to find.
