// Personal app launcher: open a program in its own window.
import { useEffect, useRef, useState } from "react";
import { useStore } from "../state/store";
import { assetUrl } from "../lib/ipc";
import { isActive } from "../lib/format";
import { openAppWindow } from "../lib/appWindow";
import Icon from "./Icon";
import { Btn } from "./ui";
import { pickApp } from "../lib/importApp";
import type { AppRecord } from "../lib/types";

const COLORS = ["#4d7bc3", "#8a6bb8", "#c5758c", "#c88a53", "#55967b", "#5095a5", "#aa9151", "#6f78b5"];
function appColor(id: string) {
  const hash = [...id].reduce((value, char) => (value * 31 + char.charCodeAt(0)) >>> 0, 7);
  return COLORS[hash % COLORS.length];
}
const initials = (name: string) => name.split(/[\s_-]+/).filter(Boolean).slice(0, 2).map((word) => word[0]).join("").toUpperCase();

export default function Launch() {
  const registered = useStore((s) => s.apps);
  const apps = registered.filter((app) => app.manifest.app.exec.trim());
  const runs = useStore((s) => s.runs);
  const windowed = useStore((s) => s.windowed);
  const [filter, setFilter] = useState("");
  const running = new Set(Object.values(runs).filter((run) => isActive(run.status)).map((run) => run.appId));
  const available = apps.filter((app) => !windowed[app.id]);
  const shown = available.filter((app) => app.name.toLowerCase().includes(filter.trim().toLowerCase()))
    .sort((a, b) => (b.lastRunAt ?? 0) - (a.lastRunAt ?? 0));
  const openCount = apps.length - available.length;
  // Long-press a tile (like rearranging a phone's home screen) → every tile shows a remove badge until Done / Esc.
  const [editing, setEditing] = useState(false);
  useEffect(() => {
    if (!editing) return;
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") setEditing(false); };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  }, [editing]);

  return (
    <div data-testid="launch-view" className="launcher h-full overflow-y-auto">
      <div className="mx-auto w-full max-w-[940px] px-8 pb-10 pt-9">
        <div className="mb-10 flex flex-wrap items-center justify-between gap-4">
          <h1 className="text-[20px] font-semibold tracking-tight">Launch</h1>
          {editing && <Btn data-testid="launch-done" variant="primary" onClick={() => setEditing(false)}>Done</Btn>}
          <div className="search-box w-64 !rounded-lg !py-2">
            <Icon name="search" size={15} />
            <input data-testid="launch-filter" aria-label="Search programs" value={filter} onChange={(e) => setFilter(e.target.value)} placeholder="Search apps" />
            {filter && <button aria-label="Clear search" onClick={() => setFilter("")}><Icon name="close" size={14} /></button>}
          </div>
        </div>
        {!apps.length && <div className="py-16 text-center"><Icon name="grid" size={30} className="mx-auto mb-4 text-faint" /><h2 className="mb-2 text-[16px] font-medium">No apps yet</h2><p className="mb-5 text-[12px] text-muted">Add an app to see it here.</p><Btn onClick={() => void pickApp()}><Icon name="plus" size={14} />Add app</Btn></div>}
        {!!apps.length && !shown.length && <div className="py-16 text-center"><h2 className="mb-2 text-[16px] font-medium">{available.length ? "No matching apps" : "All apps are open"}</h2><p className="text-[12px] text-muted">{available.length ? `No apps match “${filter}”.` : "Close an app’s window to return it to the launcher."}</p>{filter && <Btn className="mt-4" onClick={() => setFilter("")}>Clear search</Btn>}</div>}
        <div className="launcher-grid" data-editing={editing ? "1" : undefined}>
          {shown.map((app) => <Tile key={app.id} app={app} running={running.has(app.id)} editing={editing} onLongPress={() => setEditing(true)} />)}
        </div>
        {editing && <p className="mt-6 text-center text-[11px] text-faint">Tap × to remove a program from Homemade · Esc or Done to finish</p>}
        {openCount > 0 && <div data-testid="launch-open-count" className="mt-8 text-center text-[11px] text-faint">{openCount} app{openCount > 1 ? "s" : ""} open in {openCount > 1 ? "their own windows" : "its own window"}</div>}
      </div>
    </div>
  );
}

function Tile({ app, running, editing, onLongPress }: { app: AppRecord; running: boolean; editing: boolean; onLongPress: () => void }) {
  const [imgOk, setImgOk] = useState(true);
  const timer = useRef<number | null>(null);
  const fired = useRef(false);
  const launch = () => { if (fired.current || editing) { fired.current = false; return; } openAppWindow(app); useStore.getState().closeApp(app.id); };
  const press = () => { fired.current = false; timer.current = window.setTimeout(() => { fired.current = true; onLongPress(); }, 550); };
  const release = () => { if (timer.current) { window.clearTimeout(timer.current); timer.current = null; } };
  return (
    <button data-testid="launch-tile" data-app={app.id} onClick={launch} onPointerDown={press} onPointerUp={release} onPointerLeave={release} onPointerCancel={release}
      onContextMenu={(e) => { e.preventDefault(); onLongPress(); }}
      title={editing ? app.name : `Open ${app.name}`} className={`launcher-tile group flex min-w-0 flex-col items-center rounded-xl px-3 pb-3 pt-4 text-center ${editing ? "launcher-tile-editing" : ""}`}>
      <div className="relative mb-3">
        {editing && <span role="button" data-testid="launch-remove" aria-label={`Remove ${app.name}`} onClick={(e) => { e.stopPropagation(); useStore.getState().setRemoveAppId(app.id); }}
          className="absolute -left-2 -top-2 z-10 flex h-6 w-6 items-center justify-center rounded-full border border-line bg-panel text-fg shadow hover:bg-danger hover:text-white"><Icon name="close" size={13} /></span>}
        {imgOk
          ? <img src={assetUrl(`${app.rootDir}/icon.png`)} alt="" onError={() => setImgOk(false)} className="launcher-icon object-cover" />
          : <div className="launcher-icon flex items-center justify-center text-[25px] font-medium tracking-tight text-white" style={{ background: `linear-gradient(155deg, color-mix(in srgb, ${appColor(app.id)} 78%, white), ${appColor(app.id)})` }}>{initials(app.name) || "A"}</div>}
        {running && <span data-testid="launch-running" className="absolute -right-1 -top-1 h-3 w-3 rounded-full border-2 border-panel bg-ok" title="Running" />}
      </div>
      <span className="line-clamp-2 w-full break-words text-[12px] font-medium leading-5">{app.name}</span>
    </button>
  );
}
