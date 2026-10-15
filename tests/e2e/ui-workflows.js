// Run with HOMEMADE_HOME=<disposable profile> HOMEMADE_E2E=<this file>.
// Creates a project under that profile and exercises real native PTYs.
const H = window.__hm;
const q = (selector) => document.querySelector(selector);
const assert = (value, message) => { if (!value) throw new Error(message); };
const settle = () => new Promise((resolve) => requestAnimationFrame(() => requestAnimationFrame(resolve)));
const report = (name) => H.report(`PASS ${name}`);
const home = await H.ipc.homemadeHome();
await H.waitFor(() => q('[data-testid="import-app"]'));
H.click('[data-testid="import-app"]');
await settle();
H.click('[data-testid="create-project"]');
await settle();
H.setInput('[data-testid="project-name"]', "UI review project");
H.setInput('[data-testid="project-location"]', home);
await settle();
H.click('[data-testid="project-create"]');
const project = await H.waitFor(() => H.store.getState().apps.find((app) => app.name === "UI review project"));
assert(project.rootDir.endsWith("/UI review project"), "wrong project path");
assert(project.manifest.app.exec === "", "blank project should not choose a run command");
let rejected = false;
try { await H.ipc.createProject(home, "UI review project"); } catch { rejected = true; }
assert(rejected, "existing folder was reused");
await report("create project, blank manifest, and collision protection");

const terminal = await H.waitFor(() => q('[data-testid="agent-terminal"][data-pty-id]:not([data-pty-id=""])'), 20000);
const ptyId = terminal.dataset.ptyId;
const ptys = await H.ipc.listPtys();
assert(ptys.find((pty) => pty.ptyId === ptyId)?.agentType === "custom_shell", "created project opened an agent instead of a shell");
await H.ipc.ptyWrite(ptyId, "printf '\\nREVIEW_CWD=%s\\n' \"$PWD\"\r");
await H.waitFor(() => H.termText(ptyId).includes(`REVIEW_CWD=${project.rootDir}`), 10000);
assert(q('[data-testid="run-button"]').disabled, "Run must be disabled until configured");
assert(q('[data-testid="project-unconfigured"]'), "configuration guidance missing");
await report("create → workspace → interactive shell in the new folder");

const sidebar = q('aside[aria-label="Sidebar"]');
assert(!sidebar.querySelector('[data-testid="select-mode"]'), "picker still lives in sidebar");
H.click('[data-testid="sidebar-toggle"]');
await settle();
assert(sidebar.getBoundingClientRect().width === 60, "sidebar did not collapse");
assert(sidebar.querySelector(`button[aria-label="${project.name}"]`), "project inaccessible in collapsed rail");
H.click('[data-testid="sidebar-toggle"]');
await settle();
assert(sidebar.getBoundingClientRect().width > 60, "sidebar did not expand");
await report("sidebar collapse and accessible app shortcuts");

H.click('[data-testid="select-mode"]');
await settle();
assert(q('[data-testid="select-mode"]').getAttribute("aria-pressed") === "true", "picker did not activate");
window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
await settle();
assert(!H.store.getState().selectMode, "Escape did not leave select mode");
await report("workspace element picker and Escape");

H.click('[data-testid="import-app"]');
await settle();
H.click('[data-testid="create-project"]');
await settle();
assert(q('[data-testid="create-project-modal"] [role="dialog"]'), "create modal missing");
H.setInput('[data-testid="project-name"]', "../outside");
await settle();
assert(q('[data-testid="project-name"]').getAttribute("aria-invalid") === "true", "invalid name accepted in form");
assert(q('[data-testid="project-create"]').disabled, "invalid form can submit");
H.store.getState().setCreateProjectOpen(false);
await report("Add menu and create-project validation");

const runnable = await H.ipc.createProject(home, "Launcher review app");
const manifest = await H.ipc.readTextFile(`${runnable.rootDir}/homemade.toml`);
const configured = await H.ipc.saveManifest(runnable.id, manifest.replace('exec = ""', 'exec = "echo ready"'));
H.store.getState().upsertApp(configured);
H.click('[data-testid="launch-nav"]');
await settle();
assert(q(`[data-testid="launch-tile"][data-app="${configured.id}"]`), "runnable app absent from launcher");
assert(!q(`[data-testid="launch-tile"][data-app="${project.id}"]`), "blank project visible in launcher");
assert(!q('[data-testid="select-mode"]'), "development picker visible in launcher");
assert(!q('[data-testid="resource-bar"]'), "resource monitor visible in launcher");
assert(q('.launcher-icon').getBoundingClientRect().width === 76, "launcher icon isn't app-sized");
H.setInput('[data-testid="launch-filter"]', "does-not-exist");
await settle();
assert(!q('[data-testid="launch-tile"]'), "search did not filter apps");
assert(q('[data-testid="launch-view"]').textContent.includes("No matching apps"), "missing no-match state");
H.setInput('[data-testid="launch-filter"]', "");
await settle();
H.click(`[data-testid="launch-tile"][data-app="${configured.id}"]`);
await H.waitFor(async () => (await H.windows()).includes(`app-${configured.id}`), 10000);
await H.closeWindow(`app-${configured.id}`);
await report("app launcher layout, search, and standalone window");

await H.ipc.ptyKill(ptyId);
H.store.getState().closeApp(project.id);
await H.waitFor(async () => (await H.ipc.listPtys()).length === 0);
await H.report("UI WORKFLOWS PASSED");
await H.done(0);
