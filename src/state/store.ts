// OWNER: agent F. Zustand store synced with Tauri events (see CONTRACT.md for every event).
import { create } from "zustand";

/** Ticks kept for the Monitor graphs (1 Hz → 5 minutes). */
export const HISTORY_TICKS = 300;
import type { AppRecord, RunRecord, ResourceSnapshot, QueuedJobInfo, ModelStatus, HealthReport } from "../lib/types";

interface HomemadeStore {
  apps: AppRecord[];
  runs: Record<string, RunRecord>; // by run id
  resources: ResourceSnapshot | null;
  selectedAppId: string | null;
  openApps: string[];
  view: "workbench" | "settings" | "monitor" | "launch";
  resourceHistory: ResourceSnapshot[]; // last HISTORY_TICKS ticks (1 Hz), for the Monitor graphs
  confirmExit: number | null;
  /** Apps currently living in their own solo window (main window only). */
  windowed: Record<string, true>;
  setWindowed: (appId: string, on: boolean) => void;
  /** Element picker: click anything in the launcher to mention it in the agent session. */
  selectMode: boolean;
  setSelectMode: (on: boolean) => void;
  artifactTick: Record<string, number>;
  toast: string | null;
  queued: Record<string, QueuedJobInfo>; // by run id, from run://queued
  ports: Record<string, number>; // by run id, from run://port (webapp mode)
  webappReady: Record<string, boolean>; // by run id, from run://webapp_ready (port accepts connections)
  modelStatus: ModelStatus | null;
  health: Record<string, HealthReport>; // by run id, from run://health or explain_run
  createProjectOpen: boolean;
  setCreateProjectOpen: (open: boolean) => void;
  /** Apps fixed at the top of the sidebar (persisted per machine). */
  pinned: string[];
  togglePin: (appId: string) => void;
  /** Non-null → the shared "Remove app?" dialog is open for this app (sidebar, Launch, header menu). */
  removeAppId: string | null;
  setRemoveAppId: (id: string | null) => void;
  pendingShellAppId: string | null;
  setPendingShellAppId: (id: string | null) => void;
  draftPath: string | null; // non-null → DraftManifestModal open for this folder
  sessionSummaryTick: number; // bumped on pty://session_summary
  setModelStatus: (m: ModelStatus | null) => void;
  setHealth: (runId: string, h: HealthReport) => void;
  setDraftPath: (p: string | null) => void;
  bumpSessionSummary: () => void;
  setApps: (apps: AppRecord[]) => void;
  upsertApp: (app: AppRecord) => void;
  removeApp: (appId: string) => void;
  setQueued: (q: QueuedJobInfo) => void;
  setPort: (runId: string, port: number) => void;
  setWebappReady: (runId: string) => void;
  upsertRun: (run: RunRecord) => void;
  setResources: (r: ResourceSnapshot) => void;
  select: (appId: string | null) => void;
  openApp: (appId: string) => void;
  closeApp: (appId: string) => void;
  setView: (v: HomemadeStore["view"]) => void;
  setConfirmExit: (n: number | null) => void;
  bumpArtifacts: (appId: string) => void;
  notify: (msg: string | null) => void;
}

