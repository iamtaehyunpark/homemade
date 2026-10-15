// OWNER: agent F. Dockview workbench: one panel per opened app, plus agent terminal panels.
import { useEffect, useRef, useState } from "react";
import { DockviewReact, themeDark, themeLight, type DockviewApi, type DockviewReadyEvent, type IDockviewPanelProps } from "dockview";
import { useStore, latestRun } from "../state/store";
import { openAppWindow, forceTab, inWindow } from "../lib/appWindow";
import AppPanel from "./AppPanel";
import Icon from "./Icon";
import { Btn } from "./ui";
import { pickApp } from "../lib/importApp";
import { isActive, relTime } from "../lib/format";
import AgentTerminal from "./AgentTerminal";
import type { AgentType } from "../lib/types";
import { useTheme } from "../lib/theme";

export const appPanelId = (appId: string) => `app:${appId}`;
export const agentPanelId = (appId: string) => `agent:${appId}`;

function AppPanelHost(props: IDockviewPanelProps<{ appId: string }>) {
  return <AppPanel appId={props.params.appId} api={props.api} containerApi={props.containerApi} />;
}
function AgentPanelHost(props: IDockviewPanelProps<{ appId: string; initialAgentType?: AgentType }>) {
  return <AgentTerminal appId={props.params.appId} initialAgentType={props.params.initialAgentType} />;
}
const components = { app: AppPanelHost, agent: AgentPanelHost };

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

export default function Workbench() {
  const [api, setApi] = useState<DockviewApi | null>(null);
  const openApps = useStore((s) => s.openApps);
  const selected = useStore((s) => s.selectedAppId);
  const apps = useStore((s) => s.apps);
  const pendingShellAppId = useStore((s) => s.pendingShellAppId);
  const syncing = useRef(false);
  const theme = useTheme();

  const rootRef = useRef<HTMLDivElement>(null);
  const onReady = (e: DockviewReadyEvent) => {
    setApi(e.api);
    // Drag a tab out of the dock ("to nowhere": the sidebar, outside the window, …) → that program opens in its own window.
    let dragging: string | null = null;
    e.api.onWillDragPanel((ev) => { dragging = ev.panel.id.startsWith("app:") ? ev.panel.id.slice(4) : null; });
    e.api.onDidDrop(() => { dragging = null; });
    const onDragEnd = (ne: DragEvent) => {
      const id = dragging; dragging = null;
      if (!id || !rootRef.current) return;
      const r = rootRef.current.getBoundingClientRect();
      const inside = ne.clientX >= r.left && ne.clientX <= r.right && ne.clientY >= r.top && ne.clientY <= r.bottom;
      if (inside && ne.dataTransfer?.dropEffect !== "none") return; // dockview handled it
      if (inside) return; // cancelled inside the dock (Esc) — leave the tab alone
      const app = useStore.getState().apps.find((a) => a.id === id);
      if (app) { openAppWindow(app); useStore.getState().closeApp(id); }
    };
    document.addEventListener("dragend", onDragEnd, true); // Workbench lives for the whole session; no teardown needed
    e.api.onDidRemovePanel((p) => {
      if (syncing.current) return;
      const id = p.id;
      if (id.startsWith("app:")) {
        const appId = id.slice(4);
        useStore.getState().closeApp(appId);
        e.api.getPanel(agentPanelId(appId))?.api.close();
      }
    });
    e.api.onDidActivePanelChange((p) => {
      if (!p) return;
      const appId = p.id.replace(/^(app|agent):/, "");
      if (useStore.getState().selectedAppId !== appId) useStore.getState().select(appId);
    });
  };

  useEffect(() => {
    if (!api) return;
    syncing.current = true;
    const toWindow: string[] = [];
    for (const id of openApps) {
      if (api.getPanel(appPanelId(id))) continue;
      const app = apps.find((a) => a.id === id);
      const layout = app?.manifest.ui.default_layout ?? "tab";
      if (app && (inWindow.has(id) || (layout === "window" && !forceTab.has(id)))) { toWindow.push(id); openAppWindow(app); continue; }
      const ref = api.activePanel;
      api.addPanel({
        id: appPanelId(id), component: "app", title: app?.name ?? id, params: { appId: id },
        ...(layout === "split_right" && ref ? { position: { direction: "right" as const, referencePanel: ref.id } } : {}),
      });
    }
    for (const p of [...api.panels]) {
      const appId = p.id.replace(/^(app|agent):/, "");
      if (!openApps.includes(appId)) p.api.close();
    }
    syncing.current = false;
    // Window-layout apps live in their own webview window; keep them out of the dock's open list.
    if (toWindow.length) for (const id of toWindow) useStore.getState().closeApp(id);
  }, [api, openApps, apps]);

  useEffect(() => {
    if (!api || !selected) return;
    const p = api.getPanel(appPanelId(selected));
    if (p && !p.api.isActive) p.api.setActive();
  }, [api, selected]);

  useEffect(() => {
    if (!api || !pendingShellAppId) return;
    const appPanel = api.getPanel(appPanelId(pendingShellAppId));
    if (!appPanel) return;
    const id = agentPanelId(pendingShellAppId);
    if (!api.getPanel(id)) {
      api.addPanel({ id, component: "agent", title: "Shell", params: { appId: pendingShellAppId, initialAgentType: "custom_shell" }, position: { direction: "right", referencePanel: appPanel.id } });
    }
    useStore.getState().setPendingShellAppId(null);
  }, [api, pendingShellAppId, openApps]);

  return (
    <div ref={rootRef} data-testid="dock-root" className={`${theme === "dark" ? "dockview-theme-dark" : "dockview-theme-light"} h-full w-full`}>
      <DockviewReact components={components} onReady={onReady} watermarkComponent={Watermark} theme={theme === "dark" ? themeDark : themeLight} />
    </div>
  );
}
