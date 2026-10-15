// App list, import (dialog + OS drop), settings entry.
import { useEffect, useRef, useState } from "react";
import Icon, { type IconName } from "./Icon";
import { importPaths, pickApp } from "../lib/importApp";
import type { AppRecord } from "../lib/types";
import { useStore, latestRun } from "../state/store";
import { useDropTarget, setFallbackDrop } from "../lib/dropRouter";
import { fmtBytes, isActive } from "../lib/format";
import { dedupeVenv, VENV_BADGE_MIN_BYTES } from "../lib/venv";
import { Badge, Btn, Dropdown, MenuItem } from "./ui";

export { VENV_BADGE_MIN_BYTES };
const modeLabel: Record<string, string> = { cli: "CLI", rpc_worker: "Worker", webapp: "Web" };

export default function Sidebar() {
  const apps = useStore((s) => s.apps);
  const runs = useStore((s) => s.runs);
  const [filter, setFilter] = useState("");
  const [collapsed, setCollapsed] = useState(() => { try { return localStorage.getItem("homemade.sidebarCollapsed") === "true"; } catch { return false; } });
  const toggleSidebar = () => setCollapsed((value) => { try { localStorage.setItem("homemade.sidebarCollapsed", String(!value)); } catch { /* preference storage unavailable */ } return !value; });
  const createProject = useStore((s) => s.setCreateProjectOpen);
  const pinned = useStore((s) => s.pinned);
  const byPin = (a: AppRecord, b: AppRecord) => Number(pinned.includes(b.id)) - Number(pinned.includes(a.id));
  const filtered = apps.filter((a) => `${a.name} ${a.executionMode}`.toLowerCase().includes(filter.trim().toLowerCase())).sort(byPin);
  const running = filtered.filter((a) => isActive(latestRun(runs, a.id)?.status));
  const idle = filtered.filter((a) => !running.includes(a));
  const view = useStore((s) => s.view);
  const setView = useStore((s) => s.setView);
  const dropRef = useDropTarget<HTMLElement>(importPaths);

  useEffect(() => { setFallbackDrop(importPaths); return () => setFallbackDrop(null); }, []);

  return (
    <aside ref={dropRef} data-collapsed={collapsed} className="app-sidebar flex shrink-0 flex-col border-r border-line" aria-label="Sidebar">
      <div className={`flex h-16 shrink-0 items-center ${collapsed ? "justify-center" : "gap-2 px-4"}`}>
        {!collapsed && <><Icon name="home" size={18} className="text-muted" /><span className="flex-1 text-[14px] font-semibold tracking-tight">Homemade</span></>}
        <button data-testid="sidebar-toggle" aria-label={collapsed ? "Expand sidebar" : "Collapse sidebar"} aria-expanded={!collapsed} title={collapsed ? "Expand sidebar" : "Collapse sidebar"} onClick={toggleSidebar} className="rounded-md p-1.5 text-muted hover:bg-panel-2 hover:text-fg"><Icon name="sidebar" size={17} /></button>
      </div>
      <nav aria-label="Workspace" className={`space-y-1 ${collapsed ? "px-2" : "px-3"}`}>
        <Dropdown testid="import-app" label={<><Icon name="plus" size={17} />{!collapsed && <span className="flex-1 text-left">Add</span>}<span className="sr-only">{collapsed ? "Add" : ""}</span></>} buttonClassName={`sidebar-add w-full !justify-start !border-transparent !bg-transparent !py-2 !shadow-none hover:!bg-panel-2 ${collapsed ? "!justify-center !px-2" : "!px-3"}`}>
          {(close) => (<>
            <MenuItem testid="create-project" onClick={() => { close(); createProject(true); }}><Icon name="terminal" size={15} />Create project…</MenuItem>
            <div className="my-1 border-t border-line" />
            <MenuItem onClick={() => { close(); void pickApp(); }}><Icon name="folder" size={15} />Add folder…</MenuItem>
            <MenuItem onClick={() => { close(); void pickApp(false); }}><Icon name="code" size={15} />Add script…</MenuItem>
            <div className="my-1 border-t border-line" />
            <MenuItem testid="import-draft" onClick={() => { close(); void pickApp(true, true); }}>Auto-import folder…</MenuItem>
          </>)}
        </Dropdown>
        <NavItem icon="code" label="Apps" collapsed={collapsed} active={view === "workbench"} onClick={() => setView("workbench")} />
        <NavItem icon="grid" label="Launch" collapsed={collapsed} testid="launch-nav" active={view === "launch"} onClick={() => setView("launch")} />
        <NavItem icon="activity" label="Monitor" collapsed={collapsed} testid="monitor-nav" active={view === "monitor"} onClick={() => setView("monitor")} />
      </nav>
      <div className={`my-3 border-t border-line ${collapsed ? "mx-3" : "mx-4"}`} />
      {!collapsed && <div className="search-box mx-4 mb-3"><Icon name="search" size={14} /><input aria-label="Search apps" placeholder="Find an app…" value={filter} onChange={(e) => setFilter(e.target.value)} />{filter && <button aria-label="Clear search" onClick={() => setFilter("")}><Icon name="close" size={13} /></button>}</div>}
      <div className={`min-h-0 flex-1 overflow-y-auto ${collapsed ? "px-2" : "px-3"}`}>
        {collapsed ? apps.map((app) => <AppShortcut key={app.id} app={app} />) : <>
          {running.length > 0 && <><GroupLabel>Running <span className="font-normal text-faint">{running.length}</span></GroupLabel>{running.map((a) => <AppRow key={a.id} app={a} />)}</>}
          {idle.length > 0 && <><GroupLabel>Apps <span className="font-normal text-faint">{idle.length}</span></GroupLabel>{idle.map((a) => <AppRow key={a.id} app={a} />)}</>}
          {!apps.length && <p className="px-3 py-3 text-[11px] leading-5 text-faint">Add a folder or script,<br />or drop one here.</p>}
          {apps.length > 0 && !filtered.length && <div className="px-3 py-5 text-center text-[12px] text-muted">No apps match “{filter}”.</div>}
        </>}
      </div>
      <div className={`border-t border-line py-3 ${collapsed ? "px-2" : "px-3"}`}>
        <NavItem icon="settings" label="Settings" collapsed={collapsed} testid="settings" active={view === "settings"} onClick={() => setView("settings")} />
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
  const pinned = useStore((s) => s.pinned.includes(app.id));
  const [migrating, setMigrating] = useState(false);
  const [menu, setMenu] = useState(false);

  const dedupe = async (e: React.MouseEvent) => {
    e.stopPropagation();
    setMigrating(true);
    await dedupeVenv(app);
    setMigrating(false);
  };

  return (
    // The row's testid wraps both the name button and the venv badge, so tests (and the picker) can scope to one app.
    <div data-testid={`sidebar-app-${app.id}`} className={`app-row group relative mb-0.5 rounded-md ${selected ? "bg-accent-soft" : "hover:bg-panel-2"}`} onContextMenu={(e) => { e.preventDefault(); setMenu(true); }}>
      <button onClick={() => openApp(app.id)} title={app.rootDir} aria-current={selected ? "page" : undefined} className="flex w-full items-center gap-2 px-3 py-2 text-left">
        <span className={`h-1.5 w-1.5 shrink-0 rounded-full ${running ? "bg-ok" : "bg-faint"}`} title={running ? "Running" : "Idle"} />
        <span className={`min-w-0 flex-1 truncate ${selected ? "text-accent" : ""}`}>{app.name}</span>
        {pinned && <Icon name="pin" size={12} className="text-faint" />}
        <span className="group-hover:hidden"><Badge>{modeLabel[app.executionMode] ?? app.executionMode}</Badge></span>
      </button>
      <button data-testid="sidebar-app-menu" aria-label="App menu" onClick={(e) => { e.stopPropagation(); setMenu(true); }} className="absolute right-2 top-1.5 hidden rounded p-1 text-muted hover:bg-panel hover:text-fg group-hover:block"><Icon name="more" size={14} /></button>
      {menu && <RowMenu onClose={() => setMenu(false)}>
        <MenuItem testid="sidebar-pin" onClick={() => { setMenu(false); useStore.getState().togglePin(app.id); }}><Icon name="pin" size={14} />{pinned ? "Unpin" : "Pin to top"}</MenuItem>
        <MenuItem testid="sidebar-remove" onClick={() => { setMenu(false); useStore.getState().setRemoveAppId(app.id); }}><Icon name="trash" size={14} />Remove…</MenuItem>
      </RowMenu>}
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

function NavItem({ icon, label, active, collapsed, onClick, testid }: { icon: IconName; label: string; active: boolean; collapsed: boolean; onClick: () => void; testid?: string }) {
  return <button data-testid={testid} aria-label={label} title={collapsed ? label : undefined} aria-current={active ? "page" : undefined} onClick={onClick} className={`nav-item flex w-full items-center gap-3 rounded-md py-2 text-left text-[12px] font-medium ${collapsed ? "justify-center px-2" : "px-3"} ${active ? "bg-accent-soft text-accent" : "text-muted hover:bg-panel-2 hover:text-fg"}`}><Icon name={icon} size={17} />{!collapsed && label}</button>;
}

function AppShortcut({ app }: { app: AppRecord }) {
  const selected = useStore((s) => s.selectedAppId === app.id && s.view === "workbench");
  const running = useStore((s) => isActive(latestRun(s.runs, app.id)?.status));
  return <button aria-label={app.name} title={`${app.name}${running ? " · Running" : ""}`} onClick={() => useStore.getState().openApp(app.id)} className={`relative mb-1 flex h-10 w-full items-center justify-center rounded-md text-[11px] font-semibold ${selected ? "bg-accent-soft text-accent" : "text-muted hover:bg-panel-2"}`}>
    {app.name.split(/[\s_-]+/).filter(Boolean).slice(0, 2).map((word) => word[0]).join("").toUpperCase()}
    {running && <span className="absolute right-1.5 top-1.5 h-1 w-1 rounded-full bg-ok" />}
  </button>;
}

/** Small popup under a row; closes on outside click or Esc. */
function RowMenu({ children, onClose }: { children: React.ReactNode; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const down = (e: MouseEvent) => { if (!ref.current?.contains(e.target as Node)) onClose(); };
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") onClose(); };
    document.addEventListener("mousedown", down); document.addEventListener("keydown", key, true);
    return () => { document.removeEventListener("mousedown", down); document.removeEventListener("keydown", key, true); };
  }, [onClose]);
  return <div ref={ref} role="menu" className="absolute right-2 top-8 z-30 min-w-40 rounded-md border border-line bg-panel py-1 shadow-lg">{children}</div>;
}
