// One app's workspace: header (run/stop, CLI copy, presets, agent toggle), git bar, form or custom UI, logs, outputs.
import { useEffect, useMemo, useState } from "react";
import type { DockviewApi, DockviewPanelApi } from "dockview";
import { ipc } from "../lib/ipc";
import type { PresetRecord } from "../lib/types";
import { useStore, latestRun } from "../state/store";
import { errMsg, fmtMb, isActive } from "../lib/format";
import { defaultValues, validate } from "../lib/form";
import { Badge, Btn, Dropdown, MenuItem, PromptModal } from "./ui";
import AutoForm from "./AutoForm";
import AppSandbox from "./AppSandbox";
import GitBar from "./GitBar";
import LogConsole from "./LogConsole";
import OutputShelf from "./OutputShelf";

const agentId = (appId: string) => `agent:${appId}`;

export default function AppPanel({ appId, api, containerApi }: { appId: string; api: DockviewPanelApi; containerApi: DockviewApi }) {
  const app = useStore((s) => s.apps.find((a) => a.id === appId));
  const run = useStore((s) => latestRun(s.runs, appId));
  const queued = useStore((s) => s.resources?.queue.find((q) => q.runId === run?.id));
  const notify = useStore((s) => s.notify);
  const [values, setValues] = useState<Record<string, unknown>>({});
  const [touched, setTouched] = useState(false);
  const [presets, setPresets] = useState<PresetRecord[]>([]);
  const [savingPreset, setSavingPreset] = useState(false);
  const [agentOpen, setAgentOpen] = useState(() => !!containerApi.getPanel(agentId(appId)));
  const [starting, setStarting] = useState(false);

  const manifest = app?.manifest;
  useEffect(() => { if (manifest) setValues((v) => ({ ...defaultValues(manifest), ...v })); }, [manifest]);
  useEffect(() => { if (app) api.setTitle(app.name); }, [app?.name]);
  const loadPresets = () => ipc.listPresets(appId).then(setPresets).catch(() => {});
  useEffect(() => { loadPresets(); }, [appId]);
  useEffect(() => {
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
    const p = containerApi.getPanel(agentId(appId));
    if (p) { p.api.close(); setAgentOpen(false); return; }
    containerApi.addPanel({ id: agentId(appId), component: "agent", title: `Agent · ${app.name}`, params: { appId }, position: { direction: "left", referencePanel: api.id } });
    setAgentOpen(true);
  };
  const loadPreset = (p: PresetRecord) => {
    try { setValues({ ...defaultValues(manifest), ...JSON.parse(p.inputsJson) }); } catch { notify("Preset is corrupted"); }
  };

  return (
    <div className="flex h-full flex-col bg-panel" onKeyDown={(e) => { if ((e.metaKey || e.ctrlKey) && e.key === "Enter") { e.preventDefault(); active ? stop() : start(); } }}>
      <header className="flex flex-wrap items-center gap-2 border-b border-line px-3 py-2">
        <h1 className="mr-1 truncate text-[14px] font-semibold" title={app.rootDir}>{app.name}</h1>
        <Badge color="accent">{app.executionMode.replace("_", " ")}</Badge>
        {manifest.resources.requires_gpu && <Badge color="amber">GPU</Badge>}
        <div className="flex-1" />
        <Dropdown label="Presets" align="right">
          {(close) => (<>
            {presets.map((p) => (
              <div key={p.id} className="flex items-center hover:bg-white/5">
                <button className="flex-1 truncate px-3 py-1 text-left" onClick={() => { close(); loadPreset(p); }}>{p.name}</button>
                <button className="px-2 text-gray-600 hover:text-red-400" title="Delete preset" onClick={() => ipc.deletePreset(p.id).then(loadPresets)}>✕</button>
              </div>
            ))}
            {!presets.length && <div className="px-3 py-1 text-gray-500">No presets yet</div>}
            <div className="my-1 border-t border-line" />
            <MenuItem onClick={() => { close(); setSavingPreset(true); }}>Save preset…</MenuItem>
          </>)}
        </Dropdown>
        <Btn onClick={copyCli} title="Copy the equivalent shell command">Copy as CLI</Btn>
        <Btn onClick={toggleAgent} className={agentOpen ? "!border-accent text-accent" : ""}>Edit with Agent</Btn>
        {active
          ? <Btn variant="danger" onClick={stop} disabled={run?.status === "stopping"}>{run?.status === "stopping" ? "Stopping…" : "■ Stop"}</Btn>
          : <Btn variant="primary" onClick={start} disabled={starting} title="⌘↵">▶ Run</Btn>}
      </header>
      <div className="flex items-center gap-3 border-b border-line px-3 py-1">
        <GitBar appId={appId} refreshKey={app.manifestHash} />
        <div className="flex-1" />
        {run && <RunStatus status={run.status} exitCode={run.exitCode} />}
      </div>
      {run?.status === "queued_vram" && (
        <div className="flex items-center gap-3 border-b border-amber-500/30 bg-amber-500/10 px-3 py-1.5 text-amber-300">
          <span>Waiting for {fmtMb(queued?.requiredVramMb ?? manifest.resources.vram_estimate_mb ?? 0)} GPU memory{queued?.reason ? ` — ${queued.reason}` : ""}</span>
          <div className="flex-1" />
          <Btn onClick={() => ipc.forceRun(run.id).catch((e) => notify(errMsg(e)))}>Run anyway</Btn>
        </div>
      )}
      <div className="flex min-h-0 flex-1 flex-col">
        {app.hasCustomUi
          ? <div className="min-h-0 flex-1"><AppSandbox app={app} /></div>
          : <div className="min-h-0 flex-1 overflow-y-auto"><AutoForm manifest={manifest} values={values} onChange={setValues} errors={touched ? errors : {}} /></div>}
        <LogConsole runId={run?.id ?? null} />
        <OutputShelf appId={appId} runId={run?.id ?? null} />
      </div>
      {savingPreset && <PromptModal title="Save preset" label="Preset name" onClose={() => setSavingPreset(false)}
        onOk={(n) => ipc.savePreset(appId, n, values).then(loadPresets).catch((e) => notify(errMsg(e)))} />}
    </div>
  );
}

function RunStatus({ status, exitCode }: { status: string; exitCode: number | null }) {
  const color = { running: "green", stopping: "amber", queued_vram: "amber", success: "accent", failed: "red", killed: "gray" }[status] as "green";
  const label = status === "queued_vram" ? "queued" : status;
  return <Badge color={color}>{label}{exitCode != null && status !== "success" ? ` · exit ${exitCode}` : ""}</Badge>;
}
