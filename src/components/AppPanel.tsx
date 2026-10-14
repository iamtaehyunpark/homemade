// One app's workspace: header (run/stop, CLI copy, presets, agent toggle), git bar, form or custom UI, logs, outputs.
import { useEffect, useMemo, useState } from "react";
import type { DockviewApi, DockviewPanelApi } from "dockview";
import { ipc } from "../lib/ipc";
import type { PresetRecord, RunRecord } from "../lib/types";
import { useStore, latestRun } from "../state/store";
import { errMsg, fmtMb, isActive, relTime } from "../lib/format";
import { defaultValues, validate } from "../lib/form";
import { Badge, Btn, Dropdown, MenuItem, Modal, PromptModal } from "./ui";
import AutoForm from "./AutoForm";
import AppSandbox from "./AppSandbox";
import GitBar from "./GitBar";
import LogConsole from "./LogConsole";
import OutputShelf from "./OutputShelf";

const agentId = (appId: string) => `agent:${appId}`;

/** `api`/`containerApi` are absent when rendered standalone in its own window (default_layout = "window"). */
export default function AppPanel({ appId, api, containerApi }: { appId: string; api?: DockviewPanelApi; containerApi?: DockviewApi }) {
  const app = useStore((s) => s.apps.find((a) => a.id === appId));
  const run = useStore((s) => latestRun(s.runs, appId));
  const queuedTick = useStore((s) => s.resources?.queue.find((q) => q.runId === run?.id));
  const queuedEv = useStore((s) => (run ? s.queued[run.id] : undefined));
  const queued = queuedEv ?? queuedTick;
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
  if (!app || !manifest) return <div className="p-4 text-gray-500">Loading app…</div>;

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
    <div data-app-panel={appId} className="flex h-full flex-col bg-panel" onKeyDown={(e) => { if ((e.metaKey || e.ctrlKey) && e.key === "Enter") { e.preventDefault(); active ? stop() : start(); } }}>
      <header className="flex flex-wrap items-center gap-2 border-b border-line px-3 py-2">
        <h1 className="mr-1 truncate text-[14px] font-semibold" title={app.rootDir}>{app.name}</h1>
        <Badge color="accent">{app.executionMode.replace("_", " ")}</Badge>
        {manifest.resources.requires_gpu && <Badge color="amber">GPU</Badge>}
        <div className="flex-1" />
        <Dropdown testid="preset-select" label="Presets" align="right">
          {(close) => (<>
            {presets.map((p) => (
              <div key={p.id} className="flex items-center hover:bg-white/5">
                <button data-testid="preset-item" className="flex-1 truncate px-3 py-1 text-left" onClick={() => { close(); loadPreset(p); }}>{p.name}</button>
                <button className="px-2 text-gray-600 hover:text-red-400" title="Delete preset" onClick={() => ipc.deletePreset(p.id).then(loadPresets)}>✕</button>
              </div>
            ))}
            {!presets.length && <div className="px-3 py-1 text-gray-500">No presets yet</div>}
            <div className="my-1 border-t border-line" />
            <MenuItem testid="preset-save" onClick={() => { close(); setSavingPreset(true); }}>Save preset…</MenuItem>
          </>)}
        </Dropdown>
        <Btn data-testid="copy-cli" onClick={copyCli} title="Copy the equivalent shell command">Copy as CLI</Btn>
        {containerApi && <Btn data-testid="edit-with-agent" onClick={toggleAgent} className={agentOpen ? "!border-accent text-accent" : ""}>Edit with Agent</Btn>}
        <Btn data-testid="remove-app" variant="ghost" title="Remove app from Homemade" onClick={() => setRemoving(true)}>Remove</Btn>
        {active
          ? <Btn data-testid="stop-button" variant="danger" onClick={stop} disabled={run?.status === "stopping"}>{run?.status === "stopping" ? "Stopping…" : "■ Stop"}</Btn>
          : <Btn data-testid="run-button" variant="primary" onClick={start} disabled={starting} title="⌘↵">▶ Run</Btn>}
      </header>
      <div className="flex items-center gap-3 border-b border-line px-3 py-1">
        <GitBar appId={appId} refreshKey={app.manifestHash} />
        <div className="flex-1" />
        {run && <RunStatus status={run.status} exitCode={run.exitCode} />}
      </div>
      {run?.status === "queued_vram" && (
        <div data-testid="queue-banner" className="flex items-center gap-3 border-b border-amber-500/30 bg-amber-500/10 px-3 py-1.5 text-amber-300">
          <span>Waiting for {fmtMb(queued?.requiredVramMb ?? manifest.resources.vram_estimate_mb ?? 0)} GPU memory{queued?.reason ? ` — ${queued.reason}` : ""}</span>
          <div className="flex-1" />
          <Btn data-testid="queue-run-anyway" onClick={() => ipc.forceRun(run.id).catch((e) => notify(errMsg(e)))}>Run anyway</Btn>
        </div>
      )}
      <div className="flex min-h-0 flex-1 flex-col">
        {app.hasCustomUi
          ? <div className="min-h-0 flex-1"><AppSandbox app={app} /></div>
          : <div className="min-h-0 flex-1 overflow-y-auto"><AutoForm manifest={manifest} values={values} onChange={setValues} errors={touched ? errors : {}} /></div>}
        <RunHistory appId={appId} latest={run} selected={viewRunId ?? run?.id ?? null} onSelect={(r) => setViewRunId(r.id === run?.id ? null : r.id)} />
        <LogConsole runId={viewRunId ?? run?.id ?? null} />
        <OutputShelf appId={appId} runId={viewRunId ?? run?.id ?? null} />
      </div>
      {savingPreset && <PromptModal title="Save preset" label="Preset name" onClose={() => setSavingPreset(false)}
        onOk={(n) => ipc.savePreset(appId, n, values).then(loadPresets).catch((e) => notify(errMsg(e)))} />}
      {removing && <RemoveAppModal appId={appId} name={app.name} onClose={() => setRemoving(false)} />}
    </div>
  );
}

