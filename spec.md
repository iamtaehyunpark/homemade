# Homemade (`homemade`) — Complete v1 Engineering & Implementation Specification

> Formerly `guix`; renamed 2026-10-13 to avoid collision with GNU Guix.

> **v1 platform scope:** macOS (Apple Silicon, primary dev target) and Linux (x86_64, NVIDIA). Windows is out of scope for v1; the supervisor relies on POSIX process groups and signals.

---

## 1. Repository & Module Topology

The system is built as a **Tauri v2** workspace combining a lightweight Rust systems supervisor with a React + TypeScript dockable workbench UI, plus an embedded Python helper script for AST reflection and warm RPC workers.

```text
homemade/
├── Cargo.toml                      # Workspace root
├── package.json                    # Frontend dependencies (React, xterm.js, dockview, zustand)
├── src-tauri/
│   ├── Cargo.toml                  # Rust dependencies
│   ├── tauri.conf.json             # Windowing, CSP, custom protocol declarations
│   ├── resources/
│   │   ├── ast_inspector.py        # Zero-dep Python AST & --help parser
│   │   ├── homemade_worker.py          # Optional Warm JSON-RPC Python wrapper
│   │   ├── homemade-bridge.js          # Injected into app iframes as window.homemade
│   │   └── SKILL.md                # Default Agent Skill template
│   └── src/
│       ├── main.rs                 # Entrypoint, close-guard hook, custom URI protocols
│       ├── state.rs                # Global AppState (Arc<Mutex<...>> / DashMap)
│       ├── db/
│       │   ├── mod.rs              # SQLite connection pool & migrations
│       │   └── queries.rs          # CRUD for apps, runs, presets, storage refs, sessions
│       ├── manifest/
│       │   ├── schema.rs           # Serde structs for homemade.toml
│       │   └── infer.rs            # Invokes ast_inspector.py + merges with homemade.toml
│       ├── supervisor/
│       │   ├── spawner.rs          # POSIX setsid / PGID process spawner + login PATH loader
│       │   ├── reaper.rs           # 3-stage SIGINT -> SIGTERM -> SIGKILL cascade
│       │   ├── pty_manager.rs      # portable-pty sessions for CLI agents & interactive apps
│       │   ├── stdout_parser.rs    # Ring buffer + @homemade: directive state machine
│       │   └── scheduler.rs        # Active VRAM admission queue & telemetry poller
│       ├── storage/
│       │   ├── env_router.rs       # uv CoW store, HF_HOME/TORCH_HOME injection
│       │   ├── venv_scanner.rs     # Detects unmanaged .venv & calculates reclaimable bytes
│       │   └── artifact_differ.rs  # Pre/post run mtime snapshot & scratch cleaner
│       ├── integrations/
│       │   ├── agents.rs           # Session discovery for Claude Code, agy, Codex
│       │   ├── git_ops.rs          # Auto-init, smart .gitignore, checkpoint & revert
│       │   └── mcp_server.rs       # Local SSE/Stdio MCP server for coding agents
│       └── intelligence/
│           └── traits.rs           # Phase 2 plugin hooks (LogAnalyzer, SchemaInferrer)
└── src/                            # Frontend UI (React 19 + TypeScript + Tailwind)
    ├── state/store.ts              # Zustand store synced with Tauri events
    ├── components/
    │   ├── Workbench.tsx           # Dockview split-pane / tab container
    │   ├── AppSandbox.tsx          # Sandboxed iframe host + postMessage bridge
    │   ├── AutoForm.tsx            # Dynamic schema-to-HTML form renderer
    │   ├── AgentTerminal.tsx       # xterm.js PTY pane + session dropdown + Git bar
    │   ├── OutputShelf.tsx         # Artifact previewer, native OS drag-out, scratch purge
    │   └── ResourceBar.tsx         # Global & per-app CPU/RAM/VRAM monitor + Queue view

```

### Required Rust Crates (`src-tauri/Cargo.toml`)

```toml
[dependencies]
tauri = { version = "2", features = ["protocol-asset", "macos-private-api"] }   # use latest 2.x at scaffold time
tokio = { version = "1.40", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
toml = "0.8"
rusqlite = { version = "0.32", features = ["bundled"] }
nix = { version = "0.29", features = ["process", "signal", "term"] }
portable-pty = "0.8"
sysinfo = "0.32"
notify = "7.0"                 # File watcher for hot-reload & folder triggers
dashmap = "6.1"                # Lock-free concurrent maps for active processes
ringbuf = "0.4"                # Fixed-size circular log buffers
uuid = { version = "1.10", features = ["v4"] }
walkdir = "2.5"
indexmap = { version = "2", features = ["serde"] }   # ordered [inputs] map (NOT in std)
libc = "0.2"                                         # prctl(PR_SET_PDEATHSIG) on Linux
percent-encoding = "2"                               # homemade-asset:// path decoding
async-trait = "0.1"                                  # SemanticAssistor trait
keyring = "3"                                        # Secrets vault: macOS Keychain / Linux Secret Service

[target.'cfg(target_os = "linux")'.dependencies]
nvml-wrapper = "0.10"          # NVIDIA VRAM telemetry (Linux only; no NVML on macOS)

```

---

## 2. Filesystem Layout & SQLite Persistence Schema

All global launcher state lives in `~/.homemade/`. Individual apps stay self-contained in their own project folders.

