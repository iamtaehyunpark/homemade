// The Underbar: one resizable, collapsible bottom card per app panel holding every per-program surface as tabs.
// Inactive tabs stay mounted (hidden attribute) so scroll/state survive and E2E can reach their elements.
import { useEffect, useRef, useState, type ReactNode } from "react";
import { ipc } from "../lib/ipc";
import type { AppRecord, GitStatus, RunRecord } from "../lib/types";
import { isActive, isTerminal } from "../lib/format";
import { useStore } from "../state/store";
import LogsTab from "./tabs/LogsTab";
import OutputsTab from "./tabs/OutputsTab";
import RunsTab from "./tabs/RunsTab";
import GitTab from "./tabs/GitTab";
import ProgramTab from "./tabs/ProgramTab";
import ManifestTab from "./tabs/ManifestTab";
import { RunStatus } from "./ui";

export const TABS = ["logs", "outputs", "runs", "git", "program", "manifest"] as const;
export type UnderbarTab = (typeof TABS)[number];
const LABEL: Record<UnderbarTab, string> = { logs: "Logs", outputs: "Outputs", runs: "Runs", git: "Git", program: "Program", manifest: "Manifest" };
const MIN_H = 90;
const DEFAULT_H = 240;

interface Persisted { tab: UnderbarTab; height: number; collapsed: boolean }
const key = (appId: string) => `homemade.underbar.${appId}`;
function loadPersisted(appId: string): Persisted {
  const d: Persisted = { tab: "logs", height: DEFAULT_H, collapsed: false };
  try {
    const v = JSON.parse(localStorage.getItem(key(appId)) ?? "null");
    if (v && typeof v === "object") return { tab: TABS.includes(v.tab) ? v.tab : d.tab, height: Number(v.height) > 0 ? Number(v.height) : d.height, collapsed: !!v.collapsed };
  } catch { /* ignore */ }
  return d;
}

interface Props {
  app: AppRecord;
  latest: RunRecord | null;
  viewRunId: string | null; // explicitly selected run (null → latest)
  onViewRun: (id: string | null) => void;
  values: Record<string, unknown>;
}

