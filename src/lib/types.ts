// Mirrors src-tauri/src/types.rs + manifest/schema.rs. camelCase for records, snake_case inside the manifest (serde default).
export type ExecutionMode = "cli" | "rpc_worker" | "webapp";
export type RunStatus = "queued_vram" | "running" | "stopping" | "success" | "failed" | "killed";
export type AgentType = "claude_code" | "codex" | "antigravity" | "custom_shell";
export type InputType = "file" | "directory" | "file_list" | "string" | "int" | "float" | "bool" | "select" | "range";

export interface InputFieldSpec {
  type: InputType;
  cli_flag?: string | null;
  positional_index?: number | null;
  label?: string | null;
  description?: string | null;
  required?: boolean | null;
  default?: unknown;
  options?: string[] | null;
  extensions?: string[] | null;
  min?: number | null;
  max?: number | null;
  step?: number | null;
  bool_style?: "flag_only" | "key_value" | null;
}
export interface HomemadeManifest {
  app: { id: string; name: string; version: string; description?: string | null; mode: ExecutionMode; exec: string; cwd: string; env_manager: string };
  ui: { entry?: string | null; default_layout: "tab" | "split_right" | "window" };
  resources: { requires_gpu: boolean; vram_estimate_mb?: number | null; max_ram_mb?: number | null; isolated_cache: boolean; graceful_timeout_ms: number; kill_timeout_ms: number };
  environment: { secrets: string[]; vars: Record<string, string> };
  artifacts: { output_dir: string; scratch_globs: string[] };
  inputs: Record<string, InputFieldSpec>;
}
export interface AppRecord {
  id: string; name: string; rootDir: string; manifestHash: string; executionMode: ExecutionMode;
  unmanagedVenvBytes: number; learnedPeakRamMb: number; learnedPeakVramMb: number; createdAt: number; lastRunAt: number | null;
  manifest: HomemadeManifest; hasGit: boolean; hasCustomUi: boolean;
}
export interface RunRecord {
  id: string; appId: string; status: RunStatus; inputsJson: string; cliCommand: string; exitCode: number | null;
  peakRamMb: number; peakVramMb: number; startedAt: number; finishedAt: number | null; logTail: string | null;
}
export interface ArtifactRecord { id: string; runId: string; appId: string; filePath: string; fileSizeBytes: number; mimeType: string; kind: "output" | "scratch"; createdAt: number }
export interface PresetRecord { id: string; appId: string; name: string; inputsJson: string; createdAt: number }
export interface AgentSessionRecord { sessionId: string; appId: string; agentType: AgentType; title: string | null; gitCommitBefore: string | null; gitCommitAfter: string | null; updatedAt: number }
export interface RunResourceUsage { runId: string; appId: string; cpuPct: number; ramMb: number; vramMb: number }
export interface QueuedJobInfo { runId: string; appId: string; requiredVramMb: number; reason: string }
export interface ResourceSnapshot { t?: number; cpuPct: number; ramUsedMb: number; ramTotalMb: number; gpuUsedMb: number; gpuTotalMb: number; gpuLabel: string; perRun: RunResourceUsage[]; queue: QueuedJobInfo[] }
export interface ListeningPort { port: number; pid: number; command: string; runId: string | null; appId: string | null; active: boolean }
export interface GitLogEntry { sha: string; message: string; timestamp: number; parents: string[]; refs: string[]; author: string }
export interface GitDirtyFile { path: string; status: string }
export interface GitStatus { initialized: boolean; dirty: boolean; head: string | null; log: GitLogEntry[]; dirtyFiles: GitDirtyFile[] }
export interface StorageRef { assetHash: string; assetPath: string; appId: string; sizeBytes: number }
export interface AppProcessInfo { runId: string; pid: number; pgid: number; port: number | null; ready: boolean }
export interface DetectedAgent { agentType: AgentType; binaryPath: string; displayName: string }
export interface PtyInfo { ptyId: string; appId: string; agentType: AgentType; sessionId: string | null }
export interface StorageSummary { storeRoot: string; uvCacheBytes: number; pnpmStoreBytes: number; sharedCacheBytes: number; unmanagedVenvTotalBytes: number }

// Local model (Phase 2)
export interface ModelStatus { provider: string; available: boolean; reason: string | null; helperPath: string | null }
export interface DraftResult { toml: string; agentPrompt: string; factsSummary: string; validationErrors: string[]; usedModel: boolean; notes: string[] }
export type HealthSeverity = "info" | "warn" | "error";
export interface HealthReport { severity: HealthSeverity; message: string }

// Event payloads (names in CONTRACT.md)
export interface RunLineEvent { runId: string; line: string }
export interface RunProgressEvent { runId: string; percent: number; message: string }
export interface RunMetricEvent { runId: string; data: Record<string, unknown> }
export interface RunPortEvent { runId: string; port: number }
export interface PtyDataEvent { ptyId: string; data: string }
export interface PtyExitEvent { ptyId: string; exitCode: number | null }
export interface PtyNoticeEvent { ptyId: string; appId: string; text: string }
export interface RunHealthEvent { runId: string; severity: HealthSeverity; message: string }
export interface PtySessionSummaryEvent { sessionId: string; title: string }
export interface ModelStatusEvent { available: boolean; reason: string | null }