```text
~/.homemade/
├── state.db                        # SQLite state database
├── config.toml                     # Global settings, directory aliases, secrets vault metadata
├── store/
│   ├── uv_cache/                   # Central Content-Addressable uv package cache
│   ├── pnpm_store/                 # Central Content-Addressable Node store
│   └── shared_cache/               # Shared heavy weights
│       ├── huggingface/            # Injected as HF_HOME
│       ├── torch/                  # Injected as TORCH_HOME
│       ├── ollama/                 # Injected as OLLAMA_MODELS
│       └── playwright/             # Injected as PLAYWRIGHT_BROWSERS_PATH
└── skills/
    └── homemade/SKILL.md               # Symlinked into ~/.claude/skills/homemade/SKILL.md

```

### Complete SQLite DDL (`src-tauri/src/db/mod.rs`)

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

-- 1. Registered Self-Made Programs
CREATE TABLE apps (
    id TEXT PRIMARY KEY,                    -- Slug e.g., "invoice-ocr"
    name TEXT NOT NULL,
    root_dir TEXT NOT NULL UNIQUE,          -- Absolute path to project folder
    manifest_hash TEXT NOT NULL,            -- SHA256 of homemade.toml
    execution_mode TEXT NOT NULL DEFAULT 'cli', -- 'cli' | 'rpc_worker' | 'webapp'
    unmanaged_venv_bytes INTEGER DEFAULT 0, -- >0 triggers "Migrate to Managed Store" banner
    learned_peak_ram_mb INTEGER DEFAULT 0,
    learned_peak_vram_mb INTEGER DEFAULT 0,
    created_at INTEGER NOT NULL,
    last_run_at INTEGER
);

-- 2. Saved Form Presets per Program
CREATE TABLE presets (
    id TEXT PRIMARY KEY,
    app_id TEXT NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
    name TEXT NOT NULL,                     -- e.g., "Fast 720p Batch"
    inputs_json TEXT NOT NULL,              -- JSON key-value map of input fields
    created_at INTEGER NOT NULL
);

-- 3. Execution History & Replays
CREATE TABLE runs (
    id TEXT PRIMARY KEY,                    -- UUID v4
    app_id TEXT NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
    status TEXT NOT NULL,                   -- 'queued_vram' | 'running' | 'stopping' | 'success' | 'failed' | 'killed'
    inputs_json TEXT NOT NULL,              -- Exact arguments used
    cli_command TEXT NOT NULL,              -- Exact escaped shell string for 1-click copy
    exit_code INTEGER,
    peak_ram_mb INTEGER DEFAULT 0,
    peak_vram_mb INTEGER DEFAULT 0,
    started_at INTEGER NOT NULL,
    finished_at INTEGER,
    log_tail TEXT                           -- Last 200 lines of stdout/stderr on exit
);

-- 4. Output Artifacts & Scratch Tracking
CREATE TABLE artifacts (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    app_id TEXT NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
    file_path TEXT NOT NULL,
    file_size_bytes INTEGER NOT NULL,
    mime_type TEXT NOT NULL,
    kind TEXT NOT NULL,                     -- 'output' (Output Shelf) | 'scratch' (Purgeable)
    created_at INTEGER NOT NULL
);

-- 5. Shared Storage & Model Weight Reference Graph
-- v1 NOTE: populated only for model-weight directories (HF/TORCH/OLLAMA) observed under
-- shared_cache during a run. Per-app refcounting of uv/pnpm packages is deferred; "Clean
-- Delete" runs `uv cache prune` instead of tracking package refs.
CREATE TABLE storage_refs (
    asset_hash TEXT NOT NULL,               -- Package or model directory identifier
    asset_path TEXT NOT NULL,               -- Path inside ~/.homemade/store/
    app_id TEXT NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
    size_bytes INTEGER NOT NULL,
    PRIMARY KEY (asset_hash, app_id)
);

-- 6. Linked CLI Coding Agent Sessions
CREATE TABLE agent_sessions (
    session_id TEXT PRIMARY KEY,            -- Native session ID from Claude Code / agy / Codex
    app_id TEXT NOT NULL REFERENCES apps(id) ON DELETE CASCADE,
    agent_type TEXT NOT NULL,               -- 'claude_code' | 'codex' | 'antigravity' | 'custom_shell'
    title TEXT,                             -- Extracted first user prompt or summary
    git_commit_before TEXT,                 -- SHA before session started
    git_commit_after TEXT,                  -- SHA when session closed
    updated_at INTEGER NOT NULL
);

```

---

## 3. The `homemade.toml` Manifest Specification

Every self-made program can either have zero config (auto-inferred) or a `homemade.toml` file in its root directory.

### Complete `homemade.toml` Reference

```toml
[app]
id = "video-transcriber"
name = "Whisper Video Transcriber"
version = "0.2.0"
description = "Extracts audio and runs local Whisper transcription with timestamps."
mode = "cli"                           # "cli" (default) | "rpc_worker" | "webapp"
exec = "uv run python transcribe.py"   # Command executed in root_dir
cwd = "."                              # Relative to project root
env_manager = "uv"                     # "uv" (default) | "pnpm" | "cargo" | "byo" | "system"

[ui]
# If entry is omitted, Homemade renders AutoForm from [inputs].
# If set, renders this HTML inside the sandboxed iframe.
entry = "ui/index.html"
default_layout = "tab"                 # "tab" | "split_right" | "window"

[resources]
requires_gpu = true
vram_estimate_mb = 6144                # Used by Active VRAM Admission Queue
max_ram_mb = 8192                      # Linux: enforced via setrlimit(RLIMIT_AS)/cgroups. macOS: advisory only (shown in UI, not enforced)
isolated_cache = false                 # false = route HF_HOME/TORCH_HOME to ~/.homemade/store/shared_cache
graceful_timeout_ms = 3000             # Time between SIGINT and SIGTERM
kill_timeout_ms = 2000                 # Time between SIGTERM and SIGKILL

