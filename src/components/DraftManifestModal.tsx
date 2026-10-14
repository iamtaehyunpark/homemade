// Draft a homemade.toml for a folder with the local model (falls back to deterministic inference), edit, then apply.
import { useEffect, useState } from "react";
import { ipc } from "../lib/ipc";
import type { DraftResult } from "../lib/types";
import { errMsg } from "../lib/format";
import { useStore } from "../state/store";
import { Badge, Btn, Modal } from "./ui";

export default function DraftManifestModal() {
  const path = useStore((s) => s.draftPath);
  if (!path) return null;
  return <DraftBody key={path} path={path} />;
}

function DraftBody({ path }: { path: string }) {
  const close = () => useStore.getState().setDraftPath(null);
  const modelStatus = useStore((s) => s.modelStatus);
  const [draft, setDraft] = useState<DraftResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [toml, setToml] = useState("");
  const [tab, setTab] = useState<"manifest" | "prompt">("manifest");
  const [applying, setApplying] = useState(false);

  useEffect(() => {
    let alive = true;
    ipc.draftManifest(path).then((d) => { if (alive) { setDraft(d); setToml(d.toml); } }).catch((e) => alive && setError(errMsg(e)));
    return () => { alive = false; };
  }, [path]);

  const apply = async () => {
    setApplying(true);
    const st = useStore.getState();
    try {
      const app = await ipc.applyManifest(path, toml);
      st.upsertApp(app);
      st.openApp(app.id);
      st.notify(`Manifest applied: ${app.name}`);
      close();
    } catch (e) { st.notify(`Apply failed: ${errMsg(e)}`); setApplying(false); }
  };
  const copyPrompt = async () => {
    try { await navigator.clipboard.writeText(draft?.agentPrompt ?? ""); useStore.getState().notify("Agent prompt copied"); } catch (e) { useStore.getState().notify(errMsg(e)); }
  };

  const tabCls = (t: string) => `px-2 py-1 text-[12px] border-b-2 ${tab === t ? "border-accent text-accent" : "border-transparent text-gray-400 hover:text-gray-200"}`;

  return (
    <Modal testid="draft-modal" wide title={<span>Draft manifest · <span className="font-mono text-[12px] text-gray-400">{path}</span></span>} onClose={close}>
      {modelStatus?.available === false && (
        <div className="mb-2 rounded border border-amber-500/30 bg-amber-500/10 px-2 py-1 text-[11px] text-amber-300">
          Local model unavailable{modelStatus.reason ? ` (${modelStatus.reason})` : ""} — using deterministic inference.
        </div>
      )}
      {!draft && !error && (
        <div data-testid="draft-loading" className="flex items-center gap-2 py-10 justify-center text-gray-400">
          <span className="h-4 w-4 animate-spin rounded-full border-2 border-accent border-t-transparent" />
          Reading README and docs…
        </div>
      )}
      {error && <div data-testid="draft-error" className="py-4 text-red-400">Draft failed: {error}</div>}
      {draft && (<>
        <div className="mb-2 flex items-center gap-1 border-b border-line">
          <button data-testid="draft-tab-manifest" className={tabCls("manifest")} onClick={() => setTab("manifest")}>Manifest</button>
          <button data-testid="draft-tab-prompt" className={tabCls("prompt")} onClick={() => setTab("prompt")}>Agent prompt</button>
          <div className="flex-1" />
          <span data-testid="draft-used-model" data-used={draft.usedModel ? "true" : "false"}>
            <Badge color={draft.usedModel ? "accent" : "gray"}>{draft.usedModel ? "used model" : "rule-based"}</Badge>
          </span>
        </div>
        <div className={tab === "manifest" ? "" : "hidden"}>
          <textarea data-testid="draft-toml" spellCheck={false} className="field h-[50vh] w-full resize-y font-mono text-[11.5px] leading-[1.45]" value={toml} onChange={(e) => setToml(e.target.value)} />
          {draft.validationErrors.length > 0 && (
            <ul data-testid="draft-errors" className="mt-2 list-disc space-y-0.5 pl-5 text-[11px] text-red-400">
              {draft.validationErrors.map((e, i) => <li key={i}>{e}</li>)}
            </ul>
          )}
          {draft.notes.length > 0 && (
            <ul data-testid="draft-notes" className="mt-2 list-disc space-y-0.5 pl-5 text-[11px] text-gray-500">
              {draft.notes.map((n, i) => <li key={i}>{n}</li>)}
            </ul>
          )}
        </div>
        <div className={tab === "prompt" ? "" : "hidden"}>
          <div className="mb-1 flex justify-end"><Btn data-testid="draft-copy-prompt" onClick={copyPrompt}>Copy</Btn></div>
          <pre data-testid="draft-prompt" className="h-[50vh] overflow-auto whitespace-pre-wrap rounded border border-line bg-bg p-2 font-mono text-[11px] text-gray-300 select-text">{draft.agentPrompt}</pre>
        </div>
      </>)}
      <div className="mt-3 flex justify-end gap-2">
        <Btn onClick={close}>Cancel</Btn>
        <Btn data-testid="draft-apply" variant="primary" disabled={!draft || !toml.trim() || applying} onClick={apply}>{applying ? "Applying…" : "Apply & reload"}</Btn>
      </div>
    </Modal>
  );
}
