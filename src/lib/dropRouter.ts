// Routes native (OS) file drops from the Tauri webview to the DOM element under the cursor.
import { useEffect, useRef } from "react";
import { getCurrentWebview } from "@tauri-apps/api/webview";

type Handler = (paths: string[]) => void;
const targets = new Map<HTMLElement, Handler>();
let fallback: Handler | null = null;
let hoverEl: HTMLElement | null = null;
let started = false;

function targetAt(x: number, y: number): HTMLElement | null {
  const r = window.devicePixelRatio || 1;
  let el = document.elementFromPoint(x / r, y / r) as HTMLElement | null;
  while (el) { if (targets.has(el)) return el; el = el.parentElement; }
  return null;
}
function setHover(el: HTMLElement | null) {
  if (hoverEl === el) return;
  hoverEl?.removeAttribute("data-drop-hover");
  hoverEl = el;
  el?.setAttribute("data-drop-hover", "1");
}

export function startDropRouter() {
  if (started) return;
  started = true;
  try {
    getCurrentWebview().onDragDropEvent((e) => {
      const p = e.payload;
      if (p.type === "over" || p.type === "enter") setHover(targetAt(p.position.x, p.position.y));
      else if (p.type === "leave") setHover(null);
      else if (p.type === "drop") {
        const el = targetAt(p.position.x, p.position.y);
        setHover(null);
        if (!p.paths.length) return;
        if (el) targets.get(el)!(p.paths);
        else fallback?.(p.paths);
      }
    }).catch(() => {});
  } catch { /* not running inside Tauri */ }
}

export function setFallbackDrop(h: Handler | null) { fallback = h; }

/** Attach the returned ref to an element to receive OS file drops over it. */
export function useDropTarget<T extends HTMLElement>(handler: Handler) {
  const ref = useRef<T>(null);
  const h = useRef(handler);
  h.current = handler;
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    targets.set(el, (paths) => h.current(paths));
    return () => { targets.delete(el); };
  }, []);
  return ref;
}