[environment]
# Keys pulled from HOMEMADE OS Keychain Vault and injected at runtime
secrets = ["OPENAI_API_KEY", "HF_TOKEN"]
# Custom env overrides. `$NAME` / `${NAME}` expand from [aliases] in ~/.homemade/config.toml, then from the login env.
vars = { WHISPER_MODEL = "large-v3", OUTPUT_DIR = "./outputs" }

[artifacts]
output_dir = "./outputs"               # Always surfaced in the Output Shelf
scratch_globs = ["tmp_*.wav", "*.part", ".cache/**"] # Wiped on "Clean Scratch"

#Input Schema (Used by AutoForm AND exposed to custom UI via window.homemade.getSchema())
[inputs.video_file]
type = "file"                          # "file" | "directory" | "file_list" | "string" | "int" | "float" | "bool" | "select" | "range"
cli_flag = "--input"                   # Or positional_index = 0
label = "Source Video"
extensions = [".mp4", ".mkv", ".mov"]
required = true

[inputs.model_size]
type = "select"
cli_flag = "--model"
label = "Whisper Model"
options = ["tiny", "base", "small", "medium", "large-v3"]
default = "large-v3"

[inputs.temperature]
type = "range"
cli_flag = "--temp"
label = "Sampling Temperature"
min = 0.0
max = 1.0
step = 0.05
default = 0.0

# Free-text passthrough: type = "args" appends shell-split extra arguments verbatim (also the inference fallback when no CLI is detected)
[inputs.extra]
type = "args"
label = "Extra arguments"

[inputs.translate]
type = "bool"
cli_flag = "--translate"
bool_style = "flag_only"               # "flag_only" (--translate) | "key_value" (--translate true)
default = false

