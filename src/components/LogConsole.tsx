// Log console for a run (Underbar → Logs): last 2000 lines, auto-scroll, stderr red, progress + metrics, health banner + Explain.
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ipc } from "../lib/ipc";
import { useLive } from "../state/live";
import { useStore } from "../state/store";
import { errMsg, isActive } from "../lib/format";
import { Btn } from "./ui";

interface Props { runId: string | null; filter?: string; stderrOnly?: boolean; hideBefore?: number }

export default function LogConsole({ runId, filter = "", stderrOnly = false, hideBefore = 0 }: Props) {
  const live = useLive((s) => (runId ? s.runs[runId] : undefined));
  const health = useStore((s) => (runId ? s.health[runId] : undefined));
  const runActive = useStore((s) => (runId ? isActive(s.runs[runId]?.status) : false));
  const [explaining, setExplaining] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  const stick = useRef(true);

  useEffect(() => {
    if (!runId || useLive.getState().runs[runId]?.lines.length) return;
    ipc.getRunLog(runId).then((ls) => {
      useLive.getState().seed(runId, ls.map((l) => (l.startsWith("[stderr] ") ? { err: true, text: l.slice(9) } : l.startsWith("[stderr]") ? { err: true, text: l.slice(8) } : { err: false, text: l })));
    }).catch(() => {});
  }, [runId]);

  useLayoutEffect(() => {
    const el = box.current;
    if (el && stick.current) el.scrollTop = el.scrollHeight;
  }, [live?.lines, filter, stderrOnly, hideBefore]);

  const all = live?.lines ?? [];
  const f = filter.toLowerCase();
  const lines = all.slice(hideBefore).filter((l) => (!stderrOnly || l.err) && (!f || l.text.toLowerCase().includes(f)));
  const metrics = Object.entries(live?.metrics ?? {});
  const explain = async () => {
    if (!runId) return;
    setExplaining(true);
    const st = useStore.getState();
    try { st.setHealth(runId, await ipc.explainRun(runId)); } catch (e) { st.notify(`Explain failed: ${errMsg(e)}`); }
    setExplaining(false);
  };

  return (
    <div data-testid="log-console" className="flex h-full min-h-0 flex-col bg-bg">
      {(live?.progress || metrics.length > 0 || runId) && (
        <div className="flex shrink-0 items-center gap-2 px-3 py-1 text-[11px]">
          {live?.progress && (
            <div className="flex min-w-0 flex-1 items-center gap-2">
              <div data-testid="progress-bar" data-percent={Math.round(live.progress.percent)} className="h-1.5 w-40 overflow-hidden rounded bg-line"><div className="h-full bg-accent transition-[width]" style={{ width: `${Math.max(0, Math.min(100, live.progress.percent))}%` }} /></div>
              <span className="tabular-nums text-muted">{Math.round(live.progress.percent)}%</span>
              <span className="truncate text-muted">{live.progress.message}</span>
            </div>
          )}
          <div className="ml-auto flex items-center gap-1">
            {metrics.map(([k, v]) => (
              <span key={k} className="rounded bg-accent/10 px-1.5 font-mono text-accent">{k}: {typeof v === "number" ? +v.toFixed(3) : String(v)}</span>
            ))}
            {runId && <Btn data-testid="explain-run" variant="ghost" className="!py-0 !text-[11px]" disabled={runActive || explaining} onClick={explain} title="Ask the local model what happened">{explaining ? "Explaining…" : "Explain"}</Btn>}
          </div>
        </div>
      )}
      {health && (
        <div data-testid="run-health" data-severity={health.severity}
          className={`mx-3 mb-1 shrink-0 rounded border px-2 py-1 text-[11px] select-text ${health.severity === "error" ? "border-danger/30 bg-danger/10 text-danger" : health.severity === "warn" ? "border-warn/30 bg-warn/10 text-warn" : "border-line bg-panel-2 text-fg"}`}>
          {health.message || "No issues found."}
        </div>
      )}
      <div ref={box} onScroll={(e) => { const el = e.currentTarget; stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24; }}
        className="min-h-0 flex-1 overflow-y-auto px-3 pb-2 font-mono text-[11.5px] leading-[1.45] select-text">
        {lines.map((l, i) => <div key={i} data-testid="log-line" data-stream={l.err ? "stderr" : "stdout"} className={`whitespace-pre-wrap break-all ${l.err ? "text-danger" : "text-fg"}`}>{l.text || " "}</div>)}
        {!lines.length && <div className="text-faint">{!runId ? "No runs yet — press Run" : all.length > hideBefore ? "No lines match the filter." : "No output."}</div>}
      </div>
    </div>
  );
}
