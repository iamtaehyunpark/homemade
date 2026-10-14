// OWNER: agent F. Dockview workbench: one panel per opened app, plus agent terminal panels.
import { useEffect, useRef, useState } from "react";
import { DockviewReact, type DockviewApi, type DockviewReadyEvent, type IDockviewPanelProps } from "dockview";
import { useStore } from "../state/store";
import { openAppWindow } from "../lib/appWindow";
import AppPanel from "./AppPanel";
import AgentTerminal from "./AgentTerminal";

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
    <div className="flex h-full items-center justify-center text-gray-500">
      <div className="text-center">
        <div className="mb-1 text-[15px] text-gray-300">No app open</div>
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

  const onReady = (e: DockviewReadyEvent) => {
    setApi(e.api);
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
      if (layout === "window" && app) { toWindow.push(id); openAppWindow(app); continue; }
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
    <div className="dockview-theme-dark h-full w-full">
      <DockviewReact components={components} onReady={onReady} watermarkComponent={Watermark} />
    </div>
  );
}
