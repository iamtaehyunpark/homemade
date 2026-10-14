// Global Tauri event listeners -> zustand stores. Mount once in App.
import { useEffect } from "react";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { ipc, on, events } from "./ipc";
import type { QueuedJobInfo, ArtifactRecord, ResourceSnapshot, RunLineEvent, RunMetricEvent, RunProgressEvent, RunRecord, RunHealthEvent, PtySessionSummaryEvent, ModelStatusEvent } from "./types";
import { useStore } from "../state/store";
import { useLive, type LogLine } from "../state/live";
import { isTerminal } from "./format";

export function useTauriEvents() {
  useEffect(() => {
    let disposed = false;
    const unlisten: UnlistenFn[] = [];
    let pending: Record<string, LogLine[]> = {};
    let raf = 0;
    const flush = () => { raf = 0; const b = pending; pending = {}; useLive.getState().pushLines(b); };
    const pushLine = (runId: string, line: LogLine) => {
      (pending[runId] ??= []).push(line);
      if (!raf) raf = requestAnimationFrame(flush);
    };
    const st = useStore.getState;

    const subs: Promise<UnlistenFn>[] = [
      on<RunRecord>(events.runStatus, (r) => {
        const prev = st().runs[r.id];
        st().upsertRun(r);
        if (isTerminal(r.status) && !isTerminal(prev?.status)) {
          st().bumpArtifacts(r.appId);
          ipc.getApp(r.appId).then(st().upsertApp).catch(() => {});
        }
      }),
      on<QueuedJobInfo>(events.runQueued, (q) => st().setQueued(q)),
      on<{ runId: string; port: number }>(events.runPort, (p) => st().setPort(p.runId, p.port)),
      on<{ runId: string; port: number }>(events.runWebappReady, (p) => st().setWebappReady(p.runId)),
      on<RunLineEvent>(events.runStdout, (p) => pushLine(p.runId, { err: false, text: p.line })),
      on<RunLineEvent>(events.runStderr, (p) => pushLine(p.runId, { err: true, text: p.line })),
      on<RunProgressEvent>(events.runProgress, (p) => useLive.getState().setProgress(p.runId, p.percent, p.message)),
      on<RunMetricEvent>(events.runMetric, (p) => useLive.getState().mergeMetric(p.runId, p.data)),
      on<ArtifactRecord>(events.runArtifact, (a) => st().bumpArtifacts(a.appId)),
      on<ResourceSnapshot>(events.resourcesTick, (r) => st().setResources(r)),
      on<{ appId: string }>(events.appChanged, ({ appId }) => { ipc.getApp(appId).then(st().upsertApp).catch(() => {}); }),
      on<number>(events.confirmExit, (n) => st().setConfirmExit(n)),
      on<{ appId: string; message: string }>(events.storageProgress, (p) => st().notify(p.message)),
      on<{ message: string }>(events.notice, (p) => st().notify(p.message)),
      on<RunHealthEvent>(events.runHealth, (p) => st().setHealth(p.runId, { severity: p.severity, message: p.message })),
      on<PtySessionSummaryEvent>(events.ptySessionSummary, () => st().bumpSessionSummary()),
      on<ModelStatusEvent>(events.modelStatus, (p) => {
        const cur = st().modelStatus;
        st().setModelStatus({ provider: cur?.provider ?? "apple", helperPath: cur?.helperPath ?? null, available: p.available, reason: p.reason ?? null });
      }),
    ];
    for (const s of subs) s.then((u) => (disposed ? u() : unlisten.push(u))).catch(() => {});

    ipc.listApps().then(st().setApps).catch((e) => console.error("listApps", e));
    ipc.listActiveRuns().then((rs) => rs.forEach(st().upsertRun)).catch(() => {});
    ipc.resourceSnapshot().then(st().setResources).catch(() => {});
    ipc.modelStatus().then(st().setModelStatus).catch(() => {});

    return () => { disposed = true; unlisten.forEach((u) => u()); if (raf) cancelAnimationFrame(raf); };
  }, []);
}
