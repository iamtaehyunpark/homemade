// OWNER: agent F. Zustand store synced with Tauri events (see CONTRACT.md for every event).
import { create } from "zustand";
import type { AppRecord, RunRecord, ResourceSnapshot, QueuedJobInfo } from "../lib/types";

interface HomemadeStore {
  apps: AppRecord[];
  runs: Record<string, RunRecord>; // by run id
  resources: ResourceSnapshot | null;
  selectedAppId: string | null;
  openApps: string[];
  view: "workbench" | "settings";
  confirmExit: number | null;
  artifactTick: Record<string, number>;
  toast: string | null;
  queued: Record<string, QueuedJobInfo>; // by run id, from run://queued
  ports: Record<string, number>; // by run id, from run://port (webapp mode)
  setApps: (apps: AppRecord[]) => void;
  upsertApp: (app: AppRecord) => void;
  removeApp: (appId: string) => void;
  setQueued: (q: QueuedJobInfo) => void;
  setPort: (runId: string, port: number) => void;
  upsertRun: (run: RunRecord) => void;
  setResources: (r: ResourceSnapshot) => void;
  select: (appId: string | null) => void;
  openApp: (appId: string) => void;
  closeApp: (appId: string) => void;
  setView: (v: "workbench" | "settings") => void;
  setConfirmExit: (n: number | null) => void;
  bumpArtifacts: (appId: string) => void;
  notify: (msg: string | null) => void;
}

export const useStore = create<HomemadeStore>((set) => ({
  apps: [],
  runs: {},
  resources: null,
  selectedAppId: null,
  openApps: [],
  view: "workbench",
  confirmExit: null,
  artifactTick: {},
  toast: null,
  queued: {},
  ports: {},
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
  upsertRun: (run) => set((s) => ({ runs: { ...s.runs, [run.id]: run } })),
  setResources: (resources) => set({ resources }),
  select: (selectedAppId) => set({ selectedAppId }),
  openApp: (id) => set((s) => ({ openApps: s.openApps.includes(id) ? s.openApps : [...s.openApps, id], selectedAppId: id, view: "workbench" })),
  closeApp: (id) => set((s) => ({ openApps: s.openApps.filter((x) => x !== id) })),
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
