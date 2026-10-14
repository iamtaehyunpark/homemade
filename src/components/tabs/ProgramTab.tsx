// Underbar → Program: read-only raw-program facts with copy buttons.
import { useEffect, useState, type ReactNode } from "react";
import { ipc } from "../../lib/ipc";
import type { AppProcessInfo, AppRecord, RunRecord, StorageRef } from "../../lib/types";
import { useStore } from "../../state/store";
import { basename, errMsg, fmtBytes, fmtMb, isActive } from "../../lib/format";
import { dedupeVenv } from "../../lib/venv";
import { Btn } from "../ui";

const isMac = navigator.platform.toLowerCase().includes("mac");

function Copy({ text }: { text: string }) {
  const notify = useStore((s) => s.notify);
  return (
    <button className="shrink-0 rounded px-1 text-[10px] text-muted hover:bg-panel-2 hover:text-fg" title="Copy"
      onClick={() => navigator.clipboard.writeText(text).then(() => notify("Copied"), (e) => notify(errMsg(e)))}>Copy</button>
  );
}
function Row({ label, children, copy }: { label: string; children: ReactNode; copy?: string }) {
  return (<>
    <dt className="py-0.5 text-muted">{label}</dt>
    <dd className="flex min-w-0 items-center gap-1 py-0.5"><div className="min-w-0 flex-1">{children}</div>{copy != null && <Copy text={copy} />}</dd>
  </>);
}

export default function ProgramTab({ app, run, values, active }: { app: AppRecord; run: RunRecord | null; values: Record<string, unknown>; active: boolean }) {
  const m = app.manifest;
  const [cmd, setCmd] = useState<string>("");
  const [cmdErr, setCmdErr] = useState<string | null>(null);
  const [secrets, setSecrets] = useState<string[] | null>(null);
  const [refs, setRefs] = useState<StorageRef[] | null>(null);
  const [proc, setProc] = useState<AppProcessInfo | null>(null);
  const [deduping, setDeduping] = useState(false);
  const running = isActive(run?.status);
  const port = useStore((s) => (run ? s.ports[run.id] : undefined));
  const usage = useStore((s) => (run ? s.resources?.perRun.find((p) => p.runId === run.id) : undefined));

  useEffect(() => {
    if (!active) return;
    const t = setTimeout(() => {
      ipc.buildCliCommand(app.id, values).then((c) => { setCmd(c); setCmdErr(null); }).catch((e) => setCmdErr(errMsg(e)));
    }, 250);
    return () => clearTimeout(t);
  }, [app.id, app.manifestHash, values, active]);
  useEffect(() => {
    if (!active) return;
    ipc.listSecretNames().then(setSecrets).catch(() => setSecrets([]));
    ipc.storageRefs(app.id).then(setRefs).catch(() => setRefs([]));
  }, [app.id, active]);
  useEffect(() => {
    if (!active || !running) { setProc(null); return; }
    let alive = true;
    const poll = () => ipc.appProcessInfo(app.id).then((p) => alive && setProc(p)).catch(() => alive && setProc(null));
    poll();
    const t = setInterval(poll, 3000);
    return () => { alive = false; clearInterval(t); };
  }, [app.id, active, running, run?.id]);

  const cwd = m.app.cwd && m.app.cwd !== "." ? `${app.rootDir}/${m.app.cwd}` : app.rootDir;
  return (
    <div className="h-full overflow-y-auto px-3 py-2 text-[12px]">
      <div className="mb-2">
        <div className="mb-0.5 flex items-center gap-2 text-[11px] text-muted">Resolved command (current form values) {cmd && <Copy text={cmd} />}</div>
        <pre data-testid="program-command" className="whitespace-pre-wrap break-all rounded border border-line bg-bg px-2 py-1 font-mono text-[11.5px] select-text">{cmdErr ? <span className="text-danger">{cmdErr}</span> : cmd || "…"}</pre>
      </div>
      <dl className="grid grid-cols-[140px_1fr] gap-x-3">
        <Row label="Root dir" copy={app.rootDir}>
          <span className="flex items-center gap-2"><code className="truncate" title={app.rootDir}>{app.rootDir}</code>
            <button className="shrink-0 text-[11px] text-accent hover:underline" title="Reveal in Finder" onClick={() => ipc.revealInFinder(app.rootDir).catch(() => {})}>Reveal in Finder</button></span>
        </Row>
        <Row label="Working dir" copy={cwd}><code className="truncate">{cwd}</code></Row>
        <Row label="Exec" copy={m.app.exec}><code className="truncate">{m.app.exec}</code></Row>
        <Row label="Mode">{app.executionMode.replace("_", " ")}</Row>
        <Row label="Env manager">{m.app.env_manager || "–"}</Row>
        <Row label="Python / venv">
          {app.unmanagedVenvBytes > 0
            ? <span className="flex items-center gap-2"><span className="text-warn">Unmanaged venv · {fmtBytes(app.unmanagedVenvBytes)}</span>
                <Btn className="!py-0 !text-[11px]" disabled={deduping} title="Move this app's venv into the shared store"
                  onClick={async () => { setDeduping(true); await dedupeVenv(app); setDeduping(false); }}>{deduping ? "Deduplicating…" : "Deduplicate"}</Btn></span>
            : <span className="text-faint">No unmanaged venv</span>}
        </Row>
        <Row label="Resources">
          <span>{m.resources.requires_gpu ? "GPU required" : "No GPU"}
            {m.resources.vram_estimate_mb != null && ` · VRAM ≈ ${fmtMb(m.resources.vram_estimate_mb)}`}
            {m.resources.max_ram_mb != null && ` · RAM ≤ ${fmtMb(m.resources.max_ram_mb)}${isMac ? " (advisory)" : ""}`}
            {app.learnedPeakRamMb > 0 && <span className="text-faint"> · learned peak {fmtMb(app.learnedPeakRamMb)}</span>}</span>
        </Row>
        <Row label="Secrets">
          {m.environment.secrets.length
            ? <span className="flex flex-wrap gap-1.5">{m.environment.secrets.map((s) => {
                const have = secrets?.includes(s);
                return <code key={s} className={have ? "text-ok" : secrets ? "text-danger" : "text-muted"} title={have ? "Stored in the vault" : "Missing — add it in Settings"}>{secrets ? (have ? "✓ " : "✗ ") : ""}{s}</code>;
              })}</span>
            : <span className="text-faint">None required</span>}
        </Row>
        <Row label="Shared store refs">
          {refs === null ? <span className="text-faint">Loading…</span> : refs.length
            ? <ul className="space-y-px">{refs.map((r) => <li key={r.assetHash} className="flex gap-2"><code className="truncate" title={r.assetPath}>{basename(r.assetPath)}</code><span className="text-faint">{fmtBytes(r.sizeBytes)}</span></li>)}</ul>
            : <span className="text-faint">None</span>}
        </Row>
        <Row label="Process">
          {running
            ? <span className="font-mono">
                {proc ? `pid ${proc.pid} · pgid ${proc.pgid}` : "starting…"}
                {(port ?? proc?.port) != null && ` · port ${port ?? proc?.port}`}
                {usage && ` · CPU ${usage.cpuPct.toFixed(0)}% · RAM ${fmtMb(usage.ramMb)}${usage.vramMb ? ` · GPU ${fmtMb(usage.vramMb)}` : ""}`}
              </span>
            : <span className="text-faint">Not running</span>}
        </Row>
      </dl>
    </div>
  );
}
