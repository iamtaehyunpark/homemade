<!-- homemade:start -->
## Homemade

This folder is registered in **Homemade** (the personal-software launcher) as app `hello-cli` — mode `cli`, exec `python3 main.py`.
The manifest is `homemade.toml` (keep it in sync with any CLI argument you add or change; declared inputs: name (string), count (int), shout (bool), input_file (file)).
Follow the `homemade` skill (`/private/tmp/claude-501/-Users-t-github-guix/acfc7ab3-3b6e-46ea-9564-96c0b1947ef3/scratchpad/ime/home/skills/homemade/SKILL.md`) for conventions: uv, `$HOMEMADE_OUTPUT_DIR`, `@homemade:progress` / `@homemade:artifact`.

You can drive the launcher yourself instead of asking the user to click Run:
- CLI: `/private/tmp/claude-501/-Users-t-github-guix/acfc7ab3-3b6e-46ea-9564-96c0b1947ef3/scratchpad/ime/home/bin/homemade` (`homemade run hello-cli key=value --wait`, `homemade activity hello-cli`, `homemade log <run-id> --follow`,
  `homemade top|ps|signal|stdin|capture|record`, `homemade help`).
- MCP: `/Users/t/github/homemade/src-tauri/target/debug/homemade --mcp` (config file `/private/tmp/claude-501/-Users-t-github-guix/acfc7ab3-3b6e-46ea-9564-96c0b1947ef3/scratchpad/ime/home/mcp.json`; tools: start_run, wait_run, app_activity, get_run_log, last_failed_run, explain_run,
  list_artifacts, apply_manifest, git_checkpoint, open_app, capture, …). Register once: `claude mcp add homemade -- /Users/t/github/homemade/src-tauri/target/debug/homemade --mcp`.
- Inside Homemade sessions these are already on PATH / connected, and the user can forward `[homemade]` status lines (run started / finished / diagnosis) into the session.
<!-- homemade:end -->