function RunStatus({ status, exitCode }: { status: string; exitCode: number | null }) {
  const color = { running: "green", stopping: "amber", queued_vram: "amber", success: "accent", failed: "red", killed: "gray" }[status] as "green";
  const label = status === "queued_vram" ? "queued" : status;
  return <Badge color={color}>{label}{exitCode != null && status !== "success" ? ` · exit ${exitCode}` : ""}</Badge>;
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
      <p className="mb-3 text-gray-300">The app folder itself is never deleted. Its run history, presets and artifact records are removed from Homemade.</p>
      <label className="mb-1 flex items-center gap-2"><input data-testid="remove-purge-scratch" type="checkbox" checked={purge} onChange={(e) => setPurge(e.target.checked)} /> Also delete scratch files</label>
      <label className="flex items-center gap-2"><input data-testid="remove-delete-venv" type="checkbox" checked={venv} onChange={(e) => setVenv(e.target.checked)} /> Also delete the app's virtualenv</label>
      <div className="mt-4 flex justify-end gap-2">
        <Btn onClick={onClose}>Cancel</Btn>
        <Btn data-testid="remove-app-confirm" variant="danger" disabled={busy} onClick={go}>{busy ? "Removing…" : "Remove"}</Btn>
      </div>
    </Modal>
  );
}

const statusColor: Record<string, string> = {
  running: "bg-emerald-500/15 text-emerald-400", stopping: "bg-amber-500/15 text-amber-400", queued_vram: "bg-amber-500/15 text-amber-400",
  success: "bg-accent/15 text-accent", failed: "bg-red-500/15 text-red-400", killed: "bg-white/5 text-gray-400",
};

function RunHistory({ appId, latest, selected, onSelect }: { appId: string; latest: RunRecord | null; selected: string | null; onSelect: (r: RunRecord) => void }) {
  const [runs, setRuns] = useState<RunRecord[]>([]);
  const [open, setOpen] = useState(true);
  useEffect(() => {
    ipc.listRuns(appId, 30).then(setRuns).catch(() => setRuns([]));
  }, [appId, latest?.id, latest?.status]);
  if (!runs.length) return null;
  return (
    <div className="shrink-0 border-t border-line bg-panel">
      <button className="px-3 py-1 text-[11px] font-semibold uppercase tracking-wider text-gray-500 hover:text-gray-300" onClick={() => setOpen(!open)}>
        {open ? "▾" : "▸"} Runs <span className="font-normal normal-case text-gray-600">({runs.length})</span>
      </button>
      {open && (
        <div data-testid="run-history" className="flex max-h-24 flex-col overflow-y-auto px-1.5 pb-1">
          {runs.map((r) => (
            <button key={r.id} data-testid="run-history-item" data-run-id={r.id} data-status={r.status} onClick={() => onSelect(r)}
              className={`flex items-center gap-2 rounded px-1.5 py-0.5 text-left text-[11.5px] ${selected === r.id ? "bg-accent/10" : "hover:bg-white/5"}`}>
              <span className={`w-16 shrink-0 rounded px-1 text-center text-[10px] font-medium uppercase ${statusColor[r.status] ?? statusColor.killed}`}>{r.status === "queued_vram" ? "queued" : r.status}</span>
              <span className="w-20 shrink-0 text-gray-400">{relTime(r.startedAt)}</span>
              <span className="w-14 shrink-0 font-mono text-gray-500">{r.exitCode != null ? `exit ${r.exitCode}` : ""}</span>
              <span className="min-w-0 flex-1 truncate font-mono text-gray-600" title={r.cliCommand}>{r.cliCommand}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
