// App list, import (dialog + OS drop), settings entry.
import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ipc } from "../lib/ipc";
import type { AppRecord } from "../lib/types";
import { useStore, latestRun } from "../state/store";
import { useDropTarget, setFallbackDrop } from "../lib/dropRouter";
import { errMsg, fmtBytes, isActive } from "../lib/format";
import { Badge, Btn, Dropdown, MenuItem } from "./ui";

/** Spec §8B: badge only when the unmanaged venv exceeds 50 MB. */
export const VENV_BADGE_MIN_BYTES = 50 * 1024 * 1024;
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
        <span className="text-[11px] font-semibold uppercase tracking-wider text-gray-500">Apps</span>
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
        {apps.map((a) => <AppRow key={a.id} app={a} />)}
        {!apps.length && (
          <div className="m-2 rounded border border-dashed border-line p-4 text-center text-[12px] text-gray-500">
            Drop a folder or script here to import it.
          </div>
        )}
      </div>
      <div className="border-t border-line p-1.5">
        <button data-testid="monitor-nav" onClick={() => setView(view === "monitor" ? "workbench" : "monitor")}
          className={`flex w-full items-center gap-2 rounded px-2 py-1.5 text-left hover:bg-white/5 ${view === "monitor" ? "bg-white/5 text-accent" : "text-gray-300"}`}>
          📈 Monitor
        </button>
        <button data-testid="settings" onClick={() => setView(view === "settings" ? "workbench" : "settings")}
          className={`flex w-full items-center gap-2 rounded px-2 py-1.5 text-left hover:bg-white/5 ${view === "settings" ? "bg-white/5 text-accent" : "text-gray-300"}`}>
          ⚙ Settings
        </button>
      </div>
    </aside>
  );
}

function AppRow({ app }: { app: AppRecord }) {
  const selected = useStore((s) => s.selectedAppId === app.id && s.view === "workbench");
  const running = useStore((s) => isActive(latestRun(s.runs, app.id)?.status));
  const openApp = useStore((s) => s.openApp);
  const [migrating, setMigrating] = useState(false);

  const dedupe = async (e: React.MouseEvent) => {
    e.stopPropagation();
    setMigrating(true);
    const st = useStore.getState();
    try {
      const freed = await ipc.migrateVenv(app.id);
      st.notify(`${app.name}: freed ${fmtBytes(freed)}`);
      st.upsertApp(await ipc.getApp(app.id));
    } catch (err) { st.notify(`Deduplicate failed: ${errMsg(err)}`); }
    setMigrating(false);
  };

  return (
    <div data-testid={`sidebar-app-${app.id}`} onClick={() => openApp(app.id)} title={app.rootDir}
      className={`group mb-px cursor-default rounded px-2 py-1.5 ${selected ? "bg-accent/10" : "hover:bg-white/5"}`}>
      <div className="flex items-center gap-2">
        <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${running ? "bg-emerald-400 shadow-[0_0_6px] shadow-emerald-400" : "bg-gray-600"}`} />
        <span className={`min-w-0 flex-1 truncate ${selected ? "text-accent" : ""}`}>{app.name}</span>
        <Badge>{modeLabel[app.executionMode] ?? app.executionMode}</Badge>
      </div>
      {app.unmanagedVenvBytes > VENV_BADGE_MIN_BYTES && (
        <div className="ml-3.5 mt-1">
          <Btn data-testid="venv-dedupe" className="!px-1.5 !py-0 !text-[10px] text-amber-400" disabled={migrating} onClick={dedupe} title="Move this app's venv into the shared store">
            {fmtBytes(app.unmanagedVenvBytes)} · {migrating ? "Deduplicating…" : "Deduplicate"}
          </Btn>
        </div>
      )}
    </div>
  );
}
