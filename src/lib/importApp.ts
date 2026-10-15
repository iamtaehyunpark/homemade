import { open } from "@tauri-apps/plugin-dialog";
import { ipc } from "./ipc";
import { errMsg } from "./format";
import { useStore } from "../state/store";

export async function importPaths(paths: string[]) {
  for (const path of paths) {
    try {
      const app = await ipc.importApp(path);
      useStore.getState().upsertApp(app);
      useStore.getState().openApp(app.id);
    } catch (e) { useStore.getState().notify(`Import failed: ${errMsg(e)}`); }
  }
}

export async function pickApp(directory = true, draft = false) {
  try {
    const path = await open(directory
      ? { directory: true, title: draft ? "Import folder & draft manifest" : "Import app folder" }
      : { multiple: false, title: "Import script", filters: [{ name: "Scripts", extensions: ["py", "js", "ts", "sh", "toml"] }] });
    if (typeof path !== "string") return;
    if (draft) useStore.getState().setDraftPath(path);
    else await importPaths([path]);
  } catch (e) { useStore.getState().notify(`Import failed: ${errMsg(e)}`); }
}
