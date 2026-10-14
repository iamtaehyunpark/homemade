// OWNER: agent F. Artifact grid for an app: thumbnails, preview modal, reveal, scratch cleanup.
import { useEffect, useState } from "react";
import { assetUrl, ipc } from "../lib/ipc";
import type { ArtifactRecord } from "../lib/types";
import { useStore } from "../state/store";
import { basename, errMsg, fmtBytes, relTime } from "../lib/format";
import { Btn, Modal } from "./ui";
import { startDrag } from "@crabnebula/tauri-plugin-drag";

let dragIconCache: Record<string, string> = {};
/** Small PNG data URL used as the native drag preview (plugin requires data:image/png;base64). */
function dragIcon(label: string): string {
  if (dragIconCache[label]) return dragIconCache[label];
  const c = document.createElement("canvas");
  c.width = c.height = 64;
  const g = c.getContext("2d")!;
  g.fillStyle = "#1b1f27"; g.fillRect(4, 4, 56, 56);
  g.strokeStyle = "#6ea8fe"; g.lineWidth = 2; g.strokeRect(4, 4, 56, 56);
  g.fillStyle = "#e6e6e6"; g.font = "bold 14px sans-serif"; g.textAlign = "center"; g.textBaseline = "middle";
  g.fillText(label.slice(0, 5).toUpperCase(), 32, 32);
  return (dragIconCache[label] = c.toDataURL("image/png"));
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

export default function OutputShelf({ appId, runId }: { appId: string; runId: string | null }) {
  const tick = useStore((s) => s.artifactTick[appId] ?? 0);
  const notify = useStore((s) => s.notify);
  const [open, setOpen] = useState(true);
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

  const clean = async () => {
    try { const n = await ipc.cleanScratch(appId); notify(`Removed ${n} scratch file${n === 1 ? "" : "s"}`); setScratch(0); } catch (e) { notify(errMsg(e)); }
  };

  return (
    <div data-testid="output-shelf" className="shrink-0 border-t border-line bg-panel">
      <div className="flex items-center gap-2 px-3 py-1 text-[11px]">
        <button className="font-semibold uppercase tracking-wider text-gray-500 hover:text-gray-300" onClick={() => setOpen(!open)}>
          {open ? "▾" : "▸"} Outputs <span className="font-normal normal-case text-gray-600">({items.length})</span>
        </button>
        <div className="flex overflow-hidden rounded border border-line">
          {(["latest", "all"] as const).map((s) => (
            <button key={s} onClick={() => setScope(s)} className={`px-2 ${scope === s ? "bg-accent/15 text-accent" : "text-gray-500 hover:bg-white/5"}`}>{s === "latest" ? "This run" : "All"}</button>
          ))}
        </div>
        <div className="flex-1" />
        {scratch > 0 && <Btn data-testid="clean-scratch" className="!py-0 !text-[11px]" onClick={clean}>Clean scratch ({scratch} files)</Btn>}
      </div>
      {open && (
        <div className="flex max-h-44 gap-2 overflow-x-auto px-3 pb-2">
          {items.map((a) => <Tile key={a.id} a={a} onOpen={() => setPreview(a)} />)}
          {!items.length && <div className="py-3 text-[12px] text-gray-600">No outputs {scope === "latest" ? "for the latest run" : "yet"}.</div>}
        </div>
      )}
      {preview && <Preview a={preview} onClose={() => setPreview(null)} />}
    </div>
  );
}

function Tile({ a, onOpen }: { a: ArtifactRecord; onOpen: () => void }) {
  const k = kindOf(a);
  return (
    <button data-testid="shelf-item" data-path={a.filePath} onClick={onOpen} draggable title={a.filePath}
      onDragStart={(e) => {
        // Native OS drag-out (Finder, other apps) via tauri-plugin-drag; HTML data as a fallback for in-webview drops.
        e.dataTransfer.setData("text/uri-list", "file://" + a.filePath); e.dataTransfer.setData("text/plain", a.filePath);
        e.preventDefault();
        const ext = (a.filePath.split(".").pop() ?? "file");
        startDrag({ item: [a.filePath], icon: dragIcon(ext) }).catch((err) => console.warn("native drag failed", err));
      }}
      className="group flex w-28 shrink-0 flex-col overflow-hidden rounded border border-line bg-bg text-left hover:border-accent/60">
      <div className="flex h-20 items-center justify-center overflow-hidden bg-black/30">
        {k === "image" ? <img src={assetUrl(a.filePath)} loading="lazy" className="h-full w-full object-cover" alt="" />
          : k === "video" ? <video src={assetUrl(a.filePath)} muted preload="metadata" className="h-full w-full object-cover" />
          : <span className="text-2xl text-gray-600">{icon[k]}</span>}
      </div>
      <div className="px-1.5 py-1">
        <div className="truncate text-[11px]">{basename(a.filePath)}</div>
        <div className="text-[10px] text-gray-600">{fmtBytes(a.fileSizeBytes)} · {relTime(a.createdAt)}</div>
      </div>
    </button>
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
        <span>{basename(a.filePath)}</span><span className="text-[11px] font-normal text-gray-500">{fmtBytes(a.fileSizeBytes)}</span>
        <Btn onClick={() => ipc.revealInFinder(a.filePath).catch(() => {})}>Reveal in Finder</Btn>
      </div>}>
      <div className="flex min-h-40 items-center justify-center">
        {k === "image" && <img src={url} className="max-h-[75vh] max-w-full object-contain" alt="" />}
        {k === "video" && <video src={url} controls autoPlay className="max-h-[75vh] max-w-full" />}
        {k === "audio" && <audio src={url} controls autoPlay className="w-full" />}
        {k === "text" && <pre className="max-h-[75vh] w-full overflow-auto whitespace-pre-wrap rounded bg-bg p-3 font-mono text-[12px] select-text">{text ?? "Loading…"}</pre>}
        {k === "other" && <div className="text-gray-500">No preview for {a.mimeType || "this file type"}. <code className="block mt-2 text-gray-400">{a.filePath}</code></div>}
      </div>
    </Modal>
  );
}
