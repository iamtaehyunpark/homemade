// Typed wrappers over every Tauri command (contract: CONTRACT.md). Keep argument names camelCase; Tauri maps to snake_case params.
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type * as T from "./types";

export const ipc = {
  // apps
  listApps: () => invoke<T.AppRecord[]>("list_apps"),
  importApp: (path: string) => invoke<T.AppRecord>("import_app", { path }),
  getApp: (appId: string) => invoke<T.AppRecord>("get_app", { appId }),
  reloadManifest: (appId: string) => invoke<T.AppRecord>("reload_manifest", { appId }),
  removeApp: (appId: string, purgeScratch: boolean, deleteVenv: boolean) => invoke<void>("remove_app", { appId, purgeScratch, deleteVenv }),
  saveManifest: (appId: string, tomlText: string) => invoke<T.AppRecord>("save_manifest", { appId, tomlText }),
  // runs
  startRun: (appId: string, inputs: Record<string, unknown>) => invoke<T.RunRecord>("start_run", { appId, inputs }),
  stopRun: (runId: string) => invoke<void>("stop_run", { runId }),
  forceRun: (runId: string) => invoke<void>("force_run", { runId }),
  listRuns: (appId: string, limit?: number) => invoke<T.RunRecord[]>("list_runs", { appId, limit }),
  getRun: (runId: string) => invoke<T.RunRecord | null>("get_run", { runId }),
  getRunLog: (runId: string) => invoke<string[]>("get_run_log", { runId }),
  rpcCall: (appId: string, method: string, params: unknown, runId?: string) => invoke<unknown>("rpc_call", { appId, runId: runId ?? null, method, params }),
  buildCliCommand: (appId: string, inputs: Record<string, unknown>) => invoke<string>("build_cli_command", { appId, inputs }),
  listActiveRuns: () => invoke<T.RunRecord[]>("list_active_runs"),
  // presets
  listPresets: (appId: string) => invoke<T.PresetRecord[]>("list_presets", { appId }),
  savePreset: (appId: string, name: string, inputs: Record<string, unknown>) => invoke<T.PresetRecord>("save_preset", { appId, name, inputs }),
  deletePreset: (presetId: string) => invoke<void>("delete_preset", { presetId }),
  // artifacts
  listArtifacts: (appId: string, runId?: string, kind?: "output" | "scratch") => invoke<T.ArtifactRecord[]>("list_artifacts", { appId, runId: runId ?? null, kind: kind ?? null }),
  cleanScratch: (appId: string) => invoke<number>("clean_scratch", { appId }),
  deleteArtifact: (appId: string, filePath: string, deleteFile: boolean) => invoke<void>("delete_artifact", { appId, filePath, deleteFile }),
  // storage / secrets
  storageSummary: () => invoke<T.StorageSummary>("storage_summary"),
  scanVenv: (appId: string) => invoke<number>("scan_venv", { appId }),
  migrateVenv: (appId: string) => invoke<number>("migrate_venv", { appId }),
  storageRefs: (appId: string) => invoke<T.StorageRef[]>("storage_refs", { appId }),
  listSecretNames: () => invoke<string[]>("list_secret_names"),
  setSecret: (name: string, value: string) => invoke<void>("set_secret", { name, value }),
  deleteSecret: (name: string) => invoke<void>("delete_secret", { name }),
  // resources
  resourceSnapshot: () => invoke<T.ResourceSnapshot>("resource_snapshot"),
  // pty
  ptySpawn: (appId: string, agentType: T.AgentType, opts: { sessionId?: string; command?: string; cols: number; rows: number }) =>
    invoke<T.PtyInfo>("pty_spawn", { appId, agentType, sessionId: opts.sessionId ?? null, command: opts.command ?? null, cols: opts.cols, rows: opts.rows }),
  ptyWrite: (ptyId: string, data: string) => invoke<void>("pty_write", { ptyId, data }),
  ptyResize: (ptyId: string, cols: number, rows: number) => invoke<void>("pty_resize", { ptyId, cols, rows }),
  ptyKill: (ptyId: string) => invoke<void>("pty_kill", { ptyId }),
  listPtys: () => invoke<T.PtyInfo[]>("list_ptys"),
  // agents
  detectAgents: () => invoke<T.DetectedAgent[]>("detect_agents"),
  listAgentSessions: (appId: string) => invoke<T.AgentSessionRecord[]>("list_agent_sessions", { appId }),
  installSkill: () => invoke<string>("install_skill"),
  // git
  gitStatus: (appId: string) => invoke<T.GitStatus>("git_status", { appId }),
  gitInit: (appId: string) => invoke<T.GitStatus>("git_init", { appId }),
  gitCheckpoint: (appId: string, message: string) => invoke<string>("git_checkpoint", { appId, message }),
  gitRevertTo: (appId: string, sha: string) => invoke<T.GitStatus>("git_revert_to", { appId, sha }),
  // system
  activeCount: () => invoke<number>("active_count"),
  confirmQuit: () => invoke<void>("confirm_quit"),
  listListeningPorts: () => invoke<T.ListeningPort[]>("list_listening_ports"),
  killPortOwner: (pid: number) => invoke<void>("kill_port_owner", { pid }),
  appActivity: (appId: string) => invoke<{ appId: string; state: "running" | "idle"; current: unknown; lastFinished: T.RunRecord | null; recentRuns: T.RunRecord[]; latestOutputs: T.ArtifactRecord[] }>("app_activity", { appId }),
  appProcessInfo: (appId: string) => invoke<T.AppProcessInfo | null>("app_process_info", { appId }),
  revealInFinder: (path: string) => invoke<void>("reveal_in_finder", { path }),
  homemadeHome: () => invoke<string>("homemade_home"),
  readTextFile: (path: string) => invoke<string>("read_text_file", { path }),
  // local model
  modelStatus: () => invoke<T.ModelStatus>("model_status"),
  draftManifest: (path: string) => invoke<T.DraftResult>("draft_manifest", { path }),
  applyManifest: (path: string, toml: string) => invoke<T.AppRecord>("apply_manifest", { path, toml }),
  explainRun: (runId: string) => invoke<T.HealthReport>("explain_run", { runId }),
  modelComplete: (system: string, prompt: string) => invoke<string>("model_complete", { system, prompt }),
  // e2e harness
  e2eReport: (msg: string) => invoke<void>("e2e_report", { msg }),
  e2eDone: (code: number) => invoke<void>("e2e_done", { code }),
};

export const events = {
  runStdout: "run://stdout", runStderr: "run://stderr", runProgress: "run://progress", runMetric: "run://metric",
  runArtifact: "run://artifact", runPort: "run://port", runWebappReady: "run://webapp_ready", runStatus: "run://status", runQueued: "run://queued", runWorkerReady: "run://worker_ready",
  ptyData: "pty://data", ptyExit: "pty://exit", ptySessionSummary: "pty://session_summary",
  runHealth: "run://health", modelStatus: "model://status",
  appChanged: "app://changed", appUiReload: "app://ui_reload", confirmExit: "app://confirm-exit", notice: "app://notice",
  resourcesTick: "resources://tick", storageProgress: "storage://progress",
} as const;

export function on<T>(event: string, cb: (payload: T) => void): Promise<UnlistenFn> {
  return listen<T>(event, (e) => cb(e.payload));
}

/** Convert an absolute path into a URL the webview can load (images, video, audio). */
export function assetUrl(absPath: string): string {
  return `homemade-asset://localhost/${encodeURIComponent(absPath)}`;
}
/** URL of an app's custom UI entry served by the homemade-app protocol. */
export function appUiUrl(appId: string): string {
  return `homemade-app://${appId}.localhost/index.html`;
}
