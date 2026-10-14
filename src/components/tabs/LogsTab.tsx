// Underbar → Logs: toolbar (viewed run, copy, clear view, filter, stderr-only) over the LogConsole.
import { useEffect, useState } from "react";
import { useLive } from "../../state/live";
import { useStore } from "../../state/store";
import { relTime } from "../../lib/format";
import { Btn } from "../ui";
import LogConsole from "../LogConsole";

export default function LogsTab({ runId, isLatest, onShowLatest }: { runId: string | null; isLatest: boolean; onShowLatest: () => void }) {
  const run = useStore((s) => (runId ? s.runs[runId] : undefined));
  const [filter, setFilter] = useState("");
  const [stderrOnly, setStderrOnly] = useState(false);
  const [cleared, setCleared] = useState<Record<string, number>>({});
  useEffect(() => { setFilter(""); }, [runId]);

  const copy = async () => {
    const lines = (runId && useLive.getState().runs[runId]?.lines) || [];
    try { await navigator.clipboard.writeText(lines.map((l) => l.text).join("\n")); useStore.getState().notify("Log copied"); } catch { /* ignore */ }
  };
  const clear = () => { if (runId) setCleared((c) => ({ ...c, [runId]: useLive.getState().runs[runId]?.lines.length ?? 0 })); };

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-line px-3 py-1 text-[11px]">
        <span className="text-muted">{runId ? (isLatest ? "Latest run" : "Selected run") : "No run"}</span>
        {runId && <code className="text-faint" title={runId}>{runId.slice(0, 8)}{run ? ` · ${relTime(run.startedAt)}` : ""}</code>}
        {runId && !isLatest && <button className="text-accent hover:underline" onClick={onShowLatest} title="Show the latest run">Show latest</button>}
        <div className="flex-1" />
        <input className="field w-40 !py-0 !text-[11px]" placeholder="Filter…" value={filter} onChange={(e) => setFilter(e.target.value)} title="Show only lines containing this text" />
        <label className="flex items-center gap-1 text-muted" title="Show only stderr lines"><input type="checkbox" checked={stderrOnly} onChange={(e) => setStderrOnly(e.target.checked)} /> stderr only</label>
        <Btn variant="ghost" className="!py-0 !text-[11px]" disabled={!runId} onClick={copy} title="Copy the full log to the clipboard">Copy log</Btn>
        <Btn variant="ghost" className="!py-0 !text-[11px]" disabled={!runId} onClick={clear} title="Hide the lines printed so far (does not delete the log)">Clear view</Btn>
      </div>
      <div className="min-h-0 flex-1"><LogConsole runId={runId} filter={filter} stderrOnly={stderrOnly} hideBefore={runId ? cleared[runId] ?? 0 : 0} /></div>
    </div>
  );
}
