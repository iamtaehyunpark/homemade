// Live per-run output (log lines, progress, metrics), fed by useTauriEvents.
import { create } from "zustand";

export const MAX_LINES = 2000;
export interface LogLine { err: boolean; text: string }
export interface RunLive { lines: LogLine[]; progress: { percent: number; message: string } | null; metrics: Record<string, unknown> }

interface LiveStore {
  runs: Record<string, RunLive>;
  pushLines: (batch: Record<string, LogLine[]>) => void;
  seed: (runId: string, lines: LogLine[]) => void;
  setProgress: (runId: string, percent: number, message: string) => void;
  mergeMetric: (runId: string, data: Record<string, unknown>) => void;
}

const empty = (): RunLive => ({ lines: [], progress: null, metrics: {} });

export const useLive = create<LiveStore>((set) => ({
  runs: {},
  pushLines: (batch) => set((s) => {
    const runs = { ...s.runs };
    for (const [id, add] of Object.entries(batch)) {
      const cur = runs[id] ?? empty();
      const lines = cur.lines.concat(add);
      runs[id] = { ...cur, lines: lines.length > MAX_LINES ? lines.slice(lines.length - MAX_LINES) : lines };
    }
    return { runs };
  }),
  seed: (runId, lines) => set((s) => (s.runs[runId]?.lines.length ? s : { runs: { ...s.runs, [runId]: { ...(s.runs[runId] ?? empty()), lines: lines.slice(-MAX_LINES) } } })),
  setProgress: (runId, percent, message) => set((s) => ({ runs: { ...s.runs, [runId]: { ...(s.runs[runId] ?? empty()), progress: { percent, message } } } })),
  mergeMetric: (runId, data) => set((s) => {
    const cur = s.runs[runId] ?? empty();
    return { runs: { ...s.runs, [runId]: { ...cur, metrics: { ...cur.metrics, ...data } } } };
  }),
}));