```

### Rust Serde Implementation (`src-tauri/src/manifest/schema.rs`)

```rust
use serde::{Deserialize, Serialize};
use indexmap::IndexMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HomemadeManifest {
    pub app: AppConfig,
    #[serde(default)]
    pub ui: UiConfig,
    #[serde(default)]
    pub resources: ResourceConfig,
    #[serde(default)]
    pub environment: EnvConfig,
    #[serde(default)]
    pub artifacts: ArtifactConfig,
    #[serde(default)]
    pub inputs: IndexMap<String, InputFieldSpec>, // Preserves declaration order in GUI
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub id: String,
    pub name: String,
    #[serde(default = "default_version")]
    pub version: String,
    pub description: Option<String>,
    #[serde(default = "default_mode")]
    pub mode: ExecutionMode,
    pub exec: String,
    #[serde(default = "default_cwd")]
    pub cwd: String,
    #[serde(default = "default_env_mgr")]
    pub env_manager: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Cli,
    RpcWorker,
    Webapp,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceConfig {
    #[serde(default)]
    pub requires_gpu: bool,
    pub vram_estimate_mb: Option<u64>,
    pub max_ram_mb: Option<u64>,
    #[serde(default)]
    pub isolated_cache: bool,
    #[serde(default = "default_graceful_ms")]
    pub graceful_timeout_ms: u64,
    #[serde(default = "default_kill_ms")]
    pub kill_timeout_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InputFieldSpec {
    #[serde(rename = "type")]
    pub field_type: String,
    pub cli_flag: Option<String>,
    pub positional_index: Option<usize>,
    pub label: Option<String>,
    pub required: Option<bool>,
    pub default: Option<serde_json::Value>,
    pub options: Option<Vec<String>>,
    pub extensions: Option<Vec<String>>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub step: Option<f64>,
    pub bool_style: Option<String>,
}

```

---

## 4. Zero-Config CLI Discovery Engine (`ast_inspector.py`)

When a user drops a folder or `.py` script into Homemade without a `homemade.toml`, the Rust backend runs `python3 -I ast_inspector.py /path/to/script.py` to statically extract CLI definitions without executing the script's top-level side effects. **v1 scope:** `argparse` only (below). `typer`/`click` decorator parsing and a consent-gated `--help` text parser (which must execute the script) are v1.1 follow-ups; until then, unknown scripts fall back to a single free-text "Arguments" field.

### Embedded Python Inspector (`src-tauri/resources/ast_inspector.py`)

```python
import ast
import json
import sys
from pathlib import Path

def infer_argparse(tree: ast.AST) -> dict:
    inputs = {}
    positional_count = 0
    for node in ast.walk(tree):
        if isinstance(node, ast.Call) and isinstance(node.func, ast.Attribute):
            if node.func.attr == "add_argument":
                flags = [
                    arg.value for arg in node.args
                    if isinstance(arg, ast.Constant) and isinstance(arg.value, str)
                ]
                if not flags:
                    continue
                primary_flag = max(flags, key=len)
                key = primary_flag.lstrip("-").replace("-", "_")
                is_positional = not primary_flag.startswith("-")
                spec = {
                    "type": "string",
                    "cli_flag": None if is_positional else primary_flag,
                    "positional_index": positional_count if is_positional else None,
                    "label": key.replace("_", " ").title(),
                }
                if is_positional:
                    positional_count += 1
                    spec["required"] = True
                for kw in node.keywords:
                    if kw.arg == "type" and isinstance(kw.value, ast.Name):
                        t_map = {"int": "int", "float": "float", "Path": "file", "bool": "bool"}
                        spec["type"] = t_map.get(kw.value.id, "string")
                    elif kw.arg == "action" and isinstance(kw.value, ast.Constant):
                        if kw.value.value in ("store_true", "store_false"):
                            spec["type"] = "bool"
                            spec["bool_style"] = "flag_only"
                            spec["default"] = (kw.value.value == "store_false")
                    elif kw.arg == "default" and isinstance(kw.value, ast.Constant):
                        spec["default"] = kw.value.value
                    elif kw.arg == "choices" and isinstance(kw.value, (ast.List, ast.Tuple)):
                        spec["type"] = "select"
                        spec["options"] = [
                            e.value for e in kw.value.elts if isinstance(e, ast.Constant)
                        ]
                    elif kw.arg == "required" and isinstance(kw.value, ast.Constant):
                        spec["required"] = bool(kw.value.value)
                # Semantic path heuristic
                if spec["type"] == "string" and any(w in key for w in ("file", "path", "input", "img", "video")):
                    spec["type"] = "file"
                inputs[key] = spec
    return inputs

if __name__ == "__main__":
    script_path = Path(sys.argv[1])
    source = script_path.read_text(encoding="utf-8")
    tree = ast.parse(source)
    print(json.dumps({"inputs": infer_argparse(tree)}))

```

---

## 5. Process Supervisor, Clean Teardown & Close Guard

### A. Login Shell `$PATH` Hydration (`src-tauri/src/supervisor/spawner.rs`)

Because macOS `launchd` and Linux desktop entries strip user shell environments, `homemade` hydrates the user's real login environment once at startup:

```rust
use std::collections::HashMap;
use std::process::Command;

pub fn load_user_login_env() -> HashMap<String, String> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    // Login shell (-l) only, NOT interactive (-i): interactive rc files may prompt,
    // print banners, or block. Wrap in a 5s timeout at the call site and fall back
    // to the inherited env if it fails. Verified: macOS `env -0` is supported.
    let output = Command::new(&shell)
        .args(["-l", "-c", "env -0"])
        .output();

    let mut env_map = std::env::vars().collect::<HashMap<_, _>>();
    if let Ok(out) = output {
        for entry in out.stdout.split(|&b| b == 0) {
            if let Ok(s) = std::str::from_utf8(entry) {
                if let Some((k, v)) = s.split_once('=') {
                    env_map.insert(k.to_string(), v.to_string());
                }
            }
        }
    }
    // Always enforce unbuffered stdio for child processes
    env_map.insert("PYTHONUNBUFFERED".into(), "1".into());
    env_map.insert("FORCE_COLOR".into(), "1".into());
    env_map
}

```

### B. POSIX Process Group Spawner & 3-Stage Clean Reaper (`reaper.rs`)

Every spawned app is placed in its own POSIX session/process group (`setsid()`), making its `PGID == PID`. When the user clicks **Stop**, or closes a tab, the reaper guarantees zero surviving zombie children:

```rust
use nix::sys::signal::{killpg, Signal};
use nix::unistd::Pid;
use std::os::unix::process::CommandExt;
use tokio::process::Command;
use tokio::time::{sleep, Duration};

pub async fn spawn_managed_process(
    cmd_parts: &[String],
    cwd: &std::path::Path,
    env: & std::collections::HashMap<String, String>,
) -> std::io::Result<tokio::process::Child> {
    let mut cmd = Command::new(&cmd_parts[0]);
    cmd.args(&cmd_parts[1..])
        .current_dir(cwd)
        .envs(env)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    unsafe {
        cmd.pre_exec(|| {
            // Create a dedicated Process Group so PGID == child PID
            nix::unistd::setsid().map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            #[cfg(target_os = "linux")]
            {
                // Ensure kernel kills child if HOMEMADE unexpectedly crashes
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            }
            Ok(())
        });
    }
    cmd.spawn()
}

pub async fn terminate_process_group(
    pgid: i32,
    graceful_ms: u64,
    kill_ms: u64,
) {
    let target = Pid::from_raw(pgid);

    // Stage 1: SIGINT to entire process group (triggers Python KeyboardInterrupt / finally blocks)
    let _ = killpg(target, Signal::SIGINT);
    sleep(Duration::from_millis(graceful_ms)).await;

    if killpg(target, None).is_err() {
        return; // Entire process group exited cleanly
    }

    // Stage 2: SIGTERM to entire process group
    let _ = killpg(target, Signal::SIGTERM);
    sleep(Duration::from_millis(kill_ms)).await;

    if killpg(target, None).is_err() {
        return;
    }

    // Stage 3: SIGKILL (-9) to entire process group
    let _ = killpg(target, Signal::SIGKILL);
}

```

### C. Close Guard on Application Exit (`src-tauri/src/main.rs`)

The app behaves like a standard desktop browser/shell: when closed, it shuts down completely, but intercepts exit if any job is active:

```rust
window.on_window_event(move |event| {
    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
        let active_count = state.active_processes.len() + state.active_ptys.len();
        if active_count > 0 {
            api.prevent_close();
            // Emit event to React UI to display native confirmation modal:
            // "[active_count] programs are currently running. Stop all and quit?"
            let _ = window_handle.emit("homemade://confirm-exit", active_count);
        }
    }
});

```

---

## 6. `@homemade:` Stdout Directive Parser & Warm RPC Protocol

### A. Line-Oriented `@homemade:` Stream Parser (`stdout_parser.rs`)

As lines arrive from the child's `stdout`, the supervisor pushes raw lines into a 5,000-line `RingBuffer` while intercepting lines starting with `@homemade:`:

| Stdout Line Emitted by Script | Action Taken by Launcher Supervisor |
| --- | --- |
| `@homemade:progress 42 "Extracting frames..."` | Emits `homemade://progress` `{ runId, percent: 42, message: "Extracting frames..." }` to UI. |
| `@homemade:artifact "/abs/path/chart.png"` | Immediately indexes `/abs/path/chart.png` into SQLite `artifacts` and refreshes Output Shelf. |
| `@homemade:metric {"fps": 58.4, "loss": 0.12}` | Updates live telemetry pill in the app's header bar. |
| `@homemade:port 8421` | When `mode = "webapp"`, notifies launcher reverse-proxy/iframe to mount `[http://127.0.0.1:8421](http://127.0.0.1:8421)`. |

