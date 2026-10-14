// OWNER: agent F. Sandboxed iframe for an app's custom UI + host side of the homemade bridge protocol (CONTRACT.md).
import { useEffect, useRef, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { appUiUrl, events, ipc, on } from "../lib/ipc";
import type { AppRecord, RunRecord } from "../lib/types";
import { useStore, latestRun } from "../state/store";
import { useDropTarget } from "../lib/dropRouter";
import { errMsg, isActive, platformName } from "../lib/format";
import { sandboxes } from "../lib/registry";

type Params = Record<string, unknown>;
const forwarded: [string, string][] = [
  [events.runStdout, "stdout"], [events.runStderr, "stderr"], [events.runProgress, "progress"], [events.runMetric, "metric"],
  [events.runArtifact, "artifact"], [events.runStatus, "status"], [events.runWorkerReady, "worker_ready"],
];

export default function AppSandbox({ app }: { app: AppRecord }) {
  const frame = useRef<HTMLIFrameElement>(null);
  const [reloadKey, setReloadKey] = useState(0);
  const [bridgeReady, setBridgeReady] = useState(false);
  useEffect(() => setBridgeReady(false), [reloadKey]);
  const appRef = useRef(app);
  appRef.current = app;

  const post = (msg: Record<string, unknown>) => frame.current?.contentWindow?.postMessage({ homemade: 1, ...msg }, "*");
  const dropRef = useDropTarget<HTMLDivElement>((paths) => post({ type: "event", name: "file_drop", payload: { paths } }));

  // RPC handling
  useEffect(() => {
    const appId = app.id;
    const handle = async (method: string, p: Params): Promise<unknown> => {
      switch (method) {
        case "run": {
          const inputs = (p.inputs ?? p) as Record<string, unknown>;
          const run = await ipc.startRun(appId, inputs);
          useStore.getState().upsertRun(run);
          return { runId: run.id, ...run };
        }
        case "call": return ipc.rpcCall(appId, String(p.method), p.params ?? {}, p.runId as string | undefined);
        case "stop": {
          const runId = (p.runId as string) ?? Object.values(useStore.getState().runs).find((r) => r.appId === appId && isActive(r.status))?.id;
          if (runId) await ipc.stopRun(runId);
          return null;
        }
        case "pick_file": {
          const exts = (p.extensions as string[] | undefined)?.map((e) => e.replace(/^\./, ""));
          return open({ multiple: !!p.multiple, directory: false, filters: exts?.length ? [{ name: exts.join(", "), extensions: exts }] : undefined });
        }
        case "pick_folder": return open({ directory: true, multiple: false });
        case "get_schema": return appRef.current.manifest.inputs;
        case "get_presets": return (await ipc.listPresets(appId)).map((r) => ({ ...r, inputs: safeParse(r.inputsJson) }));
        case "save_preset": return ipc.savePreset(appId, String(p.name), (p.inputs ?? {}) as Record<string, unknown>);
        case "list_artifacts": return ipc.listArtifacts(appId, p.runId as string | undefined, p.kind as "output" | "scratch" | undefined);
        case "reveal": return ipc.revealInFinder(String(p.path));
        case "log": console.log(`[${appRef.current.name}]`, ...(Array.isArray(p.args) ? p.args : [p.message ?? p])); return null;
        default: throw new Error(`Unknown method: ${method}`);
      }
    };
    const onMessage = (e: MessageEvent) => {
      if (!frame.current || e.source !== frame.current.contentWindow) return;
      const m = e.data;
      if (!m || m.homemade !== 1) return;
      if (m.type === "ready") {
        post({ type: "init", appId, schema: appRef.current.manifest.inputs, platform: platformName(), e2e: true });
        setBridgeReady(true);
      } else if (m.type === "e2e_result") {
        const w = evalWaiters.get(m.id);
        if (w) { evalWaiters.delete(m.id); m.ok ? w.resolve(m.result) : w.reject(new Error(String(m.error))); }
      } else if (m.type === "rpc") {
        handle(String(m.method), (m.params ?? {}) as Params).then(
          (result) => post({ type: "rpc_result", id: m.id, ok: true, result: result ?? null }),
          (err) => post({ type: "rpc_result", id: m.id, ok: false, error: errMsg(err) }),
        );
      }
    };
    const evalWaiters = new Map<string, { resolve: (v: unknown) => void; reject: (e: unknown) => void }>();
    let n = 0;
    sandboxes.set(appId, (code) => new Promise((resolve, reject) => {
      const id = `e${++n}`;
      evalWaiters.set(id, { resolve, reject });
      post({ type: "e2e_eval", id, code });
    }));
    window.addEventListener("message", onMessage);
    return () => { window.removeEventListener("message", onMessage); sandboxes.delete(appId); };
  }, [app.id]);

  // Event forwarding for runs of this app
  useEffect(() => {
    const appId = app.id;
    let disposed = false;
    const un: UnlistenFn[] = [];
    const belongs = (payload: { runId?: string; appId?: string; id?: string }) => {
      if (payload.appId) return payload.appId === appId;
      const rid = payload.runId ?? payload.id;
      return !!rid && useStore.getState().runs[rid]?.appId === appId;
    };
    for (const [ev, name] of forwarded) {
      on<{ runId?: string; appId?: string; id?: string }>(ev, (payload) => {
        if (!belongs(payload)) return;
        const out = name === "status" ? { runId: (payload as RunRecord).id, ...payload } : payload;
        post({ type: "event", name, payload: out });
      }).then((u) => (disposed ? u() : un.push(u)));
    }
    on<{ appId: string }>(events.appUiReload, (p) => p.appId === appId && setReloadKey((k) => k + 1)).then((u) => (disposed ? u() : un.push(u)));
    return () => { disposed = true; un.forEach((u) => u()); };
  }, [app.id]);

  const running = useStore((s) => isActive(latestRun(s.runs, app.id)?.status));
  return (
    <div ref={dropRef} className="relative h-full w-full bg-white">
      <iframe key={reloadKey} ref={frame} data-testid="app-sandbox" data-bridge={bridgeReady ? "ready" : "pending"} title={app.name} src={appUiUrl(app.id)} sandbox="allow-scripts allow-forms allow-modals" className="h-full w-full border-0" />
      {running && <div className="pointer-events-none absolute right-2 top-2 h-2 w-2 animate-pulse rounded-full bg-ok" />}
    </div>
  );
}

function safeParse(s: string): unknown { try { return JSON.parse(s); } catch { return {}; } }
