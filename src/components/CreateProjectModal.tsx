import { useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { ipc } from "../lib/ipc";
import { errMsg } from "../lib/format";
import { forceTab } from "../lib/appWindow";
import { useStore } from "../state/store";
import { Btn, Modal } from "./ui";
import Icon from "./Icon";

export default function CreateProjectModal() {
  const [name, setName] = useState("");
  const [parentDir, setParentDir] = useState("");
  // Default location: ~/homemade (or `[projects] dir` in config.toml); created when the first project lands there.
  useEffect(() => { ipc.defaultProjectsDir().then((d) => setParentDir((cur) => cur || d)).catch(() => {}); }, []);
  const [creating, setCreating] = useState(false);
  const [error, setError] = useState("");
  const close = () => { if (!creating) useStore.getState().setCreateProjectOpen(false); };
  const invalidName = !name.trim() || name.trim() === "." || name.trim() === ".." || /[/\\\\:\u0000-\u001f]/.test(name);
  const chooseLocation = async () => {
    try {
      const path = await open({ directory: true, title: "Choose a location for your new project" });
      if (typeof path === "string") { setParentDir(path); setError(""); }
    } catch (e) { setError(errMsg(e)); }
  };
  const create = async () => {
    if (creating || invalidName || !parentDir) return;
    setCreating(true); setError("");
    try {
      const app = await ipc.createProject(parentDir, name.trim());
      const store = useStore.getState();
      store.upsertApp(app);
      forceTab.add(app.id);
      store.setPendingShellAppId(app.id);
      store.openApp(app.id);
      store.setCreateProjectOpen(false);
    } catch (e) { setError(errMsg(e)); setCreating(false); }
  };
  return (
    <Modal title="Create project" onClose={close} testid="create-project-modal">
      <form onSubmit={(e) => { e.preventDefault(); void create(); }} className="space-y-4">
        <p className="text-[12px] leading-5 text-muted">Create a project folder and start a shell inside it. New projects go to <code className="text-fg">~/homemade</code> unless you pick another location.</p>
        <div>
          <label htmlFor="project-name" className="mb-1.5 block text-[12px] font-medium">Folder name</label>
          <input id="project-name" data-testid="project-name" autoFocus disabled={creating} value={name} onChange={(e) => setName(e.target.value)} placeholder="my-project" className="field w-full" aria-invalid={!!name && invalidName} />
          {!!name && invalidName && <p className="mt-1 text-[11px] text-danger">Enter a folder name without slashes or special characters.</p>}
        </div>
        <div>
          <label htmlFor="project-location" className="mb-1.5 block text-[12px] font-medium">Location</label>
          <div className="flex gap-2"><input id="project-location" data-testid="project-location" disabled={creating} className="field min-w-0 flex-1" value={parentDir} onChange={(e) => setParentDir(e.target.value)} placeholder="~/homemade" /><Btn data-testid="project-browse" disabled={creating} onClick={() => void chooseLocation()}><Icon name="folder" size={15} />Browse…</Btn></div>
          {parentDir && name.trim() && !invalidName && <p className="mt-2 break-all font-mono text-[10px] text-faint">{parentDir}/{name.trim()}</p>}
        </div>
        {error && <p role="alert" className="text-[12px] text-danger">{error}</p>}
        <div className="flex justify-end gap-2 pt-2"><Btn disabled={creating} onClick={close}>Cancel</Btn><Btn data-testid="project-create" type="submit" variant="primary" disabled={creating || invalidName || !parentDir}><Icon name="terminal" size={14} />{creating ? "Creating…" : "Create & open shell"}</Btn></div>
      </form>
    </Modal>
  );
}
