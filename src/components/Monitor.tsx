// Full-pane resource monitor: 5-minute history graphs for CPU / RAM / GPU memory, per-run usage table
// with per-run sparklines, and the VRAM admission queue. Data comes from resources://tick (1 Hz) kept in
// useStore.resourceHistory; no charting library, plain SVG.
import { useEffect, useMemo, useState } from "react";
import { useStore, HISTORY_TICKS } from "../state/store";
import type { ListeningPort, RunResourceUsage } from "../lib/types";
import { fmtMb, errMsg } from "../lib/format";
import { ipc } from "../lib/ipc";
import { Btn } from "./ui";

const W = 600, H = 120;

/** Polyline over the last N ticks; `max` fixes the y scale (percent → 100, memory → total). */
function Graph({ label, series, max, unit, color, current, testid }: { label: string; series: number[]; max: number; unit: (v: number) => string; color: string; current: string; testid: string }) {
  const path = useMemo(() => {
    if (!series.length) return "";
    const n = HISTORY_TICKS;
    const off = n - series.length;
    return series.map((v, i) => `${(((off + i) / (n - 1)) * W).toFixed(1)},${(H - (Math.min(v, max) / Math.max(1, max)) * H).toFixed(1)}`).join(" ");
  }, [series, max]);
  const area = path ? `${path.split(" ")[0].split(",")[0]},${H} ${path} ${W},${H}` : "";
  return (
    <div data-testid={testid} className="rounded border border-line bg-panel p-3">
      <div className="mb-1 flex items-baseline justify-between">
        <span className="text-[12px] font-semibold uppercase tracking-wider text-gray-400">{label}</span>
        <span className="font-mono text-[13px]" style={{ color }}>{current}</span>
      </div>
      <svg viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="none" className="h-28 w-full">
        {[0.25, 0.5, 0.75].map((f) => <line key={f} x1={0} x2={W} y1={H * f} y2={H * f} stroke="currentColor" className="text-white/5" strokeWidth={1} />)}
        {area && <polygon points={area} fill={color} fillOpacity={0.12} />}
        {path && <polyline points={path} fill="none" stroke={color} strokeWidth={1.5} vectorEffect="non-scaling-stroke" />}
      </svg>
      <div className="flex justify-between text-[10px] text-gray-600"><span>5 min ago</span><span>{unit(max)} max</span><span>now</span></div>
    </div>
  );
}

function Spark({ series, max, color }: { series: number[]; max: number; color: string }) {
  const w = 90, h = 18;
  const pts = series.map((v, i) => `${((i / Math.max(1, series.length - 1)) * w).toFixed(1)},${(h - (Math.min(v, max) / Math.max(1, max)) * h).toFixed(1)}`).join(" ");
  return <svg viewBox={`0 0 ${w} ${h}`} className="h-[18px] w-[90px]"><polyline points={pts} fill="none" stroke={color} strokeWidth={1} vectorEffect="non-scaling-stroke" /></svg>;
}

