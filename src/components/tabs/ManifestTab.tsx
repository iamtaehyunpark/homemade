// Underbar → Manifest: homemade.toml editor (validate + write + reload via save_manifest).
import { useEffect, useState } from "react";
import { ipc } from "../../lib/ipc";
import type { AppRecord } from "../../lib/types";
import { errMsg } from "../../lib/format";
import { useStore } from "../../state/store";
import { Btn } from "../ui";

export default function ManifestTab({ app, active }: { app: AppRecord; active: boolean }) {
  const path = `${app.rootDir}/homemade.toml`;
  const [text, setText] = useState("");
  const [orig, setOrig] = useState<string | null>(null);
  const [onDisk, setOnDisk] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [loaded, setLoaded] = useState(false);

  const load = () => ipc.readTextFile(path).then((t) => { setText(t); setOrig(t); setOnDisk(true); setError(null); })
    .catch(() => { setOnDisk(false); setOrig(""); setText(""); })
    .finally(() => setLoaded(true));
  useEffect(() => { if (active && !loaded) load(); }, [active, loaded]);
  useEffect(() => { if (loaded && text === orig) load(); }, [app.manifestHash]); // refresh if unmodified

  const save = async () => {
    setSaving(true);
    const st = useStore.getState();
    try { const a = await ipc.saveManifest(app.id, text); st.upsertApp(a); setOrig(text); setOnDisk(true); setError(null); st.notify("Manifest saved"); }
    catch (e) { setError(errMsg(e)); }
    setSaving(false);
  };
  const dirty = orig !== null && text !== orig;

  return (
    <div className="flex h-full min-h-0 flex-col">
      <div className="flex shrink-0 items-center gap-2 border-b border-line px-3 py-1 text-[11px]">
        <code className="truncate text-muted" title={path}>homemade.toml</code>
        {!onDisk && <span className="rounded bg-warn/15 px-1 text-warn" title="Homemade inferred this app's manifest; no homemade.toml exists on disk">inferred — not written</span>}
        {dirty && <span className="text-warn">● unsaved</span>}
        <div className="flex-1" />
        <Btn variant="ghost" className="!py-0 !text-[11px]" onClick={() => useStore.getState().setDraftPath(app.rootDir)} title="Draft a homemade.toml with the local model">✨ Draft with local model</Btn>
        <Btn variant="ghost" className="!py-0 !text-[11px]" onClick={() => { setLoaded(false); }} title="Discard edits and reload homemade.toml from disk">Reload from disk</Btn>
        <Btn variant="primary" className="!py-0 !text-[11px]" disabled={saving || !text.trim() || !dirty} onClick={save} title="Validate, write and reload (⌘S)">{saving ? "Saving…" : "Save"}</Btn>
      </div>
      {error && <div data-testid="manifest-error" className="shrink-0 border-b border-danger/30 bg-danger/10 px-3 py-1 text-[11px] text-danger select-text">{error}</div>}
      <textarea data-testid="manifest-editor" spellCheck={false} value={text} onChange={(e) => setText(e.target.value)}
        onKeyDown={(e) => { if ((e.metaKey || e.ctrlKey) && e.key === "s") { e.preventDefault(); if (dirty) save(); } }}
        placeholder={onDisk ? "" : "No homemade.toml on disk. Write one here and Save, or draft it with the local model."}
        className="min-h-0 flex-1 resize-none border-0 bg-bg px-3 py-2 font-mono text-[11.5px] leading-[1.45] text-fg outline-none" />
    </div>
  );
}
