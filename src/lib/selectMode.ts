// Select mode ("⌖"): like a browser's element picker, but the pick becomes a *mention* typed into the app's agent session
// (no Enter), so the user can point at anything in the launcher — a run row, a log line, an output card, the web UI —
// and continue with "…make this faster". Nothing is sent; the agent terminal input just gains a bracketed reference.
import { ipc } from "./ipc";
import { useStore } from "../state/store";

export interface Pick {
  mention: string;
  appId: string | null;
  selector: string;
  text: string;
  rect: { x: number; y: number; w: number; h: number };
}

const MAX_TEXT = 90;
const clean = (s: string) => s.replace(/\s+/g, " ").trim();

/** Human path for an element: app › area › tab › element "text". */
export function describe(el: Element, clientX?: number, clientY?: number): Pick {
  const parts: string[] = [];
  const panel = el.closest<HTMLElement>("[data-app-panel]");
  const appId = panel?.dataset.appPanel ?? el.closest<HTMLElement>("[data-app]")?.dataset.app ?? null;
  const app = appId ? useStore.getState().apps.find((a) => a.id === appId) : null;
  if (app) parts.push(app.name);
  else if (el.closest("[data-testid='launch-view']")) parts.push("Launch");
  else if (el.closest("[data-testid='monitor-view'], [data-testid='monitor-cpu']")) parts.push("Monitor");
  else if (el.closest("aside")) parts.push("Sidebar");
  if (el.closest("[data-testid='agent-terminal']")) parts.push("Agent terminal");
  const underbar = el.closest<HTMLElement>("[data-testid='underbar']");
  if (underbar) {
    const tab = underbar.querySelector<HTMLElement>("[role='tab'][aria-selected='true'], [data-tab][data-active='true']");
    const tabName = tab ? (tab.title || tab.textContent || "").replace(/\s*\(⌘\d\)\s*$/, "").replace(/\d+$/, "") : "";
    parts.push("Underbar" + (tabName ? ` › ${clean(tabName)} tab` : ""));
  }
  // The nearest meaningful ancestor with a testid gives the element's role; text is the element's own.
  const chain: string[] = [];
  let n: Element | null = el;
  while (n && chain.length < 4) {
    const id = (n as HTMLElement).dataset?.testid;
    if (id) chain.unshift(id);
    n = n.parentElement;
  }
  const tag = el.tagName.toLowerCase();
  let what = chain.length ? chain[chain.length - 1] : tag;
  let text = clean(el.textContent ?? "");
  if (el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement || el instanceof HTMLSelectElement) {
    what = `${tag}${el.name ? ` "${el.name}"` : ""}`;
    text = String((el as HTMLInputElement).value ?? "");
  }
  if (tag === "img" && (el as HTMLImageElement).alt) text = (el as HTMLImageElement).alt;
  let iframeNote = "";
  if (tag === "iframe") {
    const r = el.getBoundingClientRect();
    const src = (el as HTMLIFrameElement).src;
    const at = clientX != null && clientY != null ? ` at (${Math.round(clientX - r.left)},${Math.round(clientY - r.top)}) of ${Math.round(r.width)}×${Math.round(r.height)}` : "";
    iframeNote = ` web view ${src}${at}`;
    what = "program UI";
    text = "";
  }
  if (text.length > MAX_TEXT) text = text.slice(0, MAX_TEXT - 1) + "…";
  const selector = chain.length ? chain.map((c) => `[data-testid="${c}"]`).join(" ") : tag;
  const r = el.getBoundingClientRect();
  const loc = parts.length ? parts.join(" › ") + " › " : "";
  const mention = `[ui: ${loc}${what}${text ? ` "${text}"` : ""}${iframeNote}${appId ? ` (app ${appId})` : ""}]`;
  return { mention, appId, selector, text, rect: { x: r.left, y: r.top, w: r.width, h: r.height } };
}

/** Type the mention into the app's newest agent session (or the selected app's). Falls back to the clipboard. */
export async function mention(pick: Pick): Promise<"typed" | "clipboard"> {
  const st = useStore.getState();
  const appId = pick.appId ?? st.selectedAppId;
  const ptys = await ipc.listPtys().catch(() => []);
  const target = ptys.filter((p) => !appId || p.appId === appId).pop() ?? ptys.pop();
  const text = pick.mention + " ";
  if (target) {
    await ipc.ptyWrite(target.ptyId, text);
    st.notify(`Mentioned in ${target.agentType === "custom_shell" ? "shell" : target.agentType.replace("_", " ")} session`);
    return "typed";
  }
  try { await navigator.clipboard.writeText(text); } catch { /* ignore */ }
  st.notify("No agent session open — mention copied to clipboard");
  return "clipboard";
}
