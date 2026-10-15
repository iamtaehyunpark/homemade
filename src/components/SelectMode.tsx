// Select mode glue: syncs the store flag to the in-frame pickers, takes their picks, Esc cancels. Mounted once per window.
import { useEffect } from "react";
import { useStore } from "../state/store";
import Icon from "./Icon";
import { broadcastSelect, frameAppId, mention, mentionText, type FramePick } from "../lib/selectMode";

export default function SelectMode() {
  const on = useStore((s) => s.selectMode);
  const setOn = useStore((s) => s.setSelectMode);
  useEffect(() => {
    broadcastSelect(on);
    const msg = (e: MessageEvent) => {
      const d = e.data as { homemade?: string; desc?: FramePick } | null;
      if (!d || typeof d.homemade !== "string") return;
      const live = useStore.getState().selectMode;
      if (d.homemade === "picker-ready") { if (live) broadcastSelect(true, e.source as Window); }
      else if (!live) return;
      else if (d.homemade === "select-cancel") setOn(false);
      else if (d.homemade === "pick" && d.desc) { setOn(false); void mention(mentionText(d.desc, frameAppId(e.source)), frameAppId(e.source)); }
    };
    const key = (e: KeyboardEvent) => { if (e.key === "Escape" && useStore.getState().selectMode) { e.preventDefault(); setOn(false); } };
    window.addEventListener("message", msg);
    window.addEventListener("keydown", key, true);
    return () => { window.removeEventListener("message", msg); window.removeEventListener("keydown", key, true); };
  }, [on, setOn]);
  return null;
}

/** Picker toggle, shown in the bottom bar of webapp-mode apps. */
export function SelectButton() {
  const on = useStore((s) => s.selectMode);
  const setOn = useStore((s) => s.setSelectMode);
  return (
    <button data-testid="select-mode" aria-label="Select element in the program" aria-pressed={on} onClick={() => setOn(!on)}
      title={on ? "Click an element in the program to mention it to the agent · Esc cancels" : "Select an element of the program's UI to mention it in the agent session"}
      className={`inline-flex items-center gap-1.5 rounded px-1.5 py-0.5 text-[11px] ${on ? "bg-accent-soft text-accent" : "text-muted hover:text-fg"}`}>
      <Icon name="cursor" size={14} />{on ? "Selecting… Esc" : "Select"}
    </button>
  );
}
