# Homemade (`homemade`)

**GUI Everything: The Operating Layer for Personal, AI-Generated Software**

---

## 1. The Core Premise: Why This Exists

Software is undergoing a fundamental shift from **finding products on the web** to **generating bespoke tools on demand.**

With modern CLI coding agents (Claude Code, Codex, `agy`, Aider), building a custom Python script, data scraper, local AI pipeline, or media utility now takes five minutes. A single power user can easily create 30 to 100 personal programs over a year—each tailored to an exact workflow that no commercial SaaS product covers.

**Creation is no longer the bottleneck. Operation and maintenance are.**

Right now, self-made software dies in a local graveyard of forgotten folders:

* **The Interface Gap:** Most custom scripts stay trapped as raw CLI tools. Six months after writing a script, you forget its exact flags, argument order, and expected input formats.
* **The Lifecycle Mess:** Running, closing, and deleting ad-hoc scripts is chaotic. Closing a terminal leaves zombie child processes eating RAM or locking GPU memory. Deleting a script folder leaves behind gigabytes of orphaned dependencies and cached weights.
* **The Storage Bloat:** Standard isolation (`venv` or `node_modules` per project) duplicates heavy libraries like PyTorch, CUDA, and model weights dozens of times across the same disk—wasting 50GB to 100GB+ on identical files.
* **The Lost Context:** A self-made program isn't just source code anymore; its true "source" includes the AI agent sessions that built it. When a script breaks months later, hunting down *which* agent built it, *which* session history holds the architectural context, and *how* to safely roll back a bad agent edit is entirely manual.

**Homemade** (CLI: `homemade`) exists to solve this. It is a lightweight, unified launcher and runtime workspace designed specifically for programs you build for yourself.

---

## 2. Core Philosophy

1. **Feel Like a Real App, Never Require Deployment:** Your custom tools should have clean interfaces, saved presets, live progress bars, and an output file shelf—without ever touching app store packaging, code signing, web server boilerplate, or browser sandboxes.
2. **Zero Restriction on Machine Capability:** The launcher never handicaps your code. Programs run directly as native OS processes with 100% access to your filesystem, CPU cores, and GPU hardware.
3. **Don't Rebuild the OS—Provide the Semantics the OS Lacks:** Operating systems already excel at CPU thread scheduling and virtual memory paging. `homemade` steps in strictly where the OS is blind: deduplicating virtual environments across projects, preventing non-preemptive GPU VRAM Out-Of-Memory (OOM) crashes, reaping orphan process trees, and linking executables to the AI agent sessions that authored them.
4. **Built for Agentic Collaboration:** Instead of forcing humans to write UI boilerplate, `homemade` ships with a clear contract (`homemade.toml`) and default **Agent Skills**. You tell your coding agent what tool you want; the agent builds the logic, generates a custom HTML/CSS/JS frontend, and registers it into the launcher automatically.
5. **Lightweight by Design:** `homemade` behaves like a normal desktop application or browser. When it is open, it works; when you close it, it shuts down completely (warning you if jobs are still running)—leaving zero hidden daemons draining your machine. v1 targets macOS (Apple Silicon) and Linux; Windows is a later port.

---

## 3. The Four Pillars of Homemade

### Pillar 1: Frictionless GUI & Frontend Bridge

Every self-made program immediately gets a graphical interface through one of two paths:

* **Zero-Effort Auto-GUI:** Point `homemade` at any existing CLI script. It automatically inspects the script's CLI flags (`argparse`, `typer`, `--help`) and renders a clean web form with file pickers, toggles, dropdowns, and sliders—plus saved input presets and a 1-click *"Copy as CLI Command"* button.
* **Agent-Crafted Custom Frontends:** When a tool needs a richer interface (an interactive image canvas, live charts, or a custom dashboard), your coding agent uses the built-in `homemade` Skill to drop a plain `ui/index.html` file into the folder. The launcher hosts this frontend in an isolated sandbox and connects it directly to your script via `window.homemade`—eliminating the need to write or maintain a Flask/FastAPI backend.

### Pillar 2: Clean Run, Close, and Delete (Lifecycle Discipline)

Self-made programs are often written quickly without production-grade error handling or teardown logic. `homemade` wraps every program in a strict operational runbook:

