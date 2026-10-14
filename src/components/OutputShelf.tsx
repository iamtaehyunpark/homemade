// Underbar → Outputs: artifact grid for an app (thumbnails, preview modal, reveal, drag-out, scratch cleanup).
import { useEffect, useState } from "react";
import { assetUrl, ipc } from "../lib/ipc";
import type { ArtifactRecord } from "../lib/types";
import { useStore } from "../state/store";
import { basename, errMsg, fmtBytes, relTime } from "../lib/format";
import { Btn, Modal } from "./ui";
import { startDrag } from "@crabnebula/tauri-plugin-drag";

import { cssVar } from "../lib/theme";

let dragIconCache: Record<string, string> = {};
/** Small PNG data URL used as the native drag preview (plugin requires data:image/png;base64). */
function dragIcon(label: string): string {
  const key = label + document.documentElement.dataset.theme;
  if (dragIconCache[key]) return dragIconCache[key];
  const c = document.createElement("canvas");
  c.width = c.height = 64;
  const g = c.getContext("2d")!;
  g.fillStyle = cssVar("--color-panel-2") || "#eef0f4"; g.fillRect(4, 4, 56, 56);
  g.strokeStyle = cssVar("--color-accent") || "#2f6fed"; g.lineWidth = 2; g.strokeRect(4, 4, 56, 56);
  g.fillStyle = cssVar("--color-fg") || "#1b1f27"; g.font = "bold 14px sans-serif"; g.textAlign = "center"; g.textBaseline = "middle";
  g.fillText(label.slice(0, 5).toUpperCase(), 32, 32);
  return (dragIconCache[key] = c.toDataURL("image/png"));
}

const kindOf = (a: ArtifactRecord) => {
  const m = a.mimeType || "";
  if (m.startsWith("image/")) return "image";
  if (m.startsWith("video/")) return "video";
  if (m.startsWith("audio/")) return "audio";
  if (m.startsWith("text/") || /json|xml|csv|yaml|toml|javascript/.test(m) || /\.(txt|md|log|csv|json|ya?ml|toml|py|js|ts)$/i.test(a.filePath)) return "text";
  return "other";
};
const icon: Record<string, string> = { audio: "♪", text: "≡", other: "▢", image: "▣", video: "▶" };

export default function OutputShelf({ appId, runId, onCount }: { appId: string; runId: string | null; onCount?: (n: number) => void }) {
  const tick = useStore((s) => s.artifactTick[appId] ?? 0);
  const notify = useStore((s) => s.notify);
  const [scope, setScope] = useState<"latest" | "all">("latest");
  const [items, setItems] = useState<ArtifactRecord[]>([]);
  const [scratch, setScratch] = useState(0);
  const [preview, setPreview] = useState<ArtifactRecord | null>(null);

  useEffect(() => {
    const rid = scope === "latest" ? runId ?? undefined : undefined;
    if (scope === "latest" && !runId) { setItems([]); } else {
      ipc.listArtifacts(appId, rid, "output").then((a) => setItems([...a].sort((x, y) => y.createdAt - x.createdAt))).catch(() => setItems([]));
    }
    ipc.listArtifacts(appId, undefined, "scratch").then((a) => setScratch(a.length)).catch(() => setScratch(0));
  }, [appId, runId, scope, tick]);
  useEffect(() => { onCount?.(items.length); }, [items.length]);

  const clean = async () => {
    try { const n = await ipc.cleanScratch(appId); notify(`Removed ${n} scratch file${n === 1 ? "" : "s"}`); setScratch(0); } catch (e) { notify(errMsg(e)); }
  };

  return (
    <div data-testid="output-shelf" className="flex h-full min-h-0 flex-col bg-panel">
      <div className="flex shrink-0 items-center gap-2 px-3 py-1 text-[11px]">
        <div className="flex overflow-hidden rounded border border-line">
          {(["latest", "all"] as const).map((s) => (
            <button key={s} onClick={() => setScope(s)} className={`px-2 ${scope === s ? "bg-accent/15 text-accent" : "text-muted hover:bg-panel-2"}`}
              title={s === "latest" ? "Outputs of the run selected in Runs (latest by default)" : "Outputs of every run"}>{s === "latest" ? "This run" : "All runs"}</button>
          ))}
        </div>
        <span className="text-faint">{items.length} file{items.length === 1 ? "" : "s"}</span>
        <div className="flex-1" />
        {scratch > 0 && <Btn data-testid="clean-scratch" className="!py-0 !text-[11px]" onClick={clean} title="Delete scratch files matched by artifacts.scratch_globs">Clean scratch ({scratch} files)</Btn>}
      </div>
      <div className="min-h-0 flex-1 overflow-y-auto px-3 pb-2">
        {items.length > 0
          ? <div className="grid grid-cols-[repeat(auto-fill,minmax(112px,1fr))] gap-2">{items.map((a) => <Tile key={a.id} a={a} onOpen={() => setPreview(a)} />)}</div>
          : <div className="py-3 text-[12px] text-faint">{scope === "latest" ? (runId ? "No outputs for this run" : "No runs yet — press Run") : "No outputs yet"}</div>}
      </div>
      {preview && <Preview a={preview} onClose={() => setPreview(null)} />}
    </div>
  );
}

