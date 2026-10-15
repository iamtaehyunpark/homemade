// Dedicated windows: an app can live in its own Tauri webview window (rendered standalone by App.tsx) either because its
// manifest says default_layout = "window" or because the user chose "Open in new window" from the ⋯ menu.
import { WebviewWindow, getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { emit } from "@tauri-apps/api/event";
import type { AppRecord } from "./types";
import { useStore } from "../state/store";
import { errMsg, isActive } from "./format";
import { ipc } from "./ipc";

export const appWindowLabel = (appId: string) => `app-${appId.replace(/[^a-zA-Z0-9_-]/g, "_")}`;
export const DOCK_EVENT = "app://dock";

/** Apps the user explicitly sent back to the dock: Workbench opens these as tabs even if the manifest says "window". */
export const forceTab = new Set<string>();
/** Apps currently living in their own window (so the sidebar focuses that window instead of opening a tab, and Launch hides them). */
export const inWindow = {
  has: (id: string) => !!useStore.getState().windowed[id],
  add: (id: string) => useStore.getState().setWindowed(id, true),
  delete: (id: string) => useStore.getState().setWindowed(id, false),
};

export async function openAppWindow(app: AppRecord): Promise<void> {
  const label = appWindowLabel(app.id);
  try {
    const existing = await WebviewWindow.getByLabel(label);
    if (existing) { inWindow.add(app.id); await existing.setFocus(); return; }
    // Built on the Rust side so the element picker is injected into the program's frames (see windows.rs).
    await ipc.openAppWindow(label, `/?app=${encodeURIComponent(app.id)}&standalone=1`, app.name);
    inWindow.add(app.id);
    forceTab.delete(app.id);
    const w = await WebviewWindow.getByLabel(label);
    w?.once("tauri://destroyed", () => inWindow.delete(app.id));
  } catch (e) {
    useStore.getState().notify(`Could not open window: ${errMsg(e)}`);
  }
}

/** Solo window close = clean exit of the program: stop its active run (SIGINT → SIGTERM → SIGKILL per the supervisor), wait for it to end, then let the window go. */
export function installSoloCloseGuard(appId: string): () => void {
  const win = getCurrentWebviewWindow();
  let closing = false;
  const p = win.onCloseRequested(async (e) => {
    if (closing) return;
    const st = useStore.getState();
    const active = Object.values(st.runs).filter((r) => r.appId === appId && isActive(r.status));
    if (!active.length) return;
    e.preventDefault();
    closing = true;
    await Promise.all(active.map((r) => ipc.stopRun(r.id).catch(() => {})));
    const deadline = Date.now() + 15000;
    while (Date.now() < deadline && Object.values(useStore.getState().runs).some((r) => r.appId === appId && isActive(r.status))) await new Promise((r) => setTimeout(r, 150));
    await win.destroy().catch(() => win.close());
  });
  return () => { p.then((u) => u()).catch(() => {}); };
}

/** From a standalone window: ask the main window to re-open this app as a tab, then close this window. */
export async function dockAppWindow(appId: string): Promise<void> {
  try {
    await emit(DOCK_EVENT, { appId });
    const main = await WebviewWindow.getByLabel("main");
    await main?.setFocus();
    await getCurrentWebviewWindow().close();
  } catch (e) {
    useStore.getState().notify(`Could not return to main window: ${errMsg(e)}`);
  }
}
