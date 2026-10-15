// Select mode: a devtools-style element cursor *inside the program's web UI*. The picker itself lives in every program
// frame (src-tauri/src/picker.js); this side only switches it on/off and turns its pick into a mention typed into the
// app's agent session (no Enter), so the user can point at a button and continue with "…make this blue".
import { ipc } from "./ipc";
import { useStore } from "../state/store";

/** What the in-frame picker reports for an element. */
export interface FramePick {
  tag: string; attrs: string[]; text: string; path: string; component?: string; source?: string; page: string; title: string;
}

/** `[ui: App › /page › button#save "Save" ‹Form› @ src/Form.tsx:12 — selector: … (app id)]` */
export function mentionText(d: FramePick, appId: string | null): string {
  const app = appId ? useStore.getState().apps.find((a) => a.id === appId) : null;
  const where = `${app ? app.name + " › " : ""}${d.page && d.page !== "/" ? d.page + " › " : ""}`;
  const el = `${d.tag}${d.attrs.join("")}${d.text ? ` "${d.text}"` : ""}`;
  const fw = d.component ? ` ‹${d.component}›${d.source ? ` @ ${d.source}` : ""}` : "";
  return `[ui: ${where}${el}${fw}${d.path ? ` — selector: ${d.path}` : ""}${appId ? ` (app ${appId})` : ""}]`;
}

/** Tell program frames whether select mode is on. */
export function broadcastSelect(on: boolean, only?: Window | null) {
  for (const f of Array.from(document.querySelectorAll("iframe"))) {
    const w = f.contentWindow;
    if (w && (!only || w === only)) w.postMessage({ homemade: "select", on }, "*");
  }
}

/** App that owns the frame a picker message came from. */
export function frameAppId(source: MessageEventSource | null): string | null {
  const f = Array.from(document.querySelectorAll("iframe")).find((i) => i.contentWindow === source);
  return f?.closest<HTMLElement>("[data-app-panel]")?.dataset.appPanel ?? null;
}

/** Type the mention into the app's newest agent session (or the selected app's). Falls back to the clipboard. */
export async function mention(text: string, appId: string | null): Promise<void> {
  const st = useStore.getState();
  const want = appId ?? st.selectedAppId;
  const ptys = await ipc.listPtys().catch(() => []);
  const target = ptys.filter((p) => !want || p.appId === want).pop() ?? ptys.pop();
  if (target) {
    await ipc.ptyWrite(target.ptyId, text + " ");
    st.notify(`Mentioned in ${target.agentType === "custom_shell" ? "shell" : target.agentType.replace("_", " ")} session`);
    return;
  }
  try { await navigator.clipboard.writeText(text + " "); } catch { /* ignore */ }
  st.notify("No agent session open — mention copied to clipboard");
}
