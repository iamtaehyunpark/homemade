// OWNER: agent F. Zustand store synced with Tauri events (see CONTRACT.md for every event).
import { create } from "zustand";
import type { AppRecord, RunRecord, ResourceSnapshot } from "../lib/types";

interface HomemadeStore {
  apps: AppRecord[];
  runs: Record<string, RunRecord>; // by run id
  resources: ResourceSnapshot | null;
  selectedAppId: string | null;
  setApps: (apps: AppRecord[]) => void;
  upsertRun: (run: RunRecord) => void;
  setResources: (r: ResourceSnapshot) => void;
  select: (appId: string | null) => void;
}

export const useStore = create<HomemadeStore>((set) => ({
  apps: [],
  runs: {},
  resources: null,
  selectedAppId: null,
  setApps: (apps) => set({ apps }),
  upsertRun: (run) => set((s) => ({ runs: { ...s.runs, [run.id]: run } })),
  setResources: (resources) => set({ resources }),
  select: (selectedAppId) => set({ selectedAppId }),
}));