function Tile({ a, onOpen }: { a: ArtifactRecord; onOpen: () => void }) {
  const k = kindOf(a);
  return (
    <div role="button" tabIndex={0} data-testid="shelf-item" data-path={a.filePath} onClick={onOpen} onKeyDown={(e) => e.key === "Enter" && onOpen()} draggable title={a.filePath}
      onDragStart={(e) => {
        // Native OS drag-out (Finder, other apps) via tauri-plugin-drag; HTML data as a fallback for in-webview drops.
        e.dataTransfer.setData("text/uri-list", "file://" + a.filePath); e.dataTransfer.setData("text/plain", a.filePath);
        e.preventDefault();
        const ext = (a.filePath.split(".").pop() ?? "file");
        startDrag({ item: [a.filePath], icon: dragIcon(ext) }).catch((err) => console.warn("native drag failed", err));
      }}
      className="group relative flex cursor-default flex-col overflow-hidden rounded border border-line bg-bg text-left hover:border-accent/60">
      <div className="flex h-20 items-center justify-center overflow-hidden bg-panel-2">
        {k === "image" ? <img src={assetUrl(a.filePath)} loading="lazy" className="h-full w-full object-cover" alt="" />
          : k === "video" ? <video src={assetUrl(a.filePath)} muted preload="metadata" className="h-full w-full object-cover" />
          : <span className="text-2xl text-faint">{icon[k]}</span>}
      </div>
      <div className="px-1.5 py-1">
        <div className="truncate text-[11px]">{basename(a.filePath)}</div>
        <div className="text-[10px] text-faint">{fmtBytes(a.fileSizeBytes)} · {relTime(a.createdAt)}</div>
      </div>
      <button className="absolute right-1 top-1 hidden rounded border border-line bg-panel px-1 text-[10px] text-muted hover:text-fg group-hover:block" title="Reveal in Finder"
        onClick={(e) => { e.stopPropagation(); ipc.revealInFinder(a.filePath).catch(() => {}); }}>Reveal</button>
    </div>
  );
}

function Preview({ a, onClose }: { a: ArtifactRecord; onClose: () => void }) {
  const k = kindOf(a);
  const [text, setText] = useState<string | null>(null);
  useEffect(() => {
    if (k !== "text") return;
    if (a.fileSizeBytes > 2 * 1024 * 1024) { setText("File too large to preview."); return; }
    ipc.readTextFile(a.filePath).then(setText).catch((e) => setText(`Could not read file: ${errMsg(e)}`));
  }, [a.filePath]);
  const url = assetUrl(a.filePath);
  return (
    <Modal wide onClose={onClose} title={
      <div className="flex items-center gap-3">
        <span>{basename(a.filePath)}</span><span className="text-[11px] font-normal text-muted">{fmtBytes(a.fileSizeBytes)}</span>
        <Btn onClick={() => ipc.revealInFinder(a.filePath).catch(() => {})}>Reveal in Finder</Btn>
      </div>}>
      <div className="flex min-h-40 items-center justify-center">
        {k === "image" && <img src={url} className="max-h-[75vh] max-w-full object-contain" alt="" />}
        {k === "video" && <video src={url} controls autoPlay className="max-h-[75vh] max-w-full" />}
        {k === "audio" && <audio src={url} controls autoPlay className="w-full" />}
        {k === "text" && <pre className="max-h-[75vh] w-full overflow-auto whitespace-pre-wrap rounded bg-bg p-3 font-mono text-[12px] select-text">{text ?? "Loading…"}</pre>}
        {k === "other" && <div className="text-muted">No preview for {a.mimeType || "this file type"}. <code className="block mt-2 text-muted">{a.filePath}</code></div>}
      </div>
    </Modal>
  );
}