### B. Optional Warm Python RPC Worker (`src-tauri/resources/homemade_worker.py`)

For tools that load heavy libraries (PyTorch, OpenCV, LLMs) and have custom HTML buttons requiring instant `<10ms` responses, agents import `homemade_worker`:

```python
# Usage inside a user's script:
# from homemade_worker import expose, run_worker
# @expose
# def apply_filter(image_path: str, sigma: float): ...
# if __name__ == "__main__": run_worker()

import sys, json, traceback

_REGISTRY = {}

def expose(fn):
    _REGISTRY[fn.__name__] = fn
    return fn

def run_worker():
    print("@homemade:worker_ready", flush=True)
    for line in sys.stdin:
        line = line.strip()
        if not line:
            continue
        try:
            req = json.loads(line)
            req_id = req.get("id")
            method = req.get("method")
            params = req.get("params", {})
            if method == "__shutdown__":
                sys.exit(0)
            res = _REGISTRY[method](**params)
            out = {"id": req_id, "ok": True, "result": res}
        except Exception as e:
            out = {"id": req_id, "ok": False, "error": str(e), "trace": traceback.format_exc()}
        print(f"@homemade:rpc_response {json.dumps(out)}", flush=True)

```

---

## 7. Frontend Sandbox, Custom Asset Protocol & `window.homemade` SDK

### A. Custom Protocol `homemade-asset://` (`src-tauri/src/main.rs`)

To allow sandboxed `<iframe>` UIs and the Output Shelf to render local images, audio, and 4K video files from anywhere on disk without CORS errors or loading blobs into RAM, register a streaming custom URI scheme with HTTP `Range` support:

```rust
tauri::Builder::default()
    .register_uri_scheme_protocol("homemade-asset", |_ctx, request| {
        let uri_path = percent_encoding::percent_decode_str(request.uri().path())
            .decode_utf8_lossy()
            .to_string();
        // Serve file from disk with proper Content-Type and Range support for video scrubbing
        serve_local_file_with_range(&uri_path, request.headers())
    })

```

### B. `window.homemade` Injected JavaScript Bridge (`src-tauri/resources/homemade-bridge.js`)

Every custom `ui/index.html` loaded in an app's `<iframe sandbox="allow-scripts allow-forms">` automatically has `homemade-bridge.js` injected. **Injection mechanism:** the iframe `src` is `homemade-app://<app_id>/index.html`, a second custom protocol that serves files from `<root_dir>/ui/`. For `text/html` responses the handler inserts `<script src="homemade-app://<app_id>/__homemade/bridge.js"></script>` immediately after `<head>` (or at the top of the document if no `<head>`). The sandbox omits `allow-same-origin`, so the iframe runs as an opaque origin and can only reach the host through `postMessage`. For `mode = "webapp"`, the iframe `src` is instead `http://127.0.0.1:<port>` and the bridge is not injected (CSP `frame-src` must allow `http://127.0.0.1:*`).

The bridge communicates with the parent Tauri window via structured `postMessage` channels:

```javascript
window.homemade = {
  // 1. Run CLI command with structured args object -> returns Promise<{ exitCode, runId, artifacts }>
  run: (inputsObj) => sendRpc("homemade:run", { inputs: inputsObj }),

  // 2. Call a warm @expose Python function (when mode = "rpc_worker")
  call: (method, params = {}) => sendRpc("homemade:rpc_call", { method, params }),

  // 3. Stop currently running execution
  stop: () => sendRpc("homemade:stop", {}),

  // 4. Native OS File/Folder Dialog (returns real absolute OS path strings, never Browser Blobs!)
  pickFile: (options = { multiple: false, extensions: [] }) => sendRpc("homemade:pick_file", options),
  pickFolder: () => sendRpc("homemade:pick_folder", {}),

  // 5. Convert an absolute OS file path into a playable/renderable URL for <img>, <video>, <audio>
  assetUrl: (absFilePath) => `homemade-asset://localhost/${encodeURIComponent(absFilePath)}`,

  // 6. Retrieve schema & saved presets
  getSchema: () => sendRpc("homemade:get_schema", {}),
  savePreset: (name, inputsObj) => sendRpc("homemade:save_preset", { name, inputs: inputsObj }),

  // 7. Real-time Event Listeners
  onStdout: (cb) => registerListener("stdout", cb),
  onStderr: (cb) => registerListener("stderr", cb),
  onProgress: (cb) => registerListener("progress", cb),
  onMetric: (cb) => registerListener("metric", cb),
  onArtifact: (cb) => registerListener("artifact", cb),
  onFileDrop: (cb) => registerListener("file_drop", cb), // Receives array of real OS paths dropped on iframe
};

```

---

## 8. Semantic Storage Router & Venv Deduplication Engine

### A. Global Environment Variable Injection (`storage/env_router.rs`)

Whenever `isolated_cache = false` (the default), the spawner injects these environment variables into every child process so PyTorch, HuggingFace, `uv`, and Playwright share a single physical disk store:

```rust
pub fn inject_shared_storage_env(env: &mut HashMap<String, String>, homemade_home: &Path, isolated: bool) {
    let store = homemade_home.join("store");
    // Always route uv and pnpm to the central Content-Addressable Store with Copy-on-Write linking
    env.insert("UV_CACHE_DIR".into(), store.join("uv_cache").to_string_lossy().into());
    env.insert("UV_LINK_MODE".into(), "clone".into()); // Uses APFS clonefile on macOS / reflink on Linux

    if !isolated {
        let shared = store.join("shared_cache");
        env.insert("HF_HOME".into(), shared.join("huggingface").to_string_lossy().into());
        env.insert("TORCH_HOME".into(), shared.join("torch").to_string_lossy().into());
        env.insert("OLLAMA_MODELS".into(), shared.join("ollama").to_string_lossy().into());
        env.insert("PLAYWRIGHT_BROWSERS_PATH".into(), shared.join("playwright").to_string_lossy().into());
    }
}