export const useStore = create<HomemadeStore>((set) => ({
  apps: [],
  runs: {},
  resources: null,
  resourceHistory: [],
  selectedAppId: null,
  openApps: [],
  view: (() => { try { const v = localStorage.getItem("homemade.view"); return v === "launch" || v === "monitor" || v === "settings" ? v : "workbench"; } catch { return "workbench"; } })(),
  confirmExit: null,
  windowed: {},
  selectMode: false,
  setSelectMode: (on) => set({ selectMode: on }),
  setWindowed: (appId, on) => set((s) => { const w = { ...s.windowed }; if (on) w[appId] = true; else delete w[appId]; return { windowed: w }; }),
  artifactTick: {},
  toast: null,
  queued: {},
  ports: {},
  webappReady: {},
  modelStatus: null,
  health: {},
  createProjectOpen: false,
  setCreateProjectOpen: (createProjectOpen) => set({ createProjectOpen }),
  pinned: (() => { try { return JSON.parse(localStorage.getItem("homemade.pinned") ?? "[]") as string[]; } catch { return []; } })(),
  togglePin: (id) => set((s) => {
    const pinned = s.pinned.includes(id) ? s.pinned.filter((x) => x !== id) : [...s.pinned, id];
    try { localStorage.setItem("homemade.pinned", JSON.stringify(pinned)); } catch { /* ignore */ }
    return { pinned };
  }),
  removeAppId: null,
  setRemoveAppId: (removeAppId) => set({ removeAppId }),
  pendingShellAppId: null,
  setPendingShellAppId: (pendingShellAppId) => set({ pendingShellAppId }),
  draftPath: null,
  sessionSummaryTick: 0,
  setModelStatus: (modelStatus) => set({ modelStatus }),
  setHealth: (runId, h) => set((s) => ({ health: { ...s.health, [runId]: h } })),
  setDraftPath: (draftPath) => set({ draftPath }),
  bumpSessionSummary: () => set((s) => ({ sessionSummaryTick: s.sessionSummaryTick + 1 })),
  setApps: (apps) => set({ apps }),
  upsertApp: (app) => set((s) => {
    const i = s.apps.findIndex((a) => a.id === app.id);
    const apps = [...s.apps];
    if (i >= 0) apps[i] = app; else apps.push(app);
    return { apps };
  }),
  removeApp: (id) => set((s) => ({
    apps: s.apps.filter((a) => a.id !== id),
    openApps: s.openApps.filter((x) => x !== id),
    selectedAppId: s.selectedAppId === id ? null : s.selectedAppId,
  })),
  setQueued: (q) => set((s) => ({ queued: { ...s.queued, [q.runId]: q } })),
  setPort: (runId, port) => set((s) => ({ ports: { ...s.ports, [runId]: port } })),
  setWebappReady: (runId) => set((s) => ({ webappReady: { ...s.webappReady, [runId]: true } })),
  upsertRun: (run) => set((s) => ({ runs: { ...s.runs, [run.id]: run } })),
  setResources: (resources) => set((s) => {
    const h = s.resourceHistory.length >= HISTORY_TICKS ? s.resourceHistory.slice(s.resourceHistory.length - HISTORY_TICKS + 1) : s.resourceHistory;
    return { resources, resourceHistory: [...h, { ...resources, t: Date.now() } as ResourceSnapshot] };
  }),
  select: (selectedAppId) => set({ selectedAppId }),
  openApp: (id) => set((s) => ({ openApps: s.openApps.includes(id) ? s.openApps : [...s.openApps, id], selectedAppId: id, view: "workbench" })),
  closeApp: (id) => set((s) => {
    const openApps = s.openApps.filter((x) => x !== id);
    return { openApps, selectedAppId: s.selectedAppId === id ? (openApps[openApps.length - 1] ?? null) : s.selectedAppId };
  }),
  setView: (view) => set({ view }),
  setConfirmExit: (confirmExit) => set({ confirmExit }),
  bumpArtifacts: (appId) => set((s) => ({ artifactTick: { ...s.artifactTick, [appId]: (s.artifactTick[appId] ?? 0) + 1 } })),
  notify: (toast) => set({ toast }),
}));

/** Most recent run for an app (by startedAt). */
export function latestRun(runs: Record<string, RunRecord>, appId: string): RunRecord | null {
  let best: RunRecord | null = null;
  for (const r of Object.values(runs)) if (r.appId === appId && (!best || r.startedAt > best.startedAt)) best = r;
  return best;
}

// Reopen where the user left off: the last page is restored on the next launch (sidebar collapse is kept by Sidebar itself).
useStore.subscribe((s, prev) => { if (s.view !== prev.view) { try { localStorage.setItem("homemade.view", s.view); } catch { /* ignore */ } } });
