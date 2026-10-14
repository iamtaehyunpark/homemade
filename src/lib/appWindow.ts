// default_layout = "window": open an app in its own Tauri webview window (rendered standalone by App.tsx).
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { AppRecord } from "./types";
import { useStore } from "../state/store";
import { errMsg } from "./format";

export const appWindowLabel = (appId: string) => `app-${appId.replace(/[^a-zA-Z0-9_-]/g, "_")}`;

export async function openAppWindow(app: AppRecord): Promise<void> {
  const label = appWindowLabel(app.id);
  try {
    const existing = await WebviewWindow.getByLabel(label);
    if (existing) { await existing.setFocus(); return; }
    const w = new WebviewWindow(label, {
      url: `/?app=${encodeURIComponent(app.id)}&standalone=1`,
      title: app.name, width: 960, height: 720,
    });
    w.once("tauri://error", (e) => useStore.getState().notify(`Could not open window: ${errMsg(e.payload)}`));
  } catch (e) {
    useStore.getState().notify(`Could not open window: ${errMsg(e)}`);
  }
}
