// One app's workspace: header (run/stop, presets, Edit with Agent, ⋯ menu), queue banner, form / custom UI / webapp, Underbar.
import { workspaces } from "../lib/registry";
import { useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import type { DockviewApi, DockviewPanelApi } from "dockview";
import { ipc } from "../lib/ipc";
import type { DeletePlan, PlanItem, PresetRecord } from "../lib/types";
import { useStore, latestRun } from "../state/store";
import { useLive } from "../state/live";
import { errMsg, fmtBytes, fmtMb, isActive } from "../lib/format";
import { defaultValues, validate } from "../lib/form";
import { Badge, Btn, Dropdown, MenuItem, Modal, PromptModal } from "./ui";
import AutoForm from "./AutoForm";
import AppSandbox from "./AppSandbox";
import WebappFrame from "./WebappFrame";
import Underbar from "./Underbar";
import { openAppWindow, dockAppWindow } from "../lib/appWindow";

const isMac = navigator.platform.toLowerCase().includes("mac");

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
  const [viewRunId, setViewRunId] = useState<string | null>(null);
  const [starting, setStarting] = useState(false);

  const manifest = app?.manifest;
  useEffect(() => { if (manifest) setValues((v) => ({ ...defaultValues(manifest), ...v })); }, [manifest]);
  useEffect(() => { if (app) api?.setTitle(app.name); }, [app?.name]);
  useEffect(() => { setViewRunId(null); }, [run?.id]); // a new run takes over the console
  const loadPresets = () => ipc.listPresets(appId).then(setPresets).catch(() => {});
  useEffect(() => { loadPresets(); }, [appId]);
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
    if (!manifest.app.exec.trim()) { notify("Set a run command in Manifest before running this project."); return; }
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
  const addShell = () => workspaces.get(appId)?.addShell();
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
        {containerApi && <Btn data-testid="edit-with-agent" onClick={addShell} title="Open a shell in this app's folder (⌘T); start an agent from there or via the selector">Edit with Agent</Btn>}
        {active
          ? <Btn data-testid="stop-button" variant="danger" onClick={stop} disabled={run?.status === "stopping"}>{run?.status === "stopping" ? "Stopping…" : "■ Stop"}</Btn>
          : <Btn data-testid="run-button" variant="primary" onClick={start} disabled={starting || !manifest.app.exec.trim()} title={manifest.app.exec.trim() ? "⌘↵" : "Set a run command in Manifest first"}>▶ Run</Btn>}
        <MoreMenu>
          {(close) => (<>
            <MenuEntry testid="copy-cli" close={close} onClick={copyCli} title="Copy the equivalent shell command">Copy as CLI</MenuEntry>
            <MenuEntry testid="draft-manifest" close={close} onClick={() => useStore.getState().setDraftPath(app.rootDir)} title="Draft a homemade.toml with the local model">✨ Draft manifest</MenuEntry>
            {containerApi && <MenuEntry testid="more-edit-with-agent" close={close} onClick={addShell}>New shell</MenuEntry>}
            <MenuEntry testid="reveal-app" close={close} onClick={() => ipc.revealInFinder(app.rootDir).catch((e) => notify(errMsg(e)))}>Reveal in Finder</MenuEntry>
            <div className="my-1 border-t border-line" />
            <MenuEntry testid="remove-app" close={close} danger onClick={() => useStore.getState().setRemoveAppId(appId)} title="Hide from Homemade or delete the program">Remove…</MenuEntry>
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
      {!manifest.app.exec.trim() && <div data-testid="project-unconfigured" className="border-b border-line bg-bg px-4 py-3 text-[12px] leading-5 text-muted">Build your project in the shell. When it’s ready, set <code className="text-fg">app.exec</code> in the Manifest tab to enable Run.</div>}
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
              : <Btn data-testid="run-button" variant="primary" onClick={start} disabled={starting || !manifest.app.exec.trim()} title={manifest.app.exec.trim() ? "⌘↵" : "Set a run command in Manifest first"}>▶ Run</Btn>}
          </footer>))
        : <Underbar app={app} latest={run} viewRunId={viewRunId} onViewRun={setViewRunId} values={values} />}
      {savingPreset && <PromptModal title="Save preset" label="Preset name" onClose={() => setSavingPreset(false)}
        onOk={(n) => ipc.savePreset(appId, n, values).then(loadPresets).catch((e) => notify(errMsg(e)))} />}
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

/** Two levels: "Hide" unregisters and keeps everything on disk; "Delete program…" fetches a plan from the backend,
 * lets the user tick what goes, then trashes the folder and frees what only this program uses. */
