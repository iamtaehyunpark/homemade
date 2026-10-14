# Live end-to-end suite

`run.js` is evaluated inside the running app's webview (`HOMEMADE_E2E=<path> pnpm tauri dev`) and drives the real backend through `window.__hm` (see CONTRACT.md). `runner.sh` shows the fixture layout expected under `F` (copies of `examples/*` plus `gpu-app`, `env-app`, `venv-app`; see the session notes in git history for their contents). Results print as `[e2e] PASS|FAIL ...` on stderr.
