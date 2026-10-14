// Launch view: a grid of program tiles. One click opens the program in its own solo window (no panels) — like an app launcher.
import { useState } from "react";
import { useStore } from "../state/store";
import { assetUrl } from "../lib/ipc";
import { isActive } from "../lib/format";
import { openAppWindow } from "../lib/appWindow";
import type { AppRecord } from "../lib/types";

const PALETTE = ["#2563eb", "#7c3aed", "#db2777", "#ea580c", "#16a34a", "#0891b2", "#ca8a04", "#4f46e5"];
const colorOf = (id: string) => PALETTE[[...id].reduce((h, c) => (h * 31 + c.charCodeAt(0)) >>> 0, 7) % PALETTE.length];
const initials = (name: string) => name.split(/[\s_-]+/).filter(Boolean).slice(0, 2).map((w) => w[0]!.toUpperCase()).join("") || "?";

export default function Launch() {
  const apps = useStore((s) => s.apps);
  const runs = useStore((s) => s.runs);
  const [filter, setFilter] = useState("");
  const running = new Set(Object.values(runs).filter((r) => isActive(r.status)).map((r) => r.appId));
  const shown = apps.filter((a) => a.name.toLowerCase().includes(filter.toLowerCase())).sort((a, b) => (b.lastRunAt ?? 0) - (a.lastRunAt ?? 0));
  return (
    <div data-testid="launch-view" className="flex h-full flex-col overflow-y-auto bg-bg p-6">
      <div className="mb-5 flex items-center gap-3">
        <h1 className="text-[18px] font-semibold">Launch</h1>
        <span className="text-muted">Click a program to open it in its own window.</span>
        <div className="flex-1" />
        <input data-testid="launch-filter" value={filter} onChange={(e) => setFilter(e.target.value)} placeholder="Filter…"
          className="w-48 rounded border border-line bg-panel px-2 py-1 outline-none focus:border-accent" />
      </div>
      {!apps.length && <div className="text-muted">No programs yet — import one from the sidebar.</div>}
      <div className="grid gap-4" style={{ gridTemplateColumns: "repeat(auto-fill, minmax(128px, 1fr))" }}>
        {shown.map((a) => <Tile key={a.id} app={a} running={running.has(a.id)} />)}
      </div>
    </div>
  );
}

function Tile({ app, running }: { app: AppRecord; running: boolean }) {
  const [imgOk, setImgOk] = useState(true);
  const launch = () => { openAppWindow(app); useStore.getState().closeApp(app.id); };
  return (
    <button data-testid="launch-tile" data-app={app.id} onClick={launch} title={`${app.name}\n${app.rootDir}`}
      className="group flex flex-col items-center gap-2 rounded-lg p-3 text-center outline-none hover:bg-panel-2 focus-visible:ring-2 focus-visible:ring-accent">
      <div className="relative">
        {imgOk
          ? <img src={assetUrl(`${app.rootDir}/icon.png`)} alt="" onError={() => setImgOk(false)} className="h-16 w-16 rounded-2xl object-cover shadow transition-transform group-hover:scale-105" />
          : <div className="flex h-16 w-16 items-center justify-center rounded-2xl text-[22px] font-bold text-white shadow transition-transform group-hover:scale-105" style={{ background: colorOf(app.id) }}>{initials(app.name)}</div>}
        {running && <span data-testid="launch-running" className="absolute -right-1 -top-1 h-3.5 w-3.5 rounded-full border-2 border-bg bg-ok" title="Running" />}
      </div>
      <div className="w-full truncate text-[12px] font-medium">{app.name}</div>
      <div className="text-[10px] uppercase tracking-wider text-faint">{app.executionMode === "webapp" ? "web" : app.hasCustomUi ? "ui" : app.executionMode.replace("_", " ")}</div>
    </button>
  );
}
