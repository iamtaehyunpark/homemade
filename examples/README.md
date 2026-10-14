# Homemade example apps

Import any of these folders with **Import App** (or drag the folder onto the Homemade window). All examples use `env_manager = "system"`, so they only need `python3` on your PATH and install nothing.

| Folder | Mode | What it exercises |
|---|---|---|
| `hello-cli/` | `cli` | Fully declared `homemade.toml`, AutoForm UI, CLI flag building, progress, artifacts |
| `hello-ui/` | `cli` | Same script with a custom `ui/index.html` using `window.homemade` |
| `hello-worker/` | `rpc_worker` | Warm Python worker via `homemade_worker` (`@expose`, `progress`, `artifact`, `metric`) |
| `no-manifest/` | (inferred) | Bare argparse script with no `homemade.toml`; Homemade infers inputs |

## hello-cli
Inputs: `name` (string, `--name`), `count` (int 1-20, `--count`), `shout` (bool, `--shout`), and an optional positional `input_file`.
Expected: AutoForm shows these four fields. Running prints one greeting per count, advances the progress bar (`@homemade:progress`), and writes `outputs/greeting.txt`, which appears in the Output Shelf (`@homemade:artifact`). The built command looks like `python3 main.py --name World --count 3`.

## hello-ui
Same script and inputs, but `[ui] entry = "ui/index.html"`, so Homemade loads the custom page through `homemade-app://hello-ui.localhost/` with the bridge injected. Click **Run**: the log streams stdout, the progress bar fills, and when the artifact is announced the page loads `greeting.txt` through `homemade.assetUrl()` and shows it. **Browse** uses the native file picker; dropping a file on the page fills the input too.

## hello-worker
`mode = "rpc_worker"`: Homemade starts `worker.py` once and keeps it warm. The worker imports `homemade_worker` from `$HOMEMADE_RESOURCES` (set by Homemade and placed on `PYTHONPATH`); when run by hand it falls back to `../../src-tauri/resources`.
- `add(a, b)` returns the sum instantly.
- `slow(n)` sleeps `n` x 0.5 s, emitting progress per step, then writes `outputs/slow.txt`, emits it as an artifact plus a `seconds` metric, and returns `{steps, file}`. The UI shows the file's contents.

Manual check: `printf '{"id":"1","method":"add","params":{"a":2,"b":3}}\n' | python3 hello-worker/worker.py` prints `@homemade:worker_ready` followed by an `@homemade:rpc_response` containing `5`.

## no-manifest
`greet.py` has no `homemade.toml`. On import Homemade should infer: positional `folder` (directory), `--pattern` (string, default `*.png`), `--limit` (int), `--scale` (float), `--mode` (select: fast/accurate), and `--verbose` (bool). You can save the inferred manifest from the app's settings. Running it lists matching files.
