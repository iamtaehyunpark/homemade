// Underbar → Runs: run history table; row click selects the run viewed in Logs/Outputs.
import { useEffect, useState } from "react";
import { ipc } from "../../lib/ipc";
import type { RunRecord } from "../../lib/types";
import { useStore } from "../../state/store";
import { errMsg, fmtMb, isActive, relTime, toMs } from "../../lib/format";

const statusColor: Record<string, string> = {
  running: "bg-ok/15 text-ok", stopping: "bg-warn/15 text-warn", queued_vram: "bg-warn/15 text-warn",
  success: "bg-accent/15 text-accent", failed: "bg-danger/15 text-danger", killed: "bg-panel-2 text-muted",
};

function duration(r: RunRecord): string {
  const end = r.finishedAt ? toMs(r.finishedAt) : isActive(r.status) ? Date.now() : null;
  if (!end) return "–";
  const s = Math.max(0, Math.round((end - toMs(r.startedAt)) / 1000));
  return s < 60 ? `${s}s` : s < 3600 ? `${Math.floor(s / 60)}m ${s % 60}s` : `${Math.floor(s / 3600)}h ${Math.floor((s % 3600) / 60)}m`;
}

export default function RunsTab({ appId, latest, selected, onSelect, onCount }: { appId: string; latest: RunRecord | null; selected: string | null; onSelect: (r: RunRecord) => void; onCount: (n: number) => void }) {
  const [runs, setRuns] = useState<RunRecord[]>([]);
  const live = useStore((s) => s.runs);
  const notify = useStore((s) => s.notify);
  useEffect(() => {
    ipc.listRuns(appId, 30).then(setRuns).catch(() => setRuns([]));
  }, [appId, latest?.id, latest?.status]);
  useEffect(() => { onCount(runs.length); }, [runs.length]);

  const rerun = async (r: RunRecord) => {
    let inputs: Record<string, unknown> = {};
    try { inputs = JSON.parse(r.inputsJson); } catch { /* empty */ }
    try { useStore.getState().upsertRun(await ipc.startRun(appId, inputs)); } catch (e) { notify(`Run failed: ${errMsg(e)}`); }
  };
  const copy = async (r: RunRecord) => { try { await navigator.clipboard.writeText(r.cliCommand); notify("CLI command copied"); } catch (e) { notify(errMsg(e)); } };

  return (
    <div data-testid="run-history" className="h-full overflow-y-auto">
      {runs.length > 0 ? (
        <table className="w-full border-collapse text-[11.5px]">
          <thead className="sticky top-0 bg-panel"><tr className="text-left text-[10px] uppercase tracking-wider text-muted">
            <th className="px-2 py-1 font-medium">Status</th><th className="px-2 py-1 font-medium">Started</th><th className="px-2 py-1 font-medium">Duration</th>
            <th className="px-2 py-1 font-medium">Exit</th><th className="px-2 py-1 font-medium">Peak RAM</th><th className="px-2 py-1 font-medium">Command</th><th />
          </tr></thead>
          <tbody>
            {runs.map((r0) => {
              const r = live[r0.id] ?? r0;
              const active = isActive(r.status);
              return (
                <tr key={r.id} data-testid="run-history-item" data-run-id={r.id} data-status={r.status} onClick={() => onSelect(r)}
                  className={`group cursor-default border-t border-line ${selected === r.id ? "bg-accent/10" : "hover:bg-panel-2"}`}>
                  <td className="px-2 py-0.5"><span className={`inline-block w-16 rounded px-1 text-center text-[10px] font-medium uppercase ${statusColor[r.status] ?? statusColor.killed}`}>{r.status === "queued_vram" ? "queued" : r.status}</span></td>
                  <td className="whitespace-nowrap px-2 py-0.5 text-muted" title={new Date(toMs(r.startedAt)).toLocaleString()}>{relTime(r.startedAt)}</td>
                  <td className="whitespace-nowrap px-2 py-0.5 font-mono text-muted">{duration(r)}</td>
                  <td className="px-2 py-0.5 font-mono text-muted">{r.exitCode ?? "–"}</td>
                  <td className="whitespace-nowrap px-2 py-0.5 font-mono text-muted">{r.peakRamMb ? fmtMb(r.peakRamMb) : "–"}</td>
                  <td className="max-w-0 w-full truncate px-2 py-0.5 font-mono text-faint" title={r.cliCommand}>{r.cliCommand}</td>
                  <td className="whitespace-nowrap px-2 py-0.5 text-right" onClick={(e) => e.stopPropagation()}>
                    <span className="invisible inline-flex gap-1 group-hover:visible">
                      {active
                        ? <button className="rounded px-1 text-danger hover:bg-danger/10" title="Stop this run" onClick={() => ipc.stopRun(r.id).catch((e) => notify(errMsg(e)))}>Stop</button>
                        : <button className="rounded px-1 text-accent hover:bg-accent/10" title="Run again with the same inputs" onClick={() => rerun(r)}>Re-run</button>}
                      <button className="rounded px-1 text-muted hover:bg-panel-2 hover:text-fg" title="Copy the command line" onClick={() => copy(r)}>Copy CLI</button>
                    </span>
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      ) : <div className="px-3 py-3 text-[12px] text-faint">No runs yet — press Run</div>}
    </div>
  );
}
