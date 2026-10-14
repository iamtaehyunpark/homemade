// OWNER: agent F. Bottom strip: CPU, RAM, GPU, per-run pills, VRAM queue popover.
import { useState } from "react";
import { ipc } from "../lib/ipc";
import { useStore } from "../state/store";
import { errMsg, fmtMb } from "../lib/format";
import { Btn } from "./ui";

function Meter({ label, pct, text }: { label: string; pct: number; text: string }) {
  const c = pct > 90 ? "bg-red-400" : pct > 70 ? "bg-amber-400" : "bg-accent";
  return (
    <div className="flex items-center gap-1.5" title={`${label}: ${text}`}>
      <span className="text-gray-500">{label}</span>
      <div className="h-1.5 w-14 overflow-hidden rounded bg-white/10"><div className={`h-full ${c}`} style={{ width: `${Math.min(100, Math.max(0, pct))}%` }} /></div>
      <span className="tabular-nums text-gray-300">{text}</span>
    </div>
  );
}

export default function ResourceBar() {
  const r = useStore((s) => s.resources);
  const apps = useStore((s) => s.apps);
  const openApp = useStore((s) => s.openApp);
  const notify = useStore((s) => s.notify);
  const [qOpen, setQOpen] = useState(false);
  const name = (id: string) => apps.find((a) => a.id === id)?.name ?? id;

  return (
    <footer className="relative flex h-7 shrink-0 items-center gap-4 border-t border-line bg-panel px-3 text-[11px]">
      {r ? (<>
        <Meter label="CPU" pct={r.cpuPct} text={`${r.cpuPct.toFixed(0)}%`} />
        <Meter label="RAM" pct={(r.ramUsedMb / Math.max(1, r.ramTotalMb)) * 100} text={`${fmtMb(r.ramUsedMb)} / ${fmtMb(r.ramTotalMb)}`} />
        {r.gpuTotalMb > 0
          ? <Meter label={r.gpuLabel || "GPU"} pct={(r.gpuUsedMb / r.gpuTotalMb) * 100} text={`${fmtMb(r.gpuUsedMb)} / ${fmtMb(r.gpuTotalMb)}`} />
          : r.gpuLabel && <span className="text-gray-500">{r.gpuLabel}</span>}
        <div className="flex min-w-0 flex-1 gap-1 overflow-hidden">
          {r.perRun.map((p) => (
            <button key={p.runId} onClick={() => openApp(p.appId)} className="flex shrink-0 items-center gap-1 rounded bg-emerald-500/10 px-1.5 text-emerald-300 hover:bg-emerald-500/20" title={`CPU ${p.cpuPct.toFixed(0)}%${p.vramMb ? ` · VRAM ${fmtMb(p.vramMb)}` : ""}`}>
              <span className="h-1.5 w-1.5 rounded-full bg-emerald-400" />{name(p.appId)} <span className="text-emerald-500/80">{fmtMb(p.ramMb)}</span>
            </button>
          ))}
        </div>
        <button onClick={() => setQOpen(!qOpen)} disabled={!r.queue.length}
          className={`rounded px-1.5 ${r.queue.length ? "bg-amber-500/15 text-amber-300 hover:bg-amber-500/25" : "text-gray-600"}`}>
          Queue {r.queue.length}
        </button>
      </>) : <span className="text-gray-600">Collecting resource stats…</span>}
      {qOpen && r && r.queue.length > 0 && (
        <div className="absolute bottom-8 right-2 z-40 w-96 rounded border border-line bg-panel p-1 shadow-xl">
          <div className="px-2 py-1 text-[11px] font-semibold uppercase tracking-wider text-gray-500">Waiting for GPU memory</div>
          {r.queue.map((q) => (
            <div key={q.runId} className="flex items-center gap-2 rounded px-2 py-1 hover:bg-white/5">
              <div className="min-w-0 flex-1">
                <div className="truncate">{name(q.appId)} <span className="text-gray-500">needs {fmtMb(q.requiredVramMb)}</span></div>
                {q.reason && <div className="truncate text-[11px] text-gray-500">{q.reason}</div>}
              </div>
              <Btn onClick={() => ipc.forceRun(q.runId).catch((e) => notify(errMsg(e)))}>Run anyway</Btn>
            </div>
          ))}
        </div>
      )}
    </footer>
  );
}
