// OWNER: agent F (frontend). Entry shell: left sidebar (app list + import), center Workbench (dockview), bottom ResourceBar.
import { useEffect } from "react";
import Workbench from "./components/Workbench";
import Sidebar from "./components/Sidebar";
import Settings from "./components/Settings";
import Monitor from "./components/Monitor";
import Launch from "./components/Launch";
import ResourceBar from "./components/ResourceBar";
import Icon from "./components/Icon";
import { Btn, Modal } from "./components/ui";
import { useTauriEvents } from "./lib/useTauriEvents";
import { startDropRouter } from "./lib/dropRouter";
import { ipc } from "./lib/ipc";
import { useStore } from "./state/store";
import AppPanel from "./components/AppPanel";
import DraftManifestModal from "./components/DraftManifestModal";
import SelectMode, { SelectButton } from "./components/SelectMode";
import CreateProjectModal from "./components/CreateProjectModal";
import { installSoloCloseGuard } from "./lib/appWindow";

const params = new URLSearchParams(window.location.search);
const standaloneAppId = params.get("app");

/** Dedicated window: just the program (web UI / custom UI / form) — no sidebar, dock, header or Underbar. */
function StandaloneApp({ appId }: { appId: string }) {
  useTauriEvents();
  useEffect(() => startDropRouter(), []);
  useEffect(() => installSoloCloseGuard(appId), [appId]);
  const toast = useStore((s) => s.toast);
  const notify = useStore((s) => s.notify);
  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => notify(null), 4000);
    return () => clearTimeout(t);
  }, [toast, notify]);
  return (
    <div className="flex h-full flex-col">
      <div className="min-h-0 flex-1"><AppPanel appId={appId} solo /></div>
      <DraftManifestModal />
      <SelectMode />
      {toast && <Toast message={toast} onDismiss={() => notify(null)} solo />}
    </div>
  );
}

export default function App() {
  return standaloneAppId ? <StandaloneApp appId={standaloneAppId} /> : <MainApp />;
}

function MainApp() {
  useTauriEvents();
  useEffect(() => startDropRouter(), []);
  const view = useStore((s) => s.view);
  const createProjectOpen = useStore((s) => s.createProjectOpen);
  const confirmExit = useStore((s) => s.confirmExit);
  const setConfirmExit = useStore((s) => s.setConfirmExit);
  const toast = useStore((s) => s.toast);
  const notify = useStore((s) => s.notify);

  useEffect(() => {
    if (!toast) return;
    const t = setTimeout(() => notify(null), 4000);
    return () => clearTimeout(t);
  }, [toast, notify]);

  return (
    <div className="flex h-full flex-col">
      <div className="flex min-h-0 flex-1">
        <Sidebar />
        <main className="relative flex min-w-0 flex-1 flex-col">
          {view === "workbench" && <header className="flex h-11 shrink-0 items-center justify-between border-b border-line bg-panel px-4"><span className="text-[12px] font-medium text-muted">Workspace</span><SelectButton /></header>}
          <div className={view === "workbench" ? "min-h-0 flex-1" : "hidden"}><Workbench /></div>
          {view === "settings" && <Settings />}
          {view === "monitor" && <Monitor />}
          {view === "launch" && <Launch />}
        </main>
      </div>
      {view !== "launch" && <ResourceBar />}
      {createProjectOpen && <CreateProjectModal />}
      <DraftManifestModal />
      <SelectMode />
      {toast && (
        <Toast message={toast} onDismiss={() => notify(null)} />
      )}
      {confirmExit != null && (
        <Modal testid="confirm-exit-modal" title="Quit Homemade?" onClose={() => setConfirmExit(null)}>
          <p className="text-fg">{confirmExit} program{confirmExit === 1 ? " is" : "s are"} running. Stop all and quit?</p>
          <div className="mt-4 flex justify-end gap-2">
            <Btn onClick={() => setConfirmExit(null)}>Cancel</Btn>
            <Btn variant="danger" autoFocus onClick={() => { setConfirmExit(null); ipc.confirmQuit().catch(() => {}); }}>Quit</Btn>
          </div>
        </Modal>
      )}
    </div>
  );
}

function Toast({ message, onDismiss, solo }: { message: string; onDismiss: () => void; solo?: boolean }) {
  return <div className={`app-toast fixed right-4 z-50 max-w-sm ${solo ? "bottom-4" : "bottom-12"}`}><span role="status" className="min-w-0 break-words">{message}</span><button onClick={onDismiss} aria-label="Dismiss notification" className="rounded p-1 text-muted hover:bg-panel-2"><Icon name="close" size={14} /></button></div>;
}
