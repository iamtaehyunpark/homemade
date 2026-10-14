// Git safety bar: init, checkpoint, log with revert.
import { useEffect, useState } from "react";
import { ipc } from "../lib/ipc";
import type { GitStatus } from "../lib/types";
import { errMsg, relTime } from "../lib/format";
import { useStore } from "../state/store";
import { Btn, Dropdown, Modal, PromptModal } from "./ui";

export default function GitBar({ appId, refreshKey }: { appId: string; refreshKey?: unknown }) {
  const notify = useStore((s) => s.notify);
  const [st, setSt] = useState<GitStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [asking, setAsking] = useState(false);
  const [revert, setRevert] = useState<{ sha: string; message: string } | null>(null);

  const load = () => ipc.gitStatus(appId).then(setSt).catch(() => setSt({ initialized: false, dirty: false, head: null, log: [] }));
  useEffect(() => { load(); }, [appId, refreshKey]);

  const act = async (f: () => Promise<unknown>) => {
    setBusy(true);
    try { await f(); } catch (e) { notify(errMsg(e)); }
    await load();
    setBusy(false);
  };

  if (!st) return null;
  if (!st.initialized) {
    return (
      <div className="flex items-center gap-2 text-[12px] text-gray-500">
        <span>Not versioned</span>
        <Btn disabled={busy} onClick={() => act(() => ipc.gitInit(appId))}>Track with Git</Btn>
      </div>
    );
  }
  return (
    <div className="flex items-center gap-2 text-[12px]">
      <span className="font-mono text-gray-400" title={st.head ?? ""}>⎇ {st.head?.slice(0, 7) ?? "no commits"}</span>
      {st.dirty && <span className="text-amber-400" title="Uncommitted changes">● modified</span>}
      <Btn disabled={busy || !st.dirty} onClick={() => setAsking(true)}>Checkpoint</Btn>
      <Dropdown label="History" align="right" disabled={!st.log.length}>
        {(close) => st.log.map((l) => (
          <div key={l.sha} className="flex items-center gap-2 px-3 py-1 hover:bg-white/5">
            <code className="text-gray-500">{l.sha.slice(0, 7)}</code>
            <span className="min-w-0 flex-1 truncate" title={l.message}>{l.message}</span>
            <span className="text-gray-600">{relTime(l.timestamp)}</span>
            {l.sha !== st.head && <Btn className="!py-0" onClick={() => { close(); setRevert(l); }}>Revert to</Btn>}
          </div>
        ))}
      </Dropdown>
      {asking && <PromptModal title="Create checkpoint" label="Message" initial="Checkpoint" okLabel="Commit" onClose={() => setAsking(false)} onOk={(m) => act(() => ipc.gitCheckpoint(appId, m))} />}
      {revert && (
        <Modal title="Revert app files?" onClose={() => setRevert(null)}>
          <p className="text-gray-300">Restore all files to <code>{revert.sha.slice(0, 7)}</code> “{revert.message}”?</p>
          <div className="mt-4 flex justify-end gap-2">
            <Btn onClick={() => setRevert(null)}>Cancel</Btn>
            <Btn variant="danger" onClick={() => { const r = revert; setRevert(null); act(() => ipc.gitRevertTo(appId, r.sha)); }}>Revert</Btn>
          </div>
        </Modal>
      )}
    </div>
  );
}