```

### A2. Secrets Vault (`storage/secrets.rs`)

- Backed by the `keyring` crate: macOS Keychain / Linux Secret Service, service name `homemade`, one entry per key name.
- `config.toml` holds only the list of key *names* (never values). Settings UI: add / rotate / delete.
- At spawn, `[environment] secrets = [...]` names are resolved from the vault and injected into the child env. Missing keys block the run with a clear "Missing secret: HF_TOKEN" error instead of a silent failure.
- Secret values are redacted from `cli_command`, `log_tail`, and the embedded PTY transcript log.

### B. Unmanaged `.venv` Detector & 1-Click Migration (`storage/venv_scanner.rs`)

When importing or scanning an app folder:

1. Check if `<app_root>/.venv` or `<app_root>/venv` exists.
2. Walk the directory and check `st_nlink` (hardlink count) or clone status, calculating total physical disk usage.
3. If `unmanaged_venv_bytes > 50MB`, display a non-intrusive badge in the UI header: **`Unmanaged .venv (3.4 GB) — [Deduplicate Now]`**.
4. Clicking **Deduplicate Now** executes:
```bash
# 1. Freeze exact dependencies if requirements.txt / pyproject.toml is missing
.venv/bin/python -m pip freeze > requirements.homemade.lock
# 2. Remove bloated standalone venv
rm -rf .venv
# 3. Rebuild via uv using Copy-on-Write reflinks/hardlinks from ~/.homemade/store/uv_cache
PYVER=$(.venv/bin/python -c 'import sys;print("%d.%d"%sys.version_info[:2])')   # capture BEFORE rm -rf
UV_CACHE_DIR=~/.homemade/store/uv_cache UV_LINK_MODE=clone uv venv .venv --python "$PYVER"
UV_CACHE_DIR=~/.homemade/store/uv_cache UV_LINK_MODE=clone uv pip install -r requirements.homemade.lock

```



### C. Working Directory Snapshot Differ for Output Shelf (`artifact_differ.rs`)

1. **Pre-Run Snapshot:** Right before spawning the process, walk `<app_root>` (skipping `.git`, `.venv`, `node_modules`, `__pycache__`) and record `HashMap<PathBuf, SystemTime>` of file modification times.
2. **Post-Run Diff:** On process exit (or live via `notify` watcher), any new or modified file is classified:
* Matches `artifacts.scratch_globs` $\rightarrow$ Inserted into `artifacts` table with `kind = 'scratch'`.
* Inside `artifacts.output_dir` or newly created media/data file (`.png`, `.mp4`, `.csv`, `.json`, `.pdf`, `.md`) $\rightarrow$ Inserted with `kind = 'output'` and immediately displayed in the **Output Shelf**.



---

## 9. Active GPU / VRAM Admission Queue & Telemetry (`scheduler.rs`)

### A. Unified VRAM Poller (macOS + Linux)

* **Linux (NVIDIA):** Queries `nvml_wrapper::Nvml` for `device.memory_info()` (`free`, `used`, `total`) and `device.running_compute_processes()` to map exact VRAM usage to child PIDs.
* **macOS (Apple Silicon Unified Memory):** There is no separate VRAM. Budget = Metal's `recommendedMaxWorkingSetSize` (≈ 75% of `hw.memsize`; read via a tiny `objc`/`metal` call or approximated). Used = `"In use system memory"` from `ioreg -r -d 1 -w 0 -c IOAccelerator` → `PerformanceStatistics` (verified present on macOS 26). Headroom = budget − used − current process RSS pressure from `sysinfo`. Over-allocation on Apple Silicon degrades into swap/thrash rather than a hard OOM, so the queue here prevents thrash rather than crashes; the UI copy should say "memory headroom" on macOS, not "VRAM".

### B. Admission Controller State Machine

```rust
pub async fn schedule_or_queue_job(state: &AppState, job: PendingJob) {
    if !job.manifest.resources.requires_gpu {
        spawn_immediately(state, job).await;
        return;
    }

    let required_vram_mb = job.manifest.resources.vram_estimate_mb
        .or(job.learned_peak_vram_mb)
        .unwrap_or(2048); // Default conservative 2GB if unknown on first run

    let current_free_vram = query_available_vram_mb();
    let active_gpu_jobs = state.count_running_gpu_jobs();

    // Admit immediately if no other GPU job is running OR enough headroom exists
    if active_gpu_jobs == 0 || current_free_vram >= required_vram_mb + 512 {
        spawn_immediately(state, job).await;
    } else {
        // Place in FIFO VRAM Queue and notify UI
        state.vram_queue.lock().await.push_back(job.clone());
        state.emit_to_ui("homemade://job-queued", json!({
            "runId": job.run_id,
            "appId": job.app_id,
            "reason": format!("Waiting for {} MB VRAM ({} MB free)", required_vram_mb, current_free_vram)
        }));
    }
}

