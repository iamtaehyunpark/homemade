// Overlay for select mode: highlights the element under the pointer, click = pick → mention; Esc / second click on the
// button leaves the mode. Mounted once in App; toggled through the store (`selectMode`) or ⌘⇧C.
import { useEffect, useState } from "react";
import { useStore } from "../state/store";
import { describe, mention } from "../lib/selectMode";

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
      {/* iframes swallow pointer events: a transparent shield keeps the picker working over web UIs */}
      <div className="pointer-events-auto absolute inset-0" style={{ background: "transparent" }} />
      {box && (
        <>
          <div className="absolute rounded border-2 border-accent bg-accent/10 transition-all duration-75" style={{ left: box.x, top: box.y, width: box.w, height: box.h }} />
          <div className="absolute max-w-[60vw] truncate rounded bg-fg px-2 py-0.5 font-mono text-[11px] text-bg shadow"
            style={{ left: Math.min(box.x, window.innerWidth - 400), top: box.y > 28 ? box.y - 24 : box.y + box.h + 4 }}>{box.label}</div>
        </>
      )}
      <div className="absolute left-1/2 top-2 -translate-x-1/2 rounded-full bg-fg px-3 py-1 text-[12px] text-bg shadow">
        Select mode: click anything to mention it to the agent · Esc to cancel
      </div>
    </div>
  );
}

/** The toggle. Lives in the agent terminal toolbar and in the sidebar footer. */
export function SelectButton({ compact }: { compact?: boolean }) {
  const on = useStore((s) => s.selectMode);
  const setOn = useStore((s) => s.setSelectMode);
  return (
    <button data-testid="select-mode" aria-pressed={on} onClick={() => setOn(!on)}
      title="Select mode (⌘⇧C): click any element in the launcher to mention it in the agent session"
      className={`rounded border px-2 py-0.5 text-[12px] ${on ? "border-accent bg-accent text-white" : "border-line bg-panel hover:bg-panel-2"}`}>
      ⌖{compact ? "" : " Select"}
    </button>
  );
}
