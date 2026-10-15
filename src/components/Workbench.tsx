// OWNER: agent F. One workspace (its own dockview tab group) per opened program: the program panel plus any number of
// shell panels. The sidebar switches which workspace is visible; the others stay mounted so their terminals keep running.
import { useEffect, useRef, useState } from "react";
import { DockviewReact, themeDark, themeLight, type DockviewApi, type DockviewReadyEvent, type IDockviewHeaderActionsProps, type IDockviewPanelProps } from "dockview";
import { useStore, latestRun } from "../state/store";
import { openAppWindow, forceTab, inWindow } from "../lib/appWindow";
import { workspaces } from "../lib/registry";
import AppPanel from "./AppPanel";
import Icon from "./Icon";
import { Btn } from "./ui";
import { pickApp } from "../lib/importApp";
import { isActive, relTime } from "../lib/format";
import AgentTerminal from "./AgentTerminal";
import type { AgentType } from "../lib/types";
import { useTheme } from "../lib/theme";

export const appPanelId = (appId: string) => `app:${appId}`;

function AppPanelHost(props: IDockviewPanelProps<{ appId: string }>) {
  return <AppPanel appId={props.params.appId} api={props.api} containerApi={props.containerApi} />;
}
function AgentPanelHost(props: IDockviewPanelProps<{ appId: string; initialAgentType?: AgentType }>) {
  return <AgentTerminal appId={props.params.appId} initialAgentType={props.params.initialAgentType} />;
}
const components = { app: AppPanelHost, agent: AgentPanelHost };

/** "+" at the right of every tab bar: another shell for this program (⌘T). */
function AddShell(props: IDockviewHeaderActionsProps) {
  const appId = (props.containerApi.panels[0]?.params as { appId?: string } | undefined)?.appId;
  return (
    <button data-testid="add-shell" aria-label="New shell" title="New shell (⌘T)" onClick={() => appId && workspaces.get(appId)?.addShell()}
      className="mx-1 flex h-full items-center px-1.5 text-muted hover:text-fg"><Icon name="plus" size={14} /></button>
  );
}

function Watermark() {
  const apps = useStore((s) => s.apps);
  const runs = useStore((s) => s.runs);
  const openApp = useStore((s) => s.openApp);
  const sorted = [...apps].sort((a, b) => (b.lastRunAt ?? b.createdAt) - (a.lastRunAt ?? a.createdAt));
  const runningCount = apps.filter((app) => isActive(latestRun(runs, app.id)?.status)).length;
  return (
    <div className="h-full overflow-y-auto bg-panel">
      <div className="workspace-heading">
        <div className="flex items-center gap-3"><span className="font-semibold text-fg">Apps</span><span>{apps.length} apps{runningCount > 0 ? ` · ${runningCount} running` : ""}</span></div>
        <Btn onClick={() => void pickApp()}><Icon name="plus" size={14} /> Add app</Btn>
      </div>
      {apps.length === 0 ? (
        <div className="workspace-empty">
          <Icon name="folder" size={28} className="mb-4 text-faint" />
          <h1 className="mb-2 text-[17px] font-semibold">No apps yet</h1>
          <p className="mb-5 max-w-sm text-[12px] leading-6 text-muted">Import a project folder or script. Homemade will add its inputs, run controls, logs, and outputs.</p>
          <div className="flex gap-2"><Btn variant="primary" onClick={() => void pickApp()}>Import folder…</Btn><Btn onClick={() => void pickApp(false)}>Import script…</Btn></div>
          <p className="mt-5 text-[11px] text-faint">Or drag a folder into this window.</p>
        </div>
      ) : (
        <div className="px-6 py-5">
          <h1 className="mb-4 text-[12px] font-medium text-muted">Apps · recently used first</h1>
          <div className="overflow-x-auto">
            <table className="workspace-table w-full text-left text-[12px]">
              <thead><tr><th>App</th><th>Type</th><th>Status</th><th className="whitespace-nowrap">Last run</th></tr></thead>
              <tbody>{sorted.map((app) => {
                const run = latestRun(runs, app.id);
                return <tr key={app.id}>
                  <td><button onClick={() => openApp(app.id)} className="group flex w-full items-center gap-3 text-left"><Icon name={app.executionMode === "webapp" ? "window" : "code"} size={17} className="text-muted" /><span className="min-w-0"><span className="block font-medium group-hover:text-accent">{app.name}</span><span className="mt-1 block max-w-[420px] truncate font-mono text-[10px] text-faint" title={app.rootDir}>{app.rootDir}</span></span></button></td>
                  <td className="whitespace-nowrap text-muted">{app.executionMode === "webapp" ? "Web app" : app.executionMode === "rpc_worker" ? "Worker" : "CLI"}</td>
                  <td><span className={`inline-flex items-center gap-1.5 ${isActive(run?.status) ? "text-ok" : run?.status === "failed" ? "text-danger" : "text-muted"}`}><span className="h-1.5 w-1.5 rounded-full bg-current" />{run?.status.replaceAll("_", " ") ?? "Idle"}</span></td>
                  <td className="whitespace-nowrap text-muted">{relTime(app.lastRunAt) || "Never"}</td>
                </tr>;
              })}</tbody>
            </table>
          </div>
        </div>
      )}
    </div>
  );
}