```

Whenever a running process exits or every `1000ms` tick, the scheduler checks `vram_queue.front()` and pops the next waiting job as soon as VRAM clears—With a **"Run Anyway (Force)"** button in the UI if the user wants to override the queue.

---

## 10. Embedded CLI Agent Hub & Git Time-Machine

### A. Session Discovery Parsers (`integrations/agents.rs`)

When an app is selected, `homemade` scans local CLI agent stores associated with the app's canonical `root_dir`:

1. **Claude Code:** Scans `~/.claude/projects/<encoded-root-dir>/*.jsonl`, where `<encoded-root-dir>` is the absolute path with every `/` replaced by `-` (verified: `/Users/t/github/homemade` → `-Users-t-github-homemade`). Session ID = filename UUID. Title = first `type == "user"` line's text (skip the `memory/` subfolder). Resume: `claude --resume <id>`.
2. **Codex CLI:** Scans `~/.codex/sessions/YYYY/MM/DD/rollout-<ts>-<session_id>.jsonl`. Line 1 is `{"type":"session_meta","payload":{"session_id":..., "cwd":...}}` (verified). Match on `payload.cwd == root_dir`. Resume: `codex resume <session_id>`.
3. **Antigravity CLI (`agy`, Gemini):** Binary at `~/.local/bin/agy`; state under `~/.gemini/antigravity-cli/`. Read `conversation_summaries.db` (SQLite, table `conversation_summaries`: `conversation_id`, `title`, `preview`, `step_count`, `last_modified_time`, `workspace_uris` as `file://` URIs, `killed`) and match `workspace_uris` contains `file://<root_dir>`. Fallback/cross-check: `cache/conversation_metadata.json` (same fields under `summary`) and `history.jsonl` (`{display, timestamp, workspace, conversationId}` per user prompt, useful for titles when `title` is empty). Per-conversation transcripts are `conversations/<id>.db` (opaque, not needed for v1). Resume: `agy --conversation <id>`. New session: `agy --add-dir <root_dir>`.
4. **Custom Command:** Allows launching any shell agent in the embedded PTY while logging the transcript to `~/.homemade/sessions/<app_id>/<uuid>.log`.

### B. Resuming or Starting a Session in the Embedded PTY (`pty_manager.rs`)

When the user selects a session from the dropdown above the `xterm.js` pane:

* **Resume Claude Code:** Spawns `claude --resume <session_id>` inside `portable-pty` with `cwd = app.root_dir`.
* **New Session (deterministic provenance):** `homemade` pre-generates a UUID and spawns `claude --session-id <uuid> --append-system-prompt "<homemade context>"`, so the `agent_sessions` row can be written *before* the session starts instead of guessed from file mtimes afterwards. The appended context points at `homemade.toml`, the installed skill, and the app's last failed run log tail (if any).

### D. Local MCP Server for Coding Agents (`integrations/mcp_server.rs`)

Launched as `homemade --mcp` (the same binary; no window). Speaks MCP over stdio (JSON-RPC 2.0: `initialize`, `notifications/initialized`, `ping`, `tools/list`, `tools/call`). Opens `~/.homemade/state.db` **read-only** so an agent can inspect launcher state without the GUI running. Tools:

| Tool | Args | Returns |
|---|---|---|
| `list_apps` | – | id, name, root_dir, mode, last_run_at |
| `get_app` | `app_id` | manifest (parsed from disk) + root_dir + has_custom_ui |
| `list_runs` | `app_id`, `limit?` | runs newest first (status, exit_code, cli_command, started/finished) |
| `get_run_log` | `run_id` | `log_tail` text |
| `last_failed_run` | `app_id` | most recent failed/killed run + its log tail, or null |
| `list_artifacts` | `app_id`, `run_id?` | artifact rows |
| `validate_manifest` | `toml_text` | `{ok, errors[], normalized_toml}` using the same serde schema |

Registration (documented in `SKILL.md`): `claude mcp add homemade -- /Applications/Homemade.app/Contents/MacOS/homemade --mcp`. Runs are intentionally not startable over MCP in v1; they go through the GUI so the user sees them.

### E. Auto-Checkpoint Policy

`~/.homemade/config.toml` `[git] auto_checkpoint = true` (default). When true and the app root is a git repo, `homemade` commits dirty state before an agent PTY starts (`homemade: before <agent> session`) and after it exits (`homemade: after <agent> session <short title>`), storing both SHAs in `agent_sessions`. When false, only the SHAs are recorded.

### C. Git Checkpoint & 1-Click Revert (`integrations/git_ops.rs`)

* **1-Click Git Init:** If `<root_dir>/.git` does not exist, clicking **"Track with Git"** runs `git init` and writes a default `.gitignore`:
```gitignore
.venv/
venv/
__pycache__/
node_modules/
*.pt
*.pth
*.safetensors
*.gguf
*.bin
tmp_*

```


* **Auto-Checkpoint Hook:** Right before spawning an Agent PTY session, and right after the PTY session exits, if `git status --porcelain` is non-empty, `homemade` can optionally create or prompt for a commit (`git add -A && git commit -m "homemade checkpoint: [session title]"`), recording the commit hashes in `agent_sessions`.

---

## 11. The Default Agent `SKILL.md` Specification

This exact file is shipped in `src-tauri/resources/SKILL.md` and auto-installed to `~/.claude/skills/homemade/SKILL.md` (and copied to `.cursorrules` / `AGENTS.md` on request) so any coding agent knows how to build and link programs to Homemade on the first try.

```markdown
---
name: homemade-app-builder
description: Build, configure, and link self-made programs and custom HTML/JS frontends to the Homemade (homemade) launcher.
---

# Homemade — Agent Integration Skill

When the user asks you to build a tool, create a GUI for a script, or register a project in **Homemade**, follow this exact specification.

## 1. Environment & Dependency Rules (CRITICAL)
- **NEVER** create a standalone `python -m venv .venv` or download duplicate model weights into the project folder.
- Always use `uv` (`pyproject.toml` or inline script metadata). Homemade automatically routes `UV_CACHE_DIR`, `HF_HOME`, and `TORCH_HOME` to a shared Copy-on-Write store.
- Always support standard CLI execution (`argparse` or `typer`) OR `homemade_worker` warm RPC mode.

## 2. Creating `homemade.toml`
Always place a `homemade.toml` file in the root directory of the project:
- Set `exec = "uv run python main.py"` (or appropriate runner).
- Declare all CLI parameters under `[inputs.<name>]` so the launcher can validate inputs, save presets, and generate CLI replay strings.
- Set `[artifacts] output_dir = "./outputs"` and write all final generated files there.
- If the script uses PyTorch/CUDA/Metal, set `[resources] requires_gpu = true` and `vram_estimate_mb = <estimate>`.

## 3. Building a Custom Web Frontend (`ui/index.html`)
If the tool benefits from a custom UI (charts, image comparison, interactive canvas, dashboards):
1. Create `ui/index.html` (vanilla HTML/CSS/JS or single-bundle React/Tailwind) and set `[ui] entry = "ui/index.html"` in `homemade.toml`.
2. **Do NOT spin up a Flask/FastAPI backend** just to talk to Python. Use the pre-injected `window.homemade` bridge:
   - `const res = await window.homemade.run({ input_file: path, threshold: 0.8 });`
   - `const filePath = await window.homemade.pickFile({ extensions: [".mp4", ".png"] });`
   - `imgElement.src = window.homemade.assetUrl(filePath);`
   - `window.homemade.onProgress(({ percent, message }) => { ... });`
   - `window.homemade.onStdout((line) => { ... });`

## 4. Emitting Live Progress & Artifacts from Python
Inside the Python script, print `@homemade:` directives to `stdout` (with `flush=True`):
- `print('@homemade:progress 50 "Halfway done..."', flush=True)`
- `print(f'@homemade:artifact "{abs_output_path}"', flush=True)`
- `print('@homemade:metric {"accuracy": 0.98}', flush=True)`

```

---

## 12. Phase 2 Extension Trait: Embedded Tiny Model (`intelligence/traits.rs`)

> **Implemented (2026-10-14) with Apple's on-device Foundation Model.** On macOS 26+ Homemade talks to the OS-resident model (the one behind Apple Intelligence) through a 60 KB Swift helper (`homemade-fm`, JSON Lines over stdio), so there is nothing to download and no memory cost while idle. The first user-facing feature is **manifest drafting**: Homemade reads the repo's README and docs, pyproject/package.json, entry scripts and their CLI definitions, and asks the model for a `homemade.toml`, validated against the schema, plus a self-contained *agent prompt* that any coding agent (Claude Code, Codex, `agy`) can take to finish the adaptation. All three trait methods are wired: failed-run explanation, schema inference for scripts without argparse, and one-line agent-session summaries. On Linux / older macOS the `RuleBasedOnlyAssistor` no-op remains. Details: CONTRACT.md "Local model".

To keep v1 100% deterministic while making the Phase 2 local 0.5B–1.5B SLM (`llama.cpp` / ONNX) a drop-in module later, define this trait boundary in Rust during v1:

```rust
#[async_trait::async_trait]
pub trait SemanticAssistor: Send + Sync {
    /// Inspects raw unstructured stdout lines to detect silent failures, loops, or implicit progress
    async fn analyze_log_window(&self, recent_lines: &[String]) -> Option<SemanticHealthReport>;

    /// Infers homemade.toml [inputs] from scripts that don't use standard argparse/typer
    async fn infer_schema_from_raw_code(&self, source_code: &str) -> Option<IndexMap<String, InputFieldSpec>>;

    /// Summarizes a closed CLI agent session diff into a 1-line changelog for the session dropdown
    async fn summarize_agent_session(&self, git_diff: &str, transcript_tail: &str) -> Option<String>;
}

/// v1 Default Implementation: Zero-overhead No-Op
pub struct RuleBasedOnlyAssistor;

```

---

## 13. Step-by-Step Build Order (Implementation Roadmap)

1. **Milestone 1: Core Spawner & `homemade.toml` Runner (Days 1–3)**
* Initialize Tauri v2 + React + SQLite (`state.db`).
* Implement `load_user_login_env()`, `spawn_managed_process()` with `setsid()`, and `terminate_process_group()` 3-stage reaper.
* Implement `homemade.toml` parser + `ast_inspector.py` fallback + `<AutoForm/>` renderer + `"Copy as CLI"` button.


2. **Milestone 2: Custom UI Sandbox & Output Shelf (Days 4–6)**
* Register `homemade-asset://` streaming protocol handler.
* Build `<AppSandbox/>` iframe host + `homemade-bridge.js` (`window.homemade` RPC & native file picker/drop hooks).
* Implement pre/post run `artifact_differ.rs` and the `<OutputShelf/>` with native OS file drag-out (`tauri-plugin-drag`).


3. **Milestone 3: Storage Router & Active VRAM Scheduler (Days 7–9)**
* Wire up `~/.homemade/store/` shared environment variable injection (`UV_CACHE_DIR`, `HF_HOME`, `TORCH_HOME`) and unmanaged `.venv` migration scanner.
* Build `scheduler.rs` with `sysinfo` + `nvml-wrapper` / macOS memory telemetry and the FIFO VRAM Admission Queue.


4. **Milestone 4: Split-Pane Workbench, CLI Agent PTY & Git Bar (Days 10–12)**
* Integrate `dockview` split panes + `portable-pty` + `xterm.js`.
* Add session discovery for Claude Code, `agy`, and Codex + 1-click Git init/checkpoint/revert bar.
* Auto-install `SKILL.md` and wire up the window close-guard modal.