export default function Monitor() {
  const r = useStore((s) => s.resources);
  const hist = useStore((s) => s.resourceHistory);
  const apps = useStore((s) => s.apps);
  const runs = useStore((s) => s.runs);
  const openApp = useStore((s) => s.openApp);
  const notify = useStore((s) => s.notify);
  const [stopping, setStopping] = useState<string | null>(null);
  const [ports, setPorts] = useState<ListeningPort[] | null>(null);
  const [killing, setKilling] = useState<number | null>(null);
  const name = (id: string) => apps.find((a) => a.id === id)?.name ?? id;
  const refreshPorts = () => ipc.listListeningPorts().then(setPorts).catch(() => setPorts([]));
  useEffect(() => { refreshPorts(); const t = setInterval(refreshPorts, 3000); return () => clearInterval(t); }, []);
  const killPort = async (p: ListeningPort) => {
    setKilling(p.pid);
    try { await ipc.killPortOwner(p.pid); notify(`Stopped ${p.command} (pid ${p.pid}) on port ${p.port}`); } catch (e) { notify(errMsg(e)); }
    setKilling(null); refreshPorts();
  };

  const cpu = useMemo(() => hist.map((x) => x.cpuPct), [hist]);
  const ram = useMemo(() => hist.map((x) => x.ramUsedMb), [hist]);
  const gpu = useMemo(() => hist.map((x) => x.gpuUsedMb), [hist]);
  const perRunHist = useMemo(() => {
    const m: Record<string, { cpu: number[]; ram: number[]; vram: number[] }> = {};
    for (const snap of hist) for (const p of snap.perRun) {
      const e = (m[p.runId] ??= { cpu: [], ram: [], vram: [] });
      e.cpu.push(p.cpuPct); e.ram.push(p.ramMb); e.vram.push(p.vramMb);
    }
    return m;
  }, [hist]);
  const peak = (xs: number[]) => xs.reduce((a, b) => Math.max(a, b), 0);

  if (!r) return <div data-testid="monitor" className="p-6 text-gray-500">Collecting resource stats…</div>;
  const stop = async (p: RunResourceUsage) => {
    setStopping(p.runId);
    try { await ipc.stopRun(p.runId); } catch (e) { notify(errMsg(e)); }
    setStopping(null);
  };

  return (
    <div data-testid="monitor" className="h-full overflow-y-auto p-4">
      <div className="mb-3 flex items-baseline gap-3">
        <h1 className="text-[15px] font-semibold">Monitor</h1>
        <span className="text-[11px] text-gray-500">1 Hz · last {Math.round(HISTORY_TICKS / 60)} min · {hist.length} samples</span>
      </div>
      <div className="grid grid-cols-1 gap-3 lg:grid-cols-3">
        <Graph testid="monitor-cpu" label="CPU" series={cpu} max={100} unit={(v) => `${v}%`} color="#6ea8fe" current={`${r.cpuPct.toFixed(0)}%`} />
        <Graph testid="monitor-ram" label="RAM" series={ram} max={r.ramTotalMb} unit={fmtMb} color="#34d399" current={`${fmtMb(r.ramUsedMb)} / ${fmtMb(r.ramTotalMb)}`} />
        {r.gpuTotalMb > 0
          ? <Graph testid="monitor-gpu" label={r.gpuLabel || "GPU memory"} series={gpu} max={r.gpuTotalMb} unit={fmtMb} color="#fbbf24" current={`${fmtMb(r.gpuUsedMb)} / ${fmtMb(r.gpuTotalMb)}`} />
          : <div className="rounded border border-dashed border-line p-3 text-[12px] text-gray-500">No GPU telemetry{r.gpuLabel ? ` (${r.gpuLabel})` : ""}.</div>}
      </div>

      <h2 className="mb-1 mt-5 text-[12px] font-semibold uppercase tracking-wider text-gray-400">Running programs</h2>
      <table data-testid="monitor-runs" className="w-full border-collapse text-[12px]">
        <thead><tr className="text-left text-[10px] uppercase tracking-wider text-gray-500">
          <th className="py-1 pr-2 font-medium">App</th><th className="py-1 pr-2 font-medium">CPU</th><th className="py-1 pr-2 font-medium">RAM</th><th className="py-1 pr-2 font-medium">GPU mem</th><th className="py-1 pr-2 font-medium">Peak RAM</th><th className="py-1 pr-2 font-medium">History</th><th className="py-1 pr-2 font-medium">Status</th><th />
        </tr></thead>
        <tbody>
          {r.perRun.map((p) => {
            const ph = perRunHist[p.runId];
            const st = runs[p.runId]?.status ?? "running";
            return (
              <tr key={p.runId} data-testid="monitor-run" className="border-t border-line hover:bg-white/5">
                <td className="py-1.5 pr-2"><button className="text-accent hover:underline" onClick={() => openApp(p.appId)}>{name(p.appId)}</button></td>
                <td className="py-1.5 pr-2 font-mono">{p.cpuPct.toFixed(0)}%</td>
                <td className="py-1.5 pr-2 font-mono">{fmtMb(p.ramMb)}</td>
                <td className="py-1.5 pr-2 font-mono">{p.vramMb ? fmtMb(p.vramMb) : "–"}</td>
                <td className="py-1.5 pr-2 font-mono text-gray-400">{ph ? fmtMb(peak(ph.ram)) : "–"}</td>
                <td className="py-1.5 pr-2">{ph && <div className="flex gap-2"><Spark series={ph.cpu} max={100} color="#6ea8fe" /><Spark series={ph.ram} max={Math.max(1, peak(ph.ram))} color="#34d399" /></div>}</td>
                <td className="py-1.5 pr-2 text-gray-400">{st}</td>
                <td className="py-1.5 text-right"><Btn variant="danger" disabled={stopping === p.runId} onClick={() => stop(p)}>Stop</Btn></td>
              </tr>
            );
          })}
          {!r.perRun.length && <tr><td colSpan={8} className="py-3 text-gray-500">Nothing is running.</td></tr>}
        </tbody>
      </table>

      <h2 className="mb-1 mt-5 text-[12px] font-semibold uppercase tracking-wider text-gray-400">GPU admission queue</h2>
      <div data-testid="monitor-queue" className="rounded border border-line">
        {r.queue.map((q) => (
          <div key={q.runId} className="flex items-center gap-3 border-b border-line px-3 py-1.5 last:border-b-0">
            <div className="min-w-0 flex-1">
              <div className="truncate">{name(q.appId)} <span className="text-gray-500">needs {fmtMb(q.requiredVramMb)}</span></div>
              {q.reason && <div className="truncate text-[11px] text-gray-500">{q.reason}</div>}
            </div>
            <Btn onClick={() => ipc.forceRun(q.runId).catch((e) => notify(errMsg(e)))}>Run anyway</Btn>
          </div>
        ))}
        {!r.queue.length && <div className="px-3 py-2 text-[12px] text-gray-500">Queue is empty.</div>}
      </div>

      <h2 className="mb-1 mt-5 text-[12px] font-semibold uppercase tracking-wider text-gray-400">Listening ports</h2>
      <p className="mb-1 text-[11px] text-gray-500">Every TCP port in LISTEN state on this machine. Rows attributed to a Homemade run can be stopped here; an attributed row whose run is not active is an orphan.</p>
      <table data-testid="monitor-ports" className="w-full border-collapse text-[12px]">
        <thead><tr className="text-left text-[10px] uppercase tracking-wider text-gray-500">
          <th className="py-1 pr-2 font-medium">Port</th><th className="py-1 pr-2 font-medium">PID</th><th className="py-1 pr-2 font-medium">Command</th><th className="py-1 pr-2 font-medium">Homemade run</th><th />
        </tr></thead>
        <tbody>
          {(ports ?? []).map((p) => (
            <tr key={`${p.pid}:${p.port}`} data-testid="monitor-port" data-port={p.port} data-run={p.runId ?? ""} className="border-t border-line hover:bg-white/5">
              <td className="py-1 pr-2 font-mono">{p.port}</td>
              <td className="py-1 pr-2 font-mono text-gray-400">{p.pid}</td>
              <td className="py-1 pr-2 truncate">{p.command}</td>
              <td className="py-1 pr-2">
                {p.appId
                  ? <span className={p.active ? "text-emerald-300" : "text-amber-300"}>{name(p.appId)} · {p.active ? "active" : "orphan"}</span>
                  : <span className="text-gray-600">–</span>}
              </td>
              <td className="py-1 text-right">{p.runId && <Btn variant="danger" disabled={killing === p.pid} onClick={() => killPort(p)}>{p.active ? "Stop" : "Kill orphan"}</Btn>}</td>
            </tr>
          ))}
          {ports === null && <tr><td colSpan={5} className="py-3 text-gray-500">Scanning…</td></tr>}
          {ports && !ports.length && <tr><td colSpan={5} className="py-3 text-gray-500">Nothing is listening.</td></tr>}
        </tbody>
      </table>
    </div>
  );
}
