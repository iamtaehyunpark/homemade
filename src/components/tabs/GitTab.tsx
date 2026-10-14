// Underbar → Git: working tree (dirty files, checkpoint, init) + commit graph with revert.
import { useEffect, useState } from "react";
import { ipc } from "../../lib/ipc";
import type { GitLogEntry, GitStatus } from "../../lib/types";
import { errMsg, relTime } from "../../lib/format";
import { useStore } from "../../state/store";
import { Btn, Modal, PromptModal } from "../ui";

const EMPTY: GitStatus = { initialized: false, dirty: false, head: null, log: [], dirtyFiles: [] };
const ROW = 26;

function checkpointTag(msg: string): string | null {
  if (!msg.startsWith("homemade:")) return null;
  const m = msg.toLowerCase();
  if (m.includes("pre-revert") || m.includes("before revert")) return "pre-revert";
  if (m.includes("before agent") || m.includes("before-agent")) return "before agent";
  if (m.includes("after agent") || m.includes("after-agent")) return "after agent";
  return "checkpoint";
}

function Lane({ first, last, merge, head }: { first: boolean; last: boolean; merge: boolean; head: boolean }) {
  const c = ROW / 2;
  return (
    <svg width={18} height={ROW} className="shrink-0 text-accent" aria-hidden>
      {!first && <line x1={9} y1={0} x2={9} y2={c} stroke="currentColor" strokeWidth={1.5} />}
      {!last && <line x1={9} y1={c} x2={9} y2={ROW} stroke="currentColor" strokeWidth={1.5} />}
      {merge && <path d={`M9 ${c} C 16 ${c + 4}, 16 ${ROW - 2}, 16 ${ROW}`} fill="none" stroke="currentColor" strokeOpacity={0.35} strokeWidth={1.5} />}
      <circle cx={9} cy={c} r={head ? 4.5 : 3.5} className={head ? "fill-accent" : "fill-panel"} stroke="currentColor" strokeWidth={1.5} />
    </svg>
  );
}

export default function GitTab({ appId, refreshKey, onStatus }: { appId: string; refreshKey?: unknown; onStatus: (s: GitStatus) => void }) {
  const notify = useStore((s) => s.notify);
  const [st, setSt] = useState<GitStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [asking, setAsking] = useState(false);
  const [revert, setRevert] = useState<GitLogEntry | null>(null);

  const load = () => ipc.gitStatus(appId).then((s) => ({ ...EMPTY, ...s, dirtyFiles: s.dirtyFiles ?? [], log: s.log ?? [] })).catch(() => EMPTY).then((s) => { setSt(s); onStatus(s); });
  useEffect(() => { load(); }, [appId, refreshKey]);

  const act = async (f: () => Promise<unknown>) => {
    setBusy(true);
    try { await f(); } catch (e) { notify(errMsg(e)); }
    await load();
    setBusy(false);
  };

  if (!st) return <div className="px-3 py-3 text-[12px] text-faint">Loading…</div>;
  return (
    <div className="flex h-full min-h-0 flex-col text-[12px]">
      <div className="shrink-0 border-b border-line px-3 py-2">
        {!st.initialized ? (
          <div className="flex items-center gap-3">
            <span className="text-muted">Not tracked with Git — track it to get checkpoints and one-click revert.</span>
            <Btn data-testid="git-init" disabled={busy} onClick={() => act(() => ipc.gitInit(appId))}>Track with Git</Btn>
          </div>
        ) : (<>
          <div className="flex items-center gap-2">
            <span className="font-mono text-muted" title={st.head ?? ""}>⎇ {st.head?.slice(0, 7) ?? "no commits"}</span>
            {st.dirty ? <span className="text-warn" title="Uncommitted changes">● {st.dirtyFiles.length || ""} modified</span> : <span className="text-faint">Working tree clean</span>}
            <div className="flex-1" />
            <span className="text-[11px] text-faint">Agent sessions are checkpointed automatically.</span>
            <Btn variant="ghost" onClick={() => load()} title="Refresh Git status">↻</Btn>
            <Btn data-testid="git-checkpoint" disabled={busy || !st.dirty} onClick={() => setAsking(true)} title="Commit all changes as a checkpoint">Checkpoint</Btn>
          </div>
          {st.dirtyFiles.length > 0 && (
            <ul data-testid="git-dirty-files" className="mt-1.5 max-h-24 overflow-y-auto rounded border border-line bg-bg font-mono text-[11px]">
              {st.dirtyFiles.map((f) => (
                <li key={f.path} className="flex gap-2 px-2 py-px" title={f.path}>
                  <span className="w-5 shrink-0 text-warn">{f.status.trim() || "?"}</span><span className="truncate">{f.path}</span>
                </li>
              ))}
            </ul>
          )}
        </>)}
      </div>
      <div data-testid="git-log" className="min-h-0 flex-1 overflow-y-auto px-2 py-1">
        {st.initialized && !st.log.length && <div className="px-1 py-2 text-faint">No commits yet</div>}
        {st.log.map((l, i) => {
          const isHead = l.sha === st.head;
          const tag = checkpointTag(l.message);
          const refs = (l.refs ?? []).filter((r) => r && r !== "HEAD");
          return (
            <div key={l.sha} data-testid="git-commit" data-sha={l.sha} className="group flex items-center gap-2 rounded pr-1 hover:bg-panel-2" style={{ height: ROW }}>
              <Lane first={i === 0} last={i === st.log.length - 1} merge={(l.parents?.length ?? 0) > 1} head={isHead} />
              <code className="shrink-0 text-faint" title={l.sha}>{l.sha.slice(0, 7)}</code>
              {isHead && <span className="shrink-0 rounded bg-accent px-1 text-[10px] font-medium text-on-accent">HEAD</span>}
              {refs.map((r) => <span key={r} className="shrink-0 rounded border border-accent/40 px-1 text-[10px] text-accent">{r}</span>)}
              {tag && <span className="shrink-0 rounded bg-panel-2 px-1 text-[10px] text-muted">{tag}</span>}
              <span className="min-w-0 flex-1 truncate" title={`${l.message}${l.author ? `\n— ${l.author}` : ""}`}>{l.message}</span>
              <span className="shrink-0 text-faint">{relTime(l.timestamp)}</span>
              {!isHead && <Btn data-testid={`git-revert-${l.sha}`} className="invisible !py-0 group-hover:visible" title="Restore all app files to this commit" onClick={() => setRevert(l)}>Revert to</Btn>}
            </div>
          );
        })}
      </div>
      {asking && <PromptModal title="Create checkpoint" label="Message" initial="Checkpoint" okLabel="Commit" onClose={() => setAsking(false)} onOk={(m) => act(() => ipc.gitCheckpoint(appId, m))} />}
      {revert && (
        <Modal title="Revert app files?" onClose={() => setRevert(null)}>
          <p className="text-fg">Restore all files to <code>{revert.sha.slice(0, 7)}</code> “{revert.message}”?</p>
          <div className="mt-4 flex justify-end gap-2">
            <Btn onClick={() => setRevert(null)}>Cancel</Btn>
            <Btn variant="danger" onClick={() => { const r = revert; setRevert(null); act(() => ipc.gitRevertTo(appId, r.sha)); }}>Revert</Btn>
          </div>
        </Modal>
      )}
    </div>
  );
}