* **Clean Run:** Automatically loads the user's true shell environment, injects centralized API keys from a secure vault, and tracks every execution in a replayable history journal.
* **Clean Close:** Wraps every run in an OS Process Group. Clicking **Stop** executes a deterministic 3-stage shutdown (`SIGINT` $\rightarrow$ `SIGTERM` $\rightarrow$ `SIGKILL`), guaranteeing that spawned child workers, multiprocessing pools, and locked ports are 100% cleaned up.
* **Output Shelf & Clean Delete:** Automatically separates final generated outputs (images, CSVs, videos—surfaced in a drag-and-drop **Output Shelf**) from temporary scratch files. Deleting an app cleanly purges its scratch data and unreferenced packages without touching shared libraries used by other tools.

### Pillar 3: Semantic Storage & GPU Resource Management

When you run dozens of personal AI and automation scripts on one machine, hardware resources collide. `homemade` manages what the OS cannot see:

* **Zero-Duplicate Virtual Environments & Weights:** Instead of downloading PyTorch or HuggingFace models 20 times across 20 folders, `homemade` routes package managers (`uv`, `pnpm`) and model caches (`HF_HOME`, `TORCH_HOME`) through a central Content-Addressable Store using Copy-on-Write links. Each app keeps its own isolated environment, while taking **0 MB of duplicate disk space**. Existing bloated `.venv` folders are automatically detected for 1-click deduplication.
* **Active GPU / VRAM Admission Control:** Unlike system RAM, GPU VRAM crashes immediately when two scripts over-allocate memory simultaneously. `homemade` monitors real-time VRAM headroom and tracks each tool's memory footprint—automatically queuing a second heavy GPU job until the first finishes, preventing hard OOM crashes.

### Pillar 4: Native CLI Agent & Version Hub

Because personal software is continuously modified by AI agents, `homemade` treats the **coding agent session** as a first-class part of the program:

* **Side-by-Side Agent Workspace:** Split the launcher window just like an IDE—run your program's live GUI on the right while interacting with your CLI coding agent (Claude Code, `agy`, Codex) in an embedded terminal on the left, with instant hot-reload as the agent edits your code.
* **Session Provenance & Fast Resume:** `homemade` automatically detects and indexes which agent sessions belong to which program. Months later, you can see a dropdown of every past session for that specific tool and resume the exact conversation with one click.
* **1-Click Git Checkpoints:** Easily initialize Git tracking with smart defaults (ignoring heavy weights and virtual environments) and roll back to working checkpoints whenever an agent edit breaks a working tool.

---

## 4. The User Journey: Before vs. After `homemade`

| Stage | Without `homemade` (Current Reality) | With `homemade` (Homemade) |
| --- | --- | --- |
| **1. Building a Tool** | Ask an agent for a Python script. Get a raw CLI tool or hack together a heavy Streamlit/FastAPI server just to have buttons. | Ask your agent to build the tool using the `homemade` skill. Get a clean script + custom HTML frontend linked automatically via `homemade.toml`. |
| **2. Dependencies** | Create another `.venv`, wait 3 minutes downloading another 4GB copy of PyTorch and Whisper weights. | Linked in 0.2 seconds to the central Copy-on-Write store; 0 MB of duplicate downloads. |
| **3. Daily Execution** | Open terminal, `cd` to folder, activate `venv`, check `README` for flags, type paths manually, hunt in Finder for output files. | Open `homemade`, pick a saved preset (or drop a file), click **Run**, and drag the finished file directly out of the **Output Shelf**. |
| **4. Multitasking** | Run two GPU scripts at once $\rightarrow$ both crash at 90% with `CUDA OutOfMemoryError`. Hit `Ctrl+C` $\rightarrow$ zombie processes stay stuck in RAM. | Second GPU job waits cleanly in the VRAM queue and auto-starts when the GPU clears. Clicking **Stop** reaps the entire process tree. |
| **5. Fixing & Evolving** | Open a blank agent session 6 months later; burn tokens while the agent re-reads the repo from scratch. | Click **Edit with Agent** inside the app's tab, select the historical session from the dropdown, and test changes live in the split pane. |

---

## 5. Summary Vision

As AI turns every computer user into a prolific software creator, the desktop needs a new kind of application layer. **Homemade** is not an IDE for writing code from scratch, nor is it a bloated container platform for enterprise cloud deployment. It is the missing personal runtime that turns a messy collection of self-made scripts into a cohesive, reliable, and effortless suite of native-feeling tools.
