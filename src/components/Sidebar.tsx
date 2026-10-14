// App list, import (dialog + OS drop), settings entry.
import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ipc } from "../lib/ipc";
import type { AppRecord } from "../lib/types";
import { useStore, latestRun } from "../state/store";
import { useDropTarget, setFallbackDrop } from "../lib/dropRouter";
import { errMsg, fmtBytes, isActive } from "../lib/format";
import { dedupeVenv, VENV_BADGE_MIN_BYTES } from "../lib/venv";
import { Badge, Btn, Dropdown, MenuItem } from "./ui";

export { VENV_BADGE_MIN_BYTES };
const modeLabel: Record<string, string> = { cli: "CLI", rpc_worker: "Worker", webapp: "Web" };

async function importPaths(paths: string[]) {
  const st = useStore.getState();
  for (const p of paths) {
    try {
      const app = await ipc.importApp(p);
      st.upsertApp(app);
      st.openApp(app.id);
    } catch (e) { st.notify(`Import failed: ${errMsg(e)}`); }
  }
}

export default function Sidebar() {
  const apps = useStore((s) => s.apps);
  const runs = useStore((s) => s.runs);
  const running = apps.filter((a) => isActive(latestRun(runs, a.id)?.status));
  const idle = apps.filter((a) => !running.includes(a));
  const view = useStore((s) => s.view);
  const setView = useStore((s) => s.setView);
  const dropRef = useDropTarget<HTMLElement>(importPaths);

  useEffect(() => { setFallbackDrop(importPaths); return () => setFallbackDrop(null); }, []);

  const pick = async (directory: boolean) => {
    const r = await open(directory ? { directory: true, title: "Import app folder" } : { multiple: false, title: "Import script", filters: [{ name: "Scripts", extensions: ["py", "js", "ts", "sh", "toml"] }] });
    if (typeof r === "string") importPaths([r]);
  };

  const pickDraft = async () => {
    const r = await open({ directory: true, title: "Import folder & draft manifest" });
    if (typeof r === "string") useStore.getState().setDraftPath(r);
  };

  return (
    <aside ref={dropRef} className="flex w-60 shrink-0 flex-col border-r border-line bg-panel">
      <div className="flex items-center justify-between px-3 pb-2 pt-3">
        <span className="text-[13px] font-semibold">Homemade</span>
        <Dropdown testid="import-app" label="Import app…" align="right">
          {(close) => (<>
            <MenuItem onClick={() => { close(); pick(true); }}>Folder…</MenuItem>
            <MenuItem onClick={() => { close(); pick(false); }}>Script file…</MenuItem>
            <div className="my-1 border-t border-line" />
            <MenuItem testid="import-draft" onClick={() => { close(); pickDraft(); }}>Import folder &amp; draft manifest…</MenuItem>
          </>)}
        </Dropdown>
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-1.5">
        {running.length > 0 && (<>
          <GroupLabel>Running <span className="font-normal text-faint">{running.length}</span></GroupLabel>
          {running.map((a) => <AppRow key={a.id} app={a} />)}
        </>)}
        {idle.length > 0 && (<>
          <GroupLabel>Apps <span className="font-normal text-faint">{idle.length}</span></GroupLabel>
          {idle.map((a) => <AppRow key={a.id} app={a} />)}
        </>)}
        {!apps.length && (
          <div className="m-2 rounded border border-dashed border-line p-4 text-center text-[12px] text-muted">
            Drop a folder or script here to import it.
          </div>
        )}
      </div>
      <div className="border-t border-line p-1.5">
        <button data-testid="launch-nav" onClick={() => setView(view === "launch" ? "workbench" : "launch")} title="Program launcher: open any program in its own window"
          className={`flex w-full items-center gap-2 rounded px-2 py-1.5 text-left hover:bg-panel-2 ${view === "launch" ? "bg-panel-2 text-accent" : "text-fg"}`}>
          🚀 Launch
        </button>
        <button data-testid="monitor-nav" onClick={() => setView(view === "monitor" ? "workbench" : "monitor")}
          className={`flex w-full items-center gap-2 rounded px-2 py-1.5 text-left hover:bg-panel-2 ${view === "monitor" ? "bg-panel-2 text-accent" : "text-fg"}`}>
          📈 Monitor
        </button>
        <button data-testid="settings" onClick={() => setView(view === "settings" ? "workbench" : "settings")}
          className={`flex w-full items-center gap-2 rounded px-2 py-1.5 text-left hover:bg-panel-2 ${view === "settings" ? "bg-panel-2 text-accent" : "text-fg"}`}>
          ⚙ Settings
        </button>
      </div>
    </aside>
  );
}

function GroupLabel({ children }: { children: React.ReactNode }) {
  return <div className="px-2 pb-0.5 pt-2 text-[10px] font-semibold uppercase tracking-wider text-muted">{children}</div>;
}

function AppRow({ app }: { app: AppRecord }) {
  const selected = useStore((s) => s.selectedAppId === app.id && s.view === "workbench");
  const running = useStore((s) => isActive(latestRun(s.runs, app.id)?.status));
  const openApp = useStore((s) => s.openApp);
  const [migrating, setMigrating] = useState(false);

  const dedupe = async (e: React.MouseEvent) => {
    e.stopPropagation();
    setMigrating(true);
    await dedupeVenv(app);
    setMigrating(false);
  };

  return (
    <div data-testid={`sidebar-app-${app.id}`} onClick={() => openApp(app.id)} title={app.rootDir}
      className={`group mb-px cursor-default rounded px-2 py-1.5 ${selected ? "bg-accent/10" : "hover:bg-panel-2"}`}>
      <div className="flex items-center gap-2">
        <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${running ? "bg-ok shadow-[0_0_6px] shadow-ok" : "bg-faint"}`} title={running ? "Running" : "Idle"} />
        <span className={`min-w-0 flex-1 truncate ${selected ? "text-accent" : ""}`}>{app.name}</span>
        <Badge>{modeLabel[app.executionMode] ?? app.executionMode}</Badge>
      </div>
      {app.unmanagedVenvBytes > VENV_BADGE_MIN_BYTES && (
        <div className="ml-3.5 mt-1">
          <Btn data-testid="venv-dedupe" className="!px-1.5 !py-0 !text-[10px] text-warn" disabled={migrating} onClick={dedupe} title="Move this app's venv into the shared store">
            {fmtBytes(app.unmanagedVenvBytes)} · {migrating ? "Deduplicating…" : "Deduplicate"}
          </Btn>
        </div>
      )}
    </div>
  );
}
