// Dedicated windows: an app can live in its own Tauri webview window (rendered standalone by App.tsx) either because its
// manifest says default_layout = "window" or because the user chose "Open in new window" from the ⋯ menu.
import { WebviewWindow, getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { emit } from "@tauri-apps/api/event";
import type { AppRecord } from "./types";
import { useStore } from "../state/store";
import { errMsg } from "./format";

export const appWindowLabel = (appId: string) => `app-${appId.replace(/[^a-zA-Z0-9_-]/g, "_")}`;
export const DOCK_EVENT = "app://dock";

/** Apps the user explicitly sent back to the dock: Workbench opens these as tabs even if the manifest says "window". */
export const forceTab = new Set<string>();
/** Apps currently living in their own window (so the sidebar focuses that window instead of opening a tab). */
export const inWindow = new Set<string>();

export async function openAppWindow(app: AppRecord): Promise<void> {
  const label = appWindowLabel(app.id);
  try {
    const existing = await WebviewWindow.getByLabel(label);
    if (existing) { inWindow.add(app.id); await existing.setFocus(); return; }
    const w = new WebviewWindow(label, {
      url: `/?app=${encodeURIComponent(app.id)}&standalone=1`,
      title: app.name, width: 960, height: 720,
    });
    inWindow.add(app.id);
    forceTab.delete(app.id);
    w.once("tauri://error", (e) => { inWindow.delete(app.id); useStore.getState().notify(`Could not open window: ${errMsg(e.payload)}`); });
    w.once("tauri://destroyed", () => inWindow.delete(app.id));
  } catch (e) {
    useStore.getState().notify(`Could not open window: ${errMsg(e)}`);
  }
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
