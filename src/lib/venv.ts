import { ipc } from "./ipc";
import type { AppRecord } from "./types";
import { useStore } from "../state/store";
import { errMsg, fmtBytes } from "./format";

/** Spec §8B: badge only when the unmanaged venv exceeds 50 MB. */
export const VENV_BADGE_MIN_BYTES = 50 * 1024 * 1024;

/** Move an app's unmanaged venv into the shared store (Sidebar badge + Program tab). */
export async function dedupeVenv(app: AppRecord): Promise<void> {
  const st = useStore.getState();
  try {
    const freed = await ipc.migrateVenv(app.id);
    st.notify(`${app.name}: freed ${fmtBytes(freed)}`);
    st.upsertApp(await ipc.getApp(app.id));
  } catch (err) { st.notify(`Deduplicate failed: ${errMsg(err)}`); }
}