export default function Underbar({ app, latest, viewRunId, onViewRun, values }: Props) {
  const appId = app.id;
  const [p, setP] = useState<Persisted>(() => loadPersisted(appId));
  const [outCount, setOutCount] = useState(0);
  const [runCount, setRunCount] = useState(0);
  const [git, setGit] = useState<GitStatus | null>(null);
  const root = useRef<HTMLDivElement>(null);
  const userPicked = useRef(false); // user clicked a tab during the current run
  const prev = useRef<{ id: string | null; status: string | null }>({ id: latest?.id ?? null, status: latest?.status ?? null });

  useEffect(() => { try { localStorage.setItem(key(appId), JSON.stringify(p)); } catch { /* ignore */ } }, [appId, p]);

  const viewed = viewRunId ?? latest?.id ?? null;
  const viewedActive = useStore((s) => (viewed ? isActive(s.runs[viewed]?.status) : false));

  const pick = (tab: UnderbarTab, manual: boolean) => {
    if (manual) userPicked.current = true;
    setP((x) => ({ ...x, tab, collapsed: manual && x.tab === tab ? x.collapsed : false }));
  };

  // Auto-switching: run start → Logs; terminal with new artifacts → Outputs (unless the user picked a tab during the run).
  useEffect(() => {
    const pr = prev.current;
    const id = latest?.id ?? null, status = latest?.status ?? null;
    prev.current = { id, status };
    if (!id) return;
    const started = isActive(status) && (id !== pr.id || (status === "running" && pr.status !== "running"));
    if (started) { userPicked.current = false; setP((x) => ({ ...x, tab: "logs", collapsed: false })); return; }
    if (id === pr.id && isTerminal(status) && !isTerminal(pr.status) && !userPicked.current) {
      ipc.listArtifacts(appId, id, "output").then((a) => { if (a.length && !userPicked.current) setP((x) => ({ ...x, tab: "outputs" })); }).catch(() => {});
    }
  }, [latest?.id, latest?.status]);

  // ⌘1…⌘6 switch tabs when this panel has focus.
  useEffect(() => {
    const panel = root.current?.closest<HTMLElement>("[data-app-panel]");
    if (!panel) return;
    const h = (e: KeyboardEvent) => {
      if (!(e.metaKey || e.ctrlKey) || e.altKey || e.shiftKey) return;
      const n = Number(e.key);
      if (n >= 1 && n <= TABS.length) { e.preventDefault(); pick(TABS[n - 1], true); }
    };
    panel.addEventListener("keydown", h);
    return () => panel.removeEventListener("keydown", h);
  }, []);

  const startDrag = (e: React.MouseEvent) => {
    e.preventDefault();
    const y0 = e.clientY, h0 = p.collapsed ? MIN_H : p.height;
    const max = () => Math.max(MIN_H, (root.current?.parentElement?.clientHeight ?? window.innerHeight) - 120);
    const mv = (ev: MouseEvent) => setP((x) => ({ ...x, collapsed: false, height: Math.round(Math.max(MIN_H, Math.min(max(), h0 - (ev.clientY - y0)))) }));
    const up = () => { window.removeEventListener("mousemove", mv); window.removeEventListener("mouseup", up); document.body.style.cursor = ""; };
    document.body.style.cursor = "row-resize";
    window.addEventListener("mousemove", mv);
    window.addEventListener("mouseup", up);
  };

  const badge = (t: UnderbarTab): ReactNode => {
    switch (t) {
      case "logs": return viewedActive ? <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-ok" title="Run is active" /> : null;
      case "outputs": return outCount ? <Count n={outCount} /> : null;
      case "runs": return runCount ? <Count n={runCount} /> : null;
      case "git": return !git ? null : !git.initialized ? <span className="text-[10px] text-faint" title="Not tracked with Git">not tracked</span>
        : git.dirty ? <span className="h-1.5 w-1.5 rounded-full bg-warn" title="Uncommitted changes" /> : null;
      default: return null;
    }
  };
  const body = (t: UnderbarTab) => {
    switch (t) {
      case "logs": return <LogsTab runId={viewed} isLatest={!viewRunId || viewRunId === latest?.id} onShowLatest={() => onViewRun(null)} />;
      case "outputs": return <OutputsTab appId={appId} runId={viewed} onCount={setOutCount} />;
      case "runs": return <RunsTab appId={appId} latest={latest} selected={viewed} onCount={setRunCount} onSelect={(r) => onViewRun(r.id === latest?.id ? null : r.id)} />;
      case "git": return <GitTab appId={appId} refreshKey={`${app.manifestHash}:${latest?.id}:${latest?.status}`} onStatus={setGit} />;
      case "program": return <ProgramTab app={app} run={latest} values={values} active={p.tab === "program" && !p.collapsed} />;
      case "manifest": return <ManifestTab app={app} active={p.tab === "manifest" && !p.collapsed} />;
    }
  };

  return (
    <div ref={root} data-testid="underbar" data-tab={p.tab} data-collapsed={p.collapsed ? "true" : "false"}
      className="relative flex shrink-0 flex-col border-t border-line bg-panel" style={{ height: p.collapsed ? undefined : p.height }}>
      <div className="absolute inset-x-0 -top-0.5 z-10 h-1.5 cursor-row-resize hover:bg-accent/40" onMouseDown={startDrag} title="Drag to resize" />
      <div role="tablist" className="flex shrink-0 items-center gap-0.5 border-b border-line px-2">
        {TABS.map((t, i) => (
          <button key={t} role="tab" aria-selected={p.tab === t} data-testid={`underbar-tab-${t}`} title={`${LABEL[t]} (⌘${i + 1})`}
            onClick={() => pick(t, true)}
            className={`flex items-center gap-1.5 border-b-2 px-2 py-1 text-[12px] ${p.tab === t && !p.collapsed ? "border-accent text-fg" : "border-transparent text-muted hover:text-fg"}`}>
            {LABEL[t]}{badge(t)}
          </button>
        ))}
        <div className="flex-1" />
        {latest && <RunStatus status={latest.status} exitCode={latest.exitCode} />}
        <button className="ml-1 rounded px-1.5 text-muted hover:bg-panel-2 hover:text-fg" title={p.collapsed ? "Expand" : "Collapse"} aria-label={p.collapsed ? "Expand" : "Collapse"}
          onClick={() => setP((x) => ({ ...x, collapsed: !x.collapsed }))}>{p.collapsed ? "⤒" : "⤓"}</button>
      </div>
      <div className="min-h-0 flex-1" hidden={p.collapsed}>
        {TABS.map((t) => (
          <div key={t} role="tabpanel" data-tab-panel={t} hidden={p.tab !== t} className="h-full min-h-0">{body(t)}</div>
        ))}
      </div>
    </div>
  );
}

function Count({ n }: { n: number }) {
  return <span className="rounded bg-panel-2 px-1 text-[10px] tabular-nums text-muted">{n}</span>;
}
