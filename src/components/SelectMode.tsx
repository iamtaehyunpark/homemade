// Overlay for select mode: highlights the element under the pointer, click = pick → mention; Esc / second click on the
// button leaves the mode. Mounted once in App; toggled through the store (`selectMode`) or ⌘⇧C.
import { useEffect, useState } from "react";
import { useStore } from "../state/store";
import Icon from "./Icon";
import { broadcastSelect, describe, describeFrame, frameAppId, mention, type FramePick } from "../lib/selectMode";

export default function SelectMode() {
  const on = useStore((s) => s.selectMode);
  const setOn = useStore((s) => s.setSelectMode);
  const [box, setBox] = useState<{ x: number; y: number; w: number; h: number; label: string } | null>(null);

  useEffect(() => {
    const key = (e: KeyboardEvent) => {
      if ((e.metaKey || e.ctrlKey) && e.shiftKey && e.key.toLowerCase() === "c") { e.preventDefault(); setOn(!useStore.getState().selectMode); }
      else if (e.key === "Escape" && useStore.getState().selectMode) { e.preventDefault(); setOn(false); }
    };
    window.addEventListener("keydown", key, true);
    return () => window.removeEventListener("keydown", key, true);
  }, [setOn]);

  // Program frames run their own picker (src-tauri/src/picker.js): keep them in sync and take their picks.
  useEffect(() => {
    broadcastSelect(on);
    const msg = (e: MessageEvent) => {
      const d = e.data as { homemade?: string; on?: boolean; desc?: FramePick } | null;
      if (!d || typeof d.homemade !== "string") return;
      const live = useStore.getState().selectMode;
      if (d.homemade === "picker-ready") { if (live) broadcastSelect(true, e.source as Window); return; }
      if (!live) return;
      if (d.homemade === "hover") setBox(null);
      else if (d.homemade === "select-cancel") setOn(false);
      else if (d.homemade === "pick" && d.desc) { setOn(false); void mention(describeFrame(d.desc, frameAppId(e.source))); }
    };
    window.addEventListener("message", msg);
    return () => window.removeEventListener("message", msg);
  }, [on, setOn]);

  useEffect(() => {
    if (!on) { setBox(null); return; }
    const under = (x: number, y: number) => {
      const els = document.elementsFromPoint(x, y);
      return els.find((e) => !(e as HTMLElement).closest("[data-select-overlay]")) ?? null;
    };
    const move = (e: MouseEvent) => {
      const el = under(e.clientX, e.clientY);
      if (!el) { setBox(null); return; }
      const d = describe(el, e.clientX, e.clientY);
      setBox({ ...d.rect, label: d.mention });
    };
    const click = (e: MouseEvent) => {
      e.preventDefault(); e.stopPropagation();
      const el = under(e.clientX, e.clientY);
      setOn(false);
      if (el) void mention(describe(el, e.clientX, e.clientY));
    };
    const swallow = (e: Event) => { e.preventDefault(); e.stopPropagation(); };
    document.addEventListener("mousemove", move, true);
    document.addEventListener("click", click, true);
    document.addEventListener("mousedown", swallow, true);
    document.addEventListener("mouseup", swallow, true);
    document.body.style.cursor = "crosshair";
    return () => {
      document.removeEventListener("mousemove", move, true);
      document.removeEventListener("click", click, true);
      document.removeEventListener("mousedown", swallow, true);
      document.removeEventListener("mouseup", swallow, true);
      document.body.style.cursor = "";
    };
  }, [on, setOn]);

  if (!on) return null;
  return (
    <div data-select-overlay data-testid="select-overlay" className="pointer-events-none fixed inset-0 z-[100]">
      {box && (
        <>
          <div className="absolute rounded border-2 border-accent bg-accent/10 transition-all duration-75" style={{ left: box.x, top: box.y, width: box.w, height: box.h }} />
          <div className="absolute max-w-[60vw] truncate rounded bg-fg px-2 py-0.5 font-mono text-[11px] text-bg shadow"
            style={{ left: Math.min(box.x, window.innerWidth - 400), top: box.y > 28 ? box.y - 24 : box.y + box.h + 4 }}>{box.label}</div>
        </>
      )}
      <div className="absolute left-1/2 top-2 -translate-x-1/2 rounded-full bg-fg px-3 py-1 text-[12px] text-bg shadow">
        Select mode: click any element of the program (or the launcher) to mention it to the agent · Esc to cancel
      </div>
    </div>
  );
}

/** Element picker in the development toolbar. */
export function SelectButton({ compact }: { compact?: boolean }) {
  const on = useStore((s) => s.selectMode);
  const setOn = useStore((s) => s.setSelectMode);
  return (
    <button data-testid="select-mode" aria-label="Select element for agent" aria-pressed={on} onClick={() => setOn(!on)}
      title="Select an element to reference in your agent session (⌘⇧C). Press Esc to cancel."
      className={`inline-flex items-center gap-2 rounded-md border px-2.5 py-1 text-[11px] ${on ? "border-accent bg-accent-soft text-accent" : "border-transparent text-muted hover:border-line hover:bg-panel-2 hover:text-fg"}`}>
      <Icon name="cursor" size={15} />{!compact && <span>{on ? "Selecting…" : "Select element"}</span>}
      {on && <kbd className="rounded border border-accent/30 px-1 text-[9px]">Esc</kbd>}
    </button>
  );
}