export function RemoveAppModal({ appId, name, onClose }: { appId: string; name: string; onClose: () => void }) {
  const [step, setStep] = useState<"choose" | "delete">("choose");
  const [busy, setBusy] = useState(false);
  const hide = async () => {
    setBusy(true);
    const st = useStore.getState();
    try {
      await ipc.hideApp(appId);
      onClose();
      st.removeApp(appId);
      st.notify(`Hidden ${name}`);
    } catch (e) { st.notify(`Hide failed: ${errMsg(e)}`); setBusy(false); }
  };
  if (step === "delete") return <DeletePlanModal appId={appId} name={name} onClose={onClose} onBack={() => setStep("choose")} />;
  const card = "flex w-full flex-col items-start gap-0.5 rounded border border-line px-3 py-2 text-left hover:bg-panel-2 disabled:opacity-50";
  return (
    <Modal testid="remove-app-modal" title={`Remove ${name}`} onClose={onClose}>
      <div className="flex flex-col gap-2">
        <button data-testid="remove-app-confirm" className={card} disabled={busy} onClick={hide}>
          <span className="font-medium text-fg">{busy ? "Hiding…" : "Hide from Homemade"}</span>
          <span className="text-xs text-muted">Unregister it. The folder, environment and outputs stay on disk.</span>
        </button>
        <button data-testid="delete-app-open" className={card} disabled={busy} onClick={() => setStep("delete")}>
          <span className="font-medium text-danger">Delete program…</span>
          <span className="text-xs text-muted">Move the folder to the Trash and free what only this program uses. You choose what goes.</span>
        </button>
      </div>
      <div className="mt-4 flex justify-end gap-2">
        <Btn onClick={onClose}>Cancel</Btn>
      </div>
    </Modal>
  );
}

function DeletePlanModal({ appId, name, onClose, onBack }: { appId: string; name: string; onClose: () => void; onBack: () => void }) {
  const [plan, setPlan] = useState<DeletePlan | null>(null);
  const [loadErr, setLoadErr] = useState<string | null>(null);
  const [checked, setChecked] = useState<Record<string, boolean>>({});
  const [busy, setBusy] = useState(false);
  useEffect(() => {
    let live = true;
    ipc.deletePlan(appId)
      .then((p) => { if (!live) return; setPlan(p); setChecked(Object.fromEntries(p.items.map((i) => [i.key, i.checked]))); })
      .catch((e) => live && setLoadErr(errMsg(e)));
    return () => { live = false; };
  }, [appId]);
  // An item whose `impliedBy` key is ticked goes anyway: shown checked+disabled and included in the request.
  const effective = (i: PlanItem) => (i.impliedBy && checked[i.impliedBy]) ? true : !!checked[i.key];
  const selected = plan ? plan.items.filter(effective).map((i) => i.key) : [];
  const go = async () => {
    setBusy(true);
    const st = useStore.getState();
    try {
      const r = await ipc.deleteApp(appId, selected);
      onClose();
      st.removeApp(appId);
      let msg = `Deleted ${name}`;
      if (r.freedBytes > 0) msg += ` · ${fmtBytes(r.freedBytes)} freed`;
      if (r.prunedPackages.length) msg += ` · pruned ${r.prunedPackages.length} package${r.prunedPackages.length === 1 ? "" : "s"}`;
      st.notify(msg);
    } catch (e) { st.notify(`Delete failed: ${errMsg(e)}`); setBusy(false); }
  };
  const body = () => {
    if (loadErr) return <p className="text-danger">{loadErr}</p>;
    if (!plan) return <div className="flex items-center gap-2 text-muted"><span className="h-4 w-4 animate-spin rounded-full border-2 border-accent border-t-transparent" />Working out what this program uses…</div>;
    if (plan.blocked) return <p data-testid="delete-app-blocked" className="text-fg">{plan.blocked}</p>;
    return (
      <>
        <p className="mb-2 text-xs text-muted">{plan.rootDir}</p>
        <ul className="flex flex-col divide-y divide-line rounded border border-line">
          {plan.items.map((i) => {
            const implied = !!(i.impliedBy && checked[i.impliedBy]);
            return (
              <li key={i.key}>
                <label className={`flex items-center gap-2 px-3 py-1.5 ${implied ? "opacity-60" : ""}`}>
                  <input data-testid={`delete-item-${i.key}`} type="checkbox" checked={effective(i)} disabled={implied || busy}
                    onChange={(e) => setChecked((c) => ({ ...c, [i.key]: e.target.checked }))} />
                  <span className="flex min-w-0 flex-1 flex-col">
                    <span className="text-fg">{i.label}</span>
                    {i.detail && <span className="truncate text-xs text-muted" title={i.detail}>{i.detail}</span>}
                  </span>
                  <span className="shrink-0 text-xs text-muted">{i.bytes === -1 ? "shared store" : fmtBytes(i.bytes)}</span>
                </label>
              </li>);
          })}
        </ul>
        {plan.always.length > 0 && <p className="mt-2 text-xs text-muted">Also removed: {plan.always.join(", ")}</p>}
      </>);
  };
  const canDelete = !!plan && !plan.blocked && !busy;
  return (
    <Modal testid="delete-app-plan" title={`Delete ${name}`} onClose={onClose}>
      {body()}
      <div className="mt-4 flex justify-end gap-2">
        <Btn onClick={onBack} disabled={busy}>Back</Btn>
        {(!plan || !plan.blocked) && <Btn data-testid="delete-app-confirm" variant="danger" disabled={!canDelete} onClick={go}>{busy ? "Deleting…" : "Delete"}</Btn>}
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