/** One program's workspace: its dockview with the program panel and shells. */
function AppWorkspace({ appId, visible }: { appId: string; visible: boolean }) {
  const app = useStore((s) => s.apps.find((a) => a.id === appId));
  const pendingShell = useStore((s) => s.pendingShellAppId === appId);
  const theme = useTheme();
  const rootRef = useRef<HTMLDivElement>(null);
  const [api, setApi] = useState<DockviewApi | null>(null);
  const shells = useRef(0);

  const onReady = (e: DockviewReadyEvent) => {
    setApi(e.api);
    e.api.addPanel({ id: appPanelId(appId), component: "app", title: app?.name ?? appId, params: { appId } });
    const addShell = () => {
      const last = [...e.api.panels].reverse().find((p) => p.id.startsWith("shell:"));
      const n = ++shells.current;
      e.api.addPanel({
        id: `shell:${appId}:${n}`, component: "agent", title: n === 1 ? "Shell" : `Shell ${n}`, params: { appId, initialAgentType: "custom_shell" },
        position: last ? { direction: "within", referencePanel: last.id } : { direction: "right", referencePanel: appPanelId(appId) },
      });
    };
    workspaces.set(appId, { addShell });
    // Drag the program tab out of the dock ("to nowhere") → it opens in its own window.
    let dragging = false;
    e.api.onWillDragPanel((ev) => { dragging = ev.panel.id === appPanelId(appId); });
    e.api.onDidDrop(() => { dragging = false; });
    const onDragEnd = (ne: DragEvent) => {
      if (!dragging || !rootRef.current) return;
      dragging = false;
      const r = rootRef.current.getBoundingClientRect();
      const inside = ne.clientX >= r.left && ne.clientX <= r.right && ne.clientY >= r.top && ne.clientY <= r.bottom;
      if (inside) return; // dockview handled it, or cancelled (Esc)
      const a = useStore.getState().apps.find((x) => x.id === appId);
      if (a) { openAppWindow(a); useStore.getState().closeApp(appId); }
    };
    document.addEventListener("dragend", onDragEnd, true);
    e.api.onDidRemovePanel((p) => { if (p.id === appPanelId(appId)) { document.removeEventListener("dragend", onDragEnd, true); useStore.getState().closeApp(appId); } });
  };
  useEffect(() => () => { workspaces.delete(appId); }, [appId]);
  useEffect(() => { if (api && app) api.getPanel(appPanelId(appId))?.setTitle(app.name); }, [api, app?.name]);
  useEffect(() => {
    if (!api || !pendingShell) return;
    workspaces.get(appId)?.addShell();
    useStore.getState().setPendingShellAppId(null);
  }, [api, pendingShell]);
  useEffect(() => { if (visible && api) api.layout(rootRef.current?.clientWidth ?? 0, rootRef.current?.clientHeight ?? 0); }, [visible, api]);

  return (
    <div ref={rootRef} data-testid="dock-root" data-workspace={appId} hidden={!visible}
      className={`${theme === "dark" ? "dockview-theme-dark" : "dockview-theme-light"} h-full w-full`}
      onKeyDown={(e) => { if ((e.metaKey || e.ctrlKey) && !e.shiftKey && e.key.toLowerCase() === "t") { e.preventDefault(); workspaces.get(appId)?.addShell(); } }}>
      <DockviewReact components={components} onReady={onReady} rightHeaderActionsComponent={AddShell} theme={theme === "dark" ? themeDark : themeLight} />
    </div>
  );
}

export default function Workbench() {
  const openApps = useStore((s) => s.openApps);
  const selected = useStore((s) => s.selectedAppId);
  const apps = useStore((s) => s.apps);

  // Window-layout apps live in their own webview window; keep them out of the workspaces.
  useEffect(() => {
    for (const id of openApps) {
      const app = apps.find((a) => a.id === id);
      if (app && (inWindow.has(id) || (app.manifest.ui.default_layout === "window" && !forceTab.has(id)))) { openAppWindow(app); useStore.getState().closeApp(id); }
    }
  }, [openApps, apps]);

  const current = selected && openApps.includes(selected) ? selected : null;
  return (
    <div className="h-full w-full">
      {!current && <Watermark />}
      {openApps.map((id) => <AppWorkspace key={id} appId={id} visible={id === current} />)}
    </div>
  );
}
