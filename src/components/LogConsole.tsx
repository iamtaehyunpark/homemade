// Collapsible log console for a run: last 2000 lines, auto-scroll, stderr red, progress + metrics.
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { ipc } from "../lib/ipc";
import { useLive } from "../state/live";

export default function LogConsole({ runId }: { runId: string | null }) {
  const live = useLive((s) => (runId ? s.runs[runId] : undefined));
  const [open, setOpen] = useState(true);
  const [height, setHeight] = useState(180);
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
  }, [live?.lines, open]);

  const lines = live?.lines ?? [];
  const metrics = Object.entries(live?.metrics ?? {});
  const startDrag = (e: React.MouseEvent) => {
    const y0 = e.clientY, h0 = height;
    const mv = (ev: MouseEvent) => setHeight(Math.max(60, Math.min(window.innerHeight * 0.7, h0 - (ev.clientY - y0))));
    const up = () => { window.removeEventListener("mousemove", mv); window.removeEventListener("mouseup", up); };
    window.addEventListener("mousemove", mv);
    window.addEventListener("mouseup", up);
  };

  return (
    <div data-testid="log-console" className="shrink-0 border-t border-line bg-bg">
      {open && <div className="h-1 cursor-row-resize hover:bg-accent/40" onMouseDown={startDrag} />}
      <div className="flex items-center gap-2 px-3 py-1 text-[11px]">
        <button className="font-semibold uppercase tracking-wider text-gray-500 hover:text-gray-300" onClick={() => setOpen(!open)}>
          {open ? "▾" : "▸"} Log {lines.length > 0 && <span className="font-normal normal-case text-gray-600">({lines.length})</span>}
        </button>
        {live?.progress && (
          <div className="flex min-w-0 flex-1 items-center gap-2">
            <div data-testid="progress-bar" data-percent={Math.round(live.progress.percent)} className="h-1.5 w-40 overflow-hidden rounded bg-white/10"><div className="h-full bg-accent transition-[width]" style={{ width: `${Math.max(0, Math.min(100, live.progress.percent))}%` }} /></div>
            <span className="tabular-nums text-gray-400">{Math.round(live.progress.percent)}%</span>
            <span className="truncate text-gray-400">{live.progress.message}</span>
          </div>
        )}
        <div className="ml-auto flex gap-1">
          {metrics.map(([k, v]) => (
            <span key={k} className="rounded bg-accent/10 px-1.5 font-mono text-accent">{k}: {typeof v === "number" ? +v.toFixed(3) : String(v)}</span>
          ))}
        </div>
      </div>
      {open && (
        <div ref={box} style={{ height }} onScroll={(e) => { const el = e.currentTarget; stick.current = el.scrollHeight - el.scrollTop - el.clientHeight < 24; }}
          className="overflow-y-auto px-3 pb-2 font-mono text-[11.5px] leading-[1.45] select-text">
          {lines.map((l, i) => <div key={i} data-testid="log-line" data-stream={l.err ? "stderr" : "stdout"} className={`whitespace-pre-wrap break-all ${l.err ? "text-red-400" : "text-gray-300"}`}>{l.text || " "}</div>)}
          {!lines.length && <div className="text-gray-600">{runId ? "No output." : "Output appears here when you run the app."}</div>}
        </div>
      )}
    </div>
  );
}
