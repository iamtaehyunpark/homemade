// OWNER: agent F. Dockview workbench: one panel per opened app, plus agent terminal panels.
import { useEffect, useRef, useState } from "react";
import { DockviewReact, themeDark, themeLight, type DockviewApi, type DockviewReadyEvent, type IDockviewPanelProps } from "dockview";
import { useStore } from "../state/store";
import { openAppWindow, forceTab, inWindow } from "../lib/appWindow";
import AppPanel from "./AppPanel";
import AgentTerminal from "./AgentTerminal";
import { useTheme } from "../lib/theme";

export const appPanelId = (appId: string) => `app:${appId}`;
export const agentPanelId = (appId: string) => `agent:${appId}`;

function AppPanelHost(props: IDockviewPanelProps<{ appId: string }>) {
  return <AppPanel appId={props.params.appId} api={props.api} containerApi={props.containerApi} />;
}
function AgentPanelHost(props: IDockviewPanelProps<{ appId: string }>) {
  return <AgentTerminal appId={props.params.appId} />;
}
const components = { app: AppPanelHost, agent: AgentPanelHost };

function Watermark() {
  return (
    <div className="flex h-full items-center justify-center text-muted">
      <div className="text-center">
        <div className="mb-1 text-[15px] text-fg">No app open</div>
        <div className="text-[12px]">Pick an app in the sidebar, or drop a folder anywhere to import it.</div>
      </div>
    </div>
  );
}

export default function Workbench() {
  const [api, setApi] = useState<DockviewApi | null>(null);
  const openApps = useStore((s) => s.openApps);
  const selected = useStore((s) => s.selectedAppId);
  const apps = useStore((s) => s.apps);
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

  return (
    <div ref={rootRef} data-testid="dock-root" className={`${theme === "dark" ? "dockview-theme-dark" : "dockview-theme-light"} h-full w-full`}>
      <DockviewReact components={components} onReady={onReady} watermarkComponent={Watermark} theme={theme === "dark" ? themeDark : themeLight} />
    </div>
  );
}
