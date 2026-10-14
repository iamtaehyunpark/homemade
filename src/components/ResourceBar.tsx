// OWNER: agent F. Bottom strip: CPU, RAM, GPU, per-run pills, VRAM queue popover.
import { useState } from "react";
import { ipc } from "../lib/ipc";
import { useStore } from "../state/store";
import { errMsg, fmtMb } from "../lib/format";
import { Btn } from "./ui";

function Meter({ label, pct, text }: { label: string; pct: number; text: string }) {
  const c = pct > 90 ? "bg-danger" : pct > 70 ? "bg-warn" : "bg-accent";
  return (
    <div className="flex items-center gap-1.5" title={`${label}: ${text}`}>
      <span className="text-muted">{label}</span>
      <div className="h-1.5 w-14 overflow-hidden rounded bg-line"><div className={`h-full ${c}`} style={{ width: `${Math.min(100, Math.max(0, pct))}%` }} /></div>
      <span className="tabular-nums text-fg">{text}</span>
    </div>
  );
}

export default function ResourceBar() {
  const r = useStore((s) => s.resources);
  const apps = useStore((s) => s.apps);
  const openApp = useStore((s) => s.openApp);
  const setView = useStore((s) => s.setView);
  const selectedAppId = useStore((s) => s.selectedAppId);
  const notify = useStore((s) => s.notify);
  const [qOpen, setQOpen] = useState(false);
  const name = (id: string) => apps.find((a) => a.id === id)?.name ?? id;
  // "Current program": the selected app's active run (if any); falls back to the first running one.
  const current = r?.perRun.find((p) => p.appId === selectedAppId) ?? r?.perRun[0];

  return (
    <footer data-testid="resource-bar" className="relative flex h-7 shrink-0 items-center gap-4 border-t border-line bg-panel px-3 text-[11px]">
      {r ? (<>
        <button className="flex items-center gap-4 hover:text-fg" title="Open Monitor" onClick={() => setView("monitor")}>
          <Meter label="CPU" pct={r.cpuPct} text={`${r.cpuPct.toFixed(0)}%`} />
          <Meter label="RAM" pct={(r.ramUsedMb / Math.max(1, r.ramTotalMb)) * 100} text={`${fmtMb(r.ramUsedMb)} / ${fmtMb(r.ramTotalMb)}`} />
          {r.gpuTotalMb > 0
            ? <Meter label={r.gpuLabel || "GPU"} pct={(r.gpuUsedMb / r.gpuTotalMb) * 100} text={`${fmtMb(r.gpuUsedMb)} / ${fmtMb(r.gpuTotalMb)}`} />
            : r.gpuLabel && <span className="text-muted">{r.gpuLabel}</span>}
        </button>
        <span className="h-4 w-px bg-line" />
        {current ? (
          <button data-testid="resource-current" onClick={() => openApp(current.appId)} className="flex min-w-0 items-center gap-2 rounded bg-ok/10 px-1.5 text-ok hover:bg-ok/20" title="Current program">
            <span className="h-1.5 w-1.5 shrink-0 rounded-full bg-ok" />
            <span className="truncate font-medium">{name(current.appId)}</span>
            <span className="font-mono text-ok">CPU {current.cpuPct.toFixed(0)}%</span>
            <span className="font-mono text-ok">RAM {fmtMb(current.ramMb)}</span>
            {current.vramMb > 0 && <span className="font-mono text-ok">GPU {fmtMb(current.vramMb)}</span>}
          </button>
        ) : <span className="text-faint">No program running</span>}
        <div className="flex min-w-0 flex-1 gap-1 overflow-hidden">
          {r.perRun.filter((p) => p.runId !== current?.runId).map((p) => (
            <button key={p.runId} onClick={() => openApp(p.appId)} className="flex shrink-0 items-center gap-1 rounded bg-panel-2 px-1.5 text-muted hover:bg-line" title={`CPU ${p.cpuPct.toFixed(0)}% · RAM ${fmtMb(p.ramMb)}${p.vramMb ? ` · GPU ${fmtMb(p.vramMb)}` : ""}`}>
              <span className="h-1.5 w-1.5 rounded-full bg-ok" />{name(p.appId)} <span className="font-mono">{p.cpuPct.toFixed(0)}% · {fmtMb(p.ramMb)}</span>
            </button>
          ))}
        </div>
        <button onClick={() => setQOpen(!qOpen)} disabled={!r.queue.length}
          className={`rounded px-1.5 ${r.queue.length ? "bg-warn/15 text-warn hover:bg-warn/25" : "text-faint"}`}>
          Queue {r.queue.length}
        </button>
      </>) : <span className="text-faint">Collecting resource stats…</span>}
      {qOpen && r && r.queue.length > 0 && (
        <div className="absolute bottom-8 right-2 z-40 w-96 rounded border border-line bg-panel p-1 shadow-xl">
          <div className="px-2 py-1 text-[11px] font-semibold uppercase tracking-wider text-muted">Waiting for GPU memory</div>
          {r.queue.map((q) => (
            <div key={q.runId} className="flex items-center gap-2 rounded px-2 py-1 hover:bg-panel-2">
              <div className="min-w-0 flex-1">
                <div className="truncate">{name(q.appId)} <span className="text-muted">needs {fmtMb(q.requiredVramMb)}</span></div>
                {q.reason && <div className="truncate text-[11px] text-muted">{q.reason}</div>}
              </div>
              <Btn onClick={() => ipc.forceRun(q.runId).catch((e) => notify(errMsg(e)))}>Run anyway</Btn>
            </div>
          ))}
        </div>
      )}
    </footer>
  );
}
