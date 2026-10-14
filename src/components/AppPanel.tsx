// One app's workspace: header (run/stop, presets, Edit with Agent, ⋯ menu), queue banner, form / custom UI / webapp, Underbar.
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { DockviewApi, DockviewPanelApi } from "dockview";
import { ipc } from "../lib/ipc";
import type { PresetRecord } from "../lib/types";
import { useStore, latestRun } from "../state/store";
import { useLive } from "../state/live";
import { errMsg, fmtMb, isActive } from "../lib/format";
import { defaultValues, validate } from "../lib/form";
import { Badge, Btn, Dropdown, MenuItem, Modal, PromptModal } from "./ui";
import AutoForm from "./AutoForm";
import AppSandbox from "./AppSandbox";
import WebappFrame from "./WebappFrame";
import Underbar from "./Underbar";
import { openAppWindow, dockAppWindow } from "../lib/appWindow";

const isMac = navigator.platform.toLowerCase().includes("mac");
const agentId = (appId: string) => `agent:${appId}`;

/** `api`/`containerApi` are absent when rendered standalone in its own window. `solo` = dedicated window: just the program
 * (web UI, custom UI or form), no header, tabs or Underbar — it should feel like one program running on its own. */
export default function AppPanel({ appId, api, containerApi, solo }: { appId: string; api?: DockviewPanelApi; containerApi?: DockviewApi; solo?: boolean }) {
  const app = useStore((s) => s.apps.find((a) => a.id === appId));
  const run = useStore((s) => latestRun(s.runs, appId));
  const queuedTick = useStore((s) => s.resources?.queue.find((q) => q.runId === run?.id));
  const queuedEv = useStore((s) => (run ? s.queued[run.id] : undefined));
  const queued = queuedEv ?? queuedTick;
  const port = useStore((s) => (run ? s.ports[run.id] : undefined));
  const webappReady = useStore((s) => (run ? !!s.webappReady[run.id] : false));
  const notify = useStore((s) => s.notify);
  const [values, setValues] = useState<Record<string, unknown>>({});
  const [touched, setTouched] = useState(false);
  const [presets, setPresets] = useState<PresetRecord[]>([]);
  const [savingPreset, setSavingPreset] = useState(false);
  const [agentOpen, setAgentOpen] = useState(() => !!containerApi?.getPanel(agentId(appId)));
  const [viewRunId, setViewRunId] = useState<string | null>(null);
  const [removing, setRemoving] = useState(false);
  const [starting, setStarting] = useState(false);

  const manifest = app?.manifest;
  useEffect(() => { if (manifest) setValues((v) => ({ ...defaultValues(manifest), ...v })); }, [manifest]);
  useEffect(() => { if (app) api?.setTitle(app.name); }, [app?.name]);
  useEffect(() => { setViewRunId(null); }, [run?.id]); // a new run takes over the console
  const loadPresets = () => ipc.listPresets(appId).then(setPresets).catch(() => {});
  useEffect(() => { loadPresets(); }, [appId]);
  useEffect(() => {
    if (!containerApi) return;
    const d = containerApi.onDidRemovePanel((p) => p.id === agentId(appId) && setAgentOpen(false));
    return () => d.dispose();
  }, [containerApi, appId]);
  useEffect(() => { // seed history: last run of this app
    if (run) return;
    ipc.listRuns(appId, 1).then((rs) => rs[0] && useStore.getState().upsertRun(rs[0])).catch(() => {});
  }, [appId]);

  const errors = useMemo(() => (manifest ? validate(manifest, values) : {}), [manifest, values]);
  const autoStarted = useRef(false);
  useEffect(() => { // solo window of a web app: the window *is* the program, so bring the server up without asking
    if (!solo || !app || app.executionMode !== "webapp" || autoStarted.current || isActive(run?.status)) return;
    autoStarted.current = true;
    ipc.startRun(appId, values).then((r) => useStore.getState().upsertRun(r)).catch((e) => notify(`Run failed: ${errMsg(e)}`));
  }, [solo, app?.id, app?.executionMode, run?.status]);
  if (!app || !manifest) return <div className="p-4 text-muted">Loading app…</div>;

  const active = isActive(run?.status);
  const start = async () => {
    setTouched(true);
    if (Object.keys(errors).length) return;
    setStarting(true);
    try { useStore.getState().upsertRun(await ipc.startRun(appId, values)); } catch (e) { notify(`Run failed: ${errMsg(e)}`); }
    setStarting(false);
  };
  const stop = () => run && ipc.stopRun(run.id).catch((e) => notify(errMsg(e)));
  const copyCli = async () => {
    try { await navigator.clipboard.writeText(await ipc.buildCliCommand(appId, values)); notify("CLI command copied"); } catch (e) { notify(errMsg(e)); }
  };
  const toggleAgent = () => {
    if (!containerApi || !api) return;
    const p = containerApi.getPanel(agentId(appId));
    if (p) { p.api.close(); setAgentOpen(false); return; }
    containerApi.addPanel({ id: agentId(appId), component: "agent", title: `Agent · ${app.name}`, params: { appId }, position: { direction: "left", referencePanel: api.id } });
    setAgentOpen(true);
  };
  const loadPreset = (p: PresetRecord) => {
    try { setValues({ ...defaultValues(manifest), ...JSON.parse(p.inputsJson) }); } catch { notify("Preset is corrupted"); }
  };

  return (
    <div data-app-panel={appId} data-solo={solo ? "1" : undefined} tabIndex={-1} className="flex h-full flex-col bg-panel outline-none" onKeyDown={(e) => {
      if ((e.metaKey || e.ctrlKey) && e.key === "Enter") { e.preventDefault(); active ? stop() : start(); }
      if (solo && (e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === "d") { e.preventDefault(); dockAppWindow(appId); }
    }}>
      {!solo && <header className="flex flex-wrap items-center gap-2 border-b border-line px-3 py-2">
        <h1 className="mr-1 truncate text-[14px] font-semibold" title={app.rootDir}>{app.name}</h1>
        <Badge color="accent">{app.executionMode.replace("_", " ")}</Badge>
        {manifest.resources.requires_gpu && <Badge color="amber">GPU</Badge>}
        {manifest.resources.max_ram_mb != null && (
          <span data-testid="ram-limit"><Badge title={isMac ? "max_ram_mb is advisory on macOS (not enforced)" : "Enforced via RLIMIT_AS"}>RAM ≤ {fmtMb(manifest.resources.max_ram_mb)}{isMac ? " (advisory)" : ""}</Badge></span>
        )}
        <div className="flex-1" />
        <Dropdown testid="preset-select" label="Presets" align="right">
          {(close) => (<>
            {presets.map((p) => (
              <div key={p.id} className="flex items-center hover:bg-panel-2">
                <button data-testid="preset-item" className="flex-1 truncate px-3 py-1 text-left" onClick={() => { close(); loadPreset(p); }}>{p.name}</button>
                <button className="px-2 text-faint hover:text-danger" title="Delete preset" onClick={() => ipc.deletePreset(p.id).then(loadPresets)}>✕</button>
              </div>
            ))}
            {!presets.length && <div className="px-3 py-1 text-muted">No presets yet</div>}
            <div className="my-1 border-t border-line" />
            <MenuItem testid="preset-save" onClick={() => { close(); setSavingPreset(true); }}>Save preset…</MenuItem>
          </>)}
        </Dropdown>
        {containerApi && <Btn data-testid="open-in-window" onClick={() => { openAppWindow(app); useStore.getState().closeApp(app.id); }} title="Open in its own window: just the program, no panels. Closing that window exits the program; ⌘⇧D there brings it back here. Tip: drag this tab out of the dock to do the same.">⧉ Window</Btn>}
        {containerApi && <Btn data-testid="edit-with-agent" onClick={toggleAgent} className={agentOpen ? "!border-accent text-accent" : ""} title="Open a coding agent in this app's folder">Edit with Agent</Btn>}
        {active
          ? <Btn data-testid="stop-button" variant="danger" onClick={stop} disabled={run?.status === "stopping"}>{run?.status === "stopping" ? "Stopping…" : "■ Stop"}</Btn>
          : <Btn data-testid="run-button" variant="primary" onClick={start} disabled={starting} title="⌘↵">▶ Run</Btn>}
        <MoreMenu>
          {(close) => (<>
            <MenuEntry testid="copy-cli" close={close} onClick={copyCli} title="Copy the equivalent shell command">Copy as CLI</MenuEntry>
            <MenuEntry testid="draft-manifest" close={close} onClick={() => useStore.getState().setDraftPath(app.rootDir)} title="Draft a homemade.toml with the local model">✨ Draft manifest</MenuEntry>
            {containerApi && <MenuEntry testid="more-edit-with-agent" close={close} onClick={toggleAgent}>{agentOpen ? "Close agent" : "Edit with Agent"}</MenuEntry>}
            <MenuEntry testid="reveal-app" close={close} onClick={() => ipc.revealInFinder(app.rootDir).catch((e) => notify(errMsg(e)))}>Reveal in Finder</MenuEntry>
            <div className="my-1 border-t border-line" />
            <MenuEntry testid="remove-app" close={close} danger onClick={() => setRemoving(true)} title="Remove app from Homemade">Remove…</MenuEntry>
          </>)}
        </MoreMenu>
      </header>}
      {run?.status === "queued_vram" && (
        <div data-testid="queue-banner" className="flex items-center gap-3 border-b border-warn/30 bg-warn/10 px-3 py-1.5 text-warn">
          <span>Waiting for {fmtMb(queued?.requiredVramMb ?? manifest.resources.vram_estimate_mb ?? 0)} GPU memory{queued?.reason ? ` — ${queued.reason}` : ""}</span>
          <div className="flex-1" />
          <Btn data-testid="queue-run-anyway" onClick={() => ipc.forceRun(run.id).catch((e) => notify(errMsg(e)))}>Run anyway</Btn>
        </div>
      )}
      <div className="flex min-h-0 flex-1 flex-col">
        {app.executionMode === "webapp"
          ? <div className="min-h-0 flex-1"><WebappFrame app={app} run={run ?? undefined} port={active ? port : undefined} ready={webappReady} chrome={!solo} /></div>
          : app.hasCustomUi
          ? <div className="min-h-0 flex-1"><AppSandbox app={app} /></div>
          : <div className="min-h-0 flex-1 overflow-y-auto"><AutoForm manifest={manifest} values={values} onChange={setValues} errors={touched ? errors : {}} /></div>}
      </div>
      {solo
        ? (app.executionMode !== "webapp" && !app.hasCustomUi && (
          <footer data-testid="solo-footer" className="flex items-center gap-3 border-t border-line px-3 py-2">
            <SoloStatus run={run} />
            <div className="flex-1" />
            {active
              ? <Btn data-testid="stop-button" variant="danger" onClick={stop} disabled={run?.status === "stopping"}>{run?.status === "stopping" ? "Stopping…" : "■ Stop"}</Btn>
              : <Btn data-testid="run-button" variant="primary" onClick={start} disabled={starting} title="⌘↵">▶ Run</Btn>}
          </footer>))
        : <Underbar app={app} latest={run} viewRunId={viewRunId} onViewRun={setViewRunId} values={values} />}
      {savingPreset && <PromptModal title="Save preset" label="Preset name" onClose={() => setSavingPreset(false)}
        onOk={(n) => ipc.savePreset(appId, n, values).then(loadPresets).catch((e) => notify(errMsg(e)))} />}
      {removing && <RemoveAppModal appId={appId} name={app.name} onClose={() => setRemoving(false)} />}
    </div>
  );
}

/** One line of state for a solo form window: progress while running, exit/artifact summary after. */
function SoloStatus({ run }: { run?: ReturnType<typeof latestRun> }) {
  const prog = useLive((s) => (run ? s.runs[run.id]?.progress : undefined));
  const arts = useStore((s) => (run ? s.artifactTick[run.appId] : 0));
  const [count, setCount] = useState(0);
  useEffect(() => { if (run && !isActive(run.status)) ipc.listArtifacts(run.appId, run.id).then((a) => setCount(a.length)).catch(() => {}); }, [run?.id, run?.status, arts]);
  if (!run) return <span className="text-muted">Fill in the form and press Run</span>;
  if (isActive(run.status)) return (
    <div className="flex min-w-0 flex-1 items-center gap-2">
      <div className="h-1.5 w-40 overflow-hidden rounded bg-panel-2"><div className="h-full bg-accent transition-all" style={{ width: `${prog?.percent ?? 5}%` }} /></div>
      <span className="truncate text-muted">{prog?.message ?? "Running…"}</span>
    </div>);
  return (
    <div className="flex min-w-0 items-center gap-2">
      <Badge color={run.status === "success" ? "green" : "red"}>{run.status}</Badge>
      {count > 0 && <button className="text-accent hover:underline" onClick={() => { ipc.listArtifacts(run.appId, run.id).then((a) => a[0] && ipc.revealInFinder(a[0].filePath)).catch(() => {}); }}>{count} output{count > 1 ? "s" : ""} · Reveal</button>}
    </div>);
}

function RemoveAppModal({ appId, name, onClose }: { appId: string; name: string; onClose: () => void }) {
  const [purge, setPurge] = useState(false);
  const [venv, setVenv] = useState(false);
  const [busy, setBusy] = useState(false);
  const go = async () => {
    setBusy(true);
    const st = useStore.getState();
    try {
      await ipc.removeApp(appId, purge, venv);
      onClose();
      st.removeApp(appId);
      st.notify(`Removed ${name}`);
    } catch (e) { st.notify(`Remove failed: ${errMsg(e)}`); setBusy(false); }
  };
  return (
    <Modal testid="remove-app-modal" title={`Remove ${name}?`} onClose={onClose}>
      <p className="mb-3 text-fg">The app folder itself is never deleted. Its run history, presets and artifact records are removed from Homemade.</p>
      <label className="mb-1 flex items-center gap-2"><input data-testid="remove-purge-scratch" type="checkbox" checked={purge} onChange={(e) => setPurge(e.target.checked)} /> Also delete scratch files</label>
      <label className="flex items-center gap-2"><input data-testid="remove-delete-venv" type="checkbox" checked={venv} onChange={(e) => setVenv(e.target.checked)} /> Also delete the app's virtualenv</label>
      <div className="mt-4 flex justify-end gap-2">
        <Btn onClick={onClose}>Cancel</Btn>
        <Btn data-testid="remove-app-confirm" variant="danger" disabled={busy} onClick={go}>{busy ? "Removing…" : "Remove"}</Btn>
      </div>
    </Modal>
  );
}

/** Header `⋯` menu. Items stay mounted (hidden) so their testids are always reachable. */
function MoreMenu({ children }: { children: (close: () => void) => ReactNode }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const h = (e: MouseEvent) => ref.current && !ref.current.contains(e.target as Node) && setOpen(false);
    document.addEventListener("mousedown", h);
    return () => document.removeEventListener("mousedown", h);
  }, [open]);
  return (
    <div ref={ref} className="relative">
      <Btn data-testid="app-more" variant="ghost" aria-label="More actions" title="More actions" aria-expanded={open} onClick={() => setOpen(!open)}>⋯</Btn>
      <div data-testid="app-more-menu" hidden={!open} className="absolute right-0 top-full z-40 mt-1 min-w-48 rounded border border-line bg-panel py-1 shadow-xl">
        {children(() => setOpen(false))}
      </div>
    </div>
  );
}

function MenuEntry({ testid, onClick, close, children, title, danger }: { testid: string; onClick: () => void; close: () => void; children: ReactNode; title?: string; danger?: boolean }) {
  return <button data-testid={testid} title={title} onClick={() => { close(); onClick(); }}
    className={`flex w-full items-center gap-2 px-3 py-1 text-left hover:bg-panel-2 ${danger ? "text-danger" : ""}`}>{children}</button>;
}
