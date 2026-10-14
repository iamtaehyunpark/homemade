// Homemade live E2E. Evaluated inside the main webview via HOMEMADE_E2E. Uses window.__hm.
const H = window.__hm;
const F = (typeof window.__E2E_FIXTURES === "string" ? window.__E2E_FIXTURES : "/tmp/homemade-e2e");
const R = (m) => H.report(m);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
let fails = 0, passes = 0;
async function t(name, fn) {
  try { await fn(); passes++; await R("PASS " + name); }
  catch (e) { fails++; await R("FAIL " + name + " :: " + (e && e.message || e) + " @ " + (e && e.stack || "").split("\n")[0]); }
}
const ok = (c, m) => { if (!c) throw new Error(m || "assertion failed"); };
const runOf = (id) => H.store.getState().runs[id];
const waitRun = (id, statuses, ms = 30000) => H.waitFor(() => { const r = runOf(id); return r && statuses.includes(r.status) ? r : null; }, ms);
const q = (s) => document.querySelector(s);
const qa = (s) => Array.from(document.querySelectorAll(s));

await R("start " + new Date().toISOString() + " __hm keys: " + Object.keys(H).join(","));
const apps = {};

await t("import apps (manifest + inference)", async () => {
  for (const n of ["hello-cli", "hello-ui", "hello-worker", "no-manifest", "gpu-app", "env-app", "venv-app", "webapp-app"]) {
    apps[n] = await H.ipc.importApp(F + "/" + n);
    ok(apps[n].id, "no id for " + n);
  }
  const nm = apps["no-manifest"];
  ok(nm.manifest.inputs.folder && nm.manifest.inputs.folder.type === "directory", "inferred folder input missing: " + JSON.stringify(Object.keys(nm.manifest.inputs)));
  ok(nm.manifest.inputs.pattern && nm.manifest.inputs.pattern.default === "*.png", "inferred default wrong");
  ok(apps["hello-ui"].hasCustomUi, "hello-ui should have custom UI");
  const list = await H.ipc.listApps();
  ok(list.length >= 8, "listApps count " + list.length);
  await R("  app ids: " + list.map((a) => a.id).join(","));
});

await t("detect agents", async () => {
  const d = await H.ipc.detectAgents();
  const types = d.map((x) => x.agentType);
  await R("  agents: " + JSON.stringify(d));
  ok(types.includes("claude_code") && types.includes("codex") && types.includes("antigravity"), "missing agents " + types);
});

await t("hello-cli: AutoForm run via UI, log, progress, shelf, copy-cli, preset", async () => {
  const a = apps["hello-cli"];
  await H.openApp(a.id);
  ok(q('[data-testid="run-button"]'), "no run button");
  const sel = ['input[name="name"]', '[data-testid="input-name"]', '[data-input="name"] input'].find((x) => q(x));
  if (sel) H.setInput(sel, "E2E"); else await R("  (no name input found; inputs present: " + qa("input").map((i) => i.name || i.placeholder || i.type).join(",") + ")");
  await R("  run-button present: " + !!q('[data-testid="run-button"]'));
  H.click('[data-testid="run-button"]');
  const started = await H.waitFor(() => Object.values(H.store.getState().runs).find((r) => r.appId === a.id), 10000);
  const fin = await waitRun(started.id, ["success", "failed", "killed"], 30000);
  ok(fin.status === "success", "status " + fin.status + " tail=" + fin.logTail);
  await sleep(500);
  const lines = qa('[data-testid="log-line"]').map((e) => e.textContent);
  ok(lines.some((l) => /HELLO|Hello/.test(l)), "log lines missing greeting: " + lines.slice(0, 5).join("|"));
  ok(!lines.some((l) => l.includes("@homemade:")), "directive lines leaked into console");
  ok(q('[data-testid="progress-bar"]'), "no progress bar");
  const arts = await H.ipc.listArtifacts(a.id, started.id);
  ok(arts.length >= 1 && arts[0].filePath.endsWith("greeting.txt"), "artifacts " + JSON.stringify(arts));
  await H.waitFor(() => qa('[data-testid="shelf-item"]').length >= 1, 5000);
  const cmd = await H.ipc.buildCliCommand(a.id, { name: "X", count: 2, shout: true });
  ok(cmd.includes("--name X") && cmd.includes("--shout"), "cli cmd: " + cmd);
  const p = await H.ipc.savePreset(a.id, "E2E preset", { name: "P" });
  const ps = await H.ipc.listPresets(a.id);
  ok(ps.some((x) => x.id === p.id), "preset not listed");
  await H.ipc.deletePreset(p.id);
  const hist = qa('[data-testid="run-history-item"]');
  ok(hist.length >= 1, "run history empty");
});

await t("hello-ui: custom UI bridge (ready, schema, run, assetUrl)", async () => {
  const a = apps["hello-ui"];
  await H.openApp(a.id);
  await H.waitFor(() => q('[data-testid="app-sandbox"][data-bridge="ready"]'), 15000);
  const typ = await H.sandboxEval(a.id, "typeof window.homemade.run");
  ok(typ === "function", "window.homemade.run is " + typ);
  const schema = await H.sandboxEval(a.id, "Object.keys(await window.homemade.getSchema())");
  ok(Array.isArray(schema) && schema.includes("name"), "schema " + JSON.stringify(schema));
  const res = await H.sandboxEval(a.id, "await window.homemade.run({ name: 'Bridge', count: 1 })");
  await R("  bridge run result: " + JSON.stringify(res).slice(0, 200));
  ok(res && (res.status === "success"), "bridge run status " + JSON.stringify(res));
  const url = await H.sandboxEval(a.id, "window.homemade.assetUrl('/tmp/x.png')");
  ok(typeof url === "string" && url.includes("homemade-asset"), "assetUrl " + url);
  const arts = await H.sandboxEval(a.id, "await window.homemade.listArtifacts()");
  ok(Array.isArray(arts), "listArtifacts via bridge");
});

await t("hello-worker: rpc_worker mode, call via ipc and bridge, stop reaps", async () => {
  const a = apps["hello-worker"];
  await H.openApp(a.id);
  const run = await H.ipc.startRun(a.id, {});
  let got = null;
  for (let i = 0; i < 40 && got === null; i++) { try { const v = await H.ipc.rpcCall(a.id, "add", { a: 2, b: 3 }, run.id); if (v === 5 || (v && v.result === 5)) got = v; } catch (e) { await sleep(500); } }
  ok(got !== null, "worker never answered add()");
  await H.waitFor(() => q('[data-testid="app-sandbox"][data-bridge="ready"]'), 15000);
  const v = await H.sandboxEval(a.id, "await window.homemade.call('add', { a: 10, b: 5 })");
  ok(v === 15 || (v && v.result === 15), "bridge call result " + JSON.stringify(v));
  await H.ipc.stopRun(run.id);
  const fin = await waitRun(run.id, ["killed", "success", "failed"], 15000);
  ok(fin.status === "killed" || fin.status === "success", "worker stop status " + fin.status);
  for (let i = 0; i < 20; i++) { if ((await H.ipc.listActiveRuns()).length === 0) break; await sleep(500); }
  ok((await H.ipc.listActiveRuns()).length === 0, "active runs remain");
});

await t("gpu-app: admission queue, banner, run anyway, stop", async () => {
  const a = apps["gpu-app"];
  await H.openApp(a.id);
  const r1 = await H.ipc.startRun(a.id, {});
  await waitRun(r1.id, ["running"], 10000);
  const r2 = await H.ipc.startRun(a.id, {});
  const q2 = await waitRun(r2.id, ["queued_vram", "running"], 10000);
  ok(q2.status === "queued_vram", "second GPU run should queue, got " + q2.status);
  await H.waitFor(() => q('[data-testid="queue-banner"]'), 5000);
  await H.waitFor(() => ((H.store.getState().resources || {}).queue || []).length >= 1 ? true : null, 5000);
  await H.ipc.forceRun(r2.id);
  await waitRun(r2.id, ["running"], 10000);
  await H.ipc.stopRun(r1.id); await H.ipc.stopRun(r2.id);
  const f1 = await waitRun(r1.id, ["killed"], 15000); const f2 = await waitRun(r2.id, ["killed"], 15000);
  ok(f1.status === "killed" && f2.status === "killed", "statuses " + f1.status + "," + f2.status);
});

await t("gpu-app: stop a queued run finishes as killed", async () => {
  const a = apps["gpu-app"];
  const r1 = await H.ipc.startRun(a.id, {});
  await waitRun(r1.id, ["running"], 10000);
  const r2 = await H.ipc.startRun(a.id, {});
  await waitRun(r2.id, ["queued_vram"], 10000);
  await H.ipc.stopRun(r2.id);
  const f2 = await waitRun(r2.id, ["killed"], 10000);
  ok(f2.status === "killed");
  await H.ipc.stopRun(r1.id); await waitRun(r1.id, ["killed"], 15000);
});

await t("env-app: secrets vault, redaction, alias expansion, args input, stderr marking, scratch, storage_refs", async () => {
  const a = apps["env-app"];
  await H.ipc.setSecret("E2E_SECRET", "s3cr3t-value-xyz");
  ok((await H.ipc.listSecretNames()).includes("E2E_SECRET"), "secret name not listed");
  await H.openApp(a.id);
  const run = await H.ipc.startRun(a.id, { extra: "--alpha 'two words'" });
  const fin = await waitRun(run.id, ["success", "failed", "killed"], 30000);
  await R("  env-app tail: " + JSON.stringify(fin.logTail));
  ok(fin.status === "success", "status " + fin.status);
  ok(!fin.logTail.includes("s3cr3t-value-xyz") && fin.logTail.includes("SECRET=***"), "secret not redacted in log_tail");
  ok(fin.logTail.includes("ALIAS=/tmp/e2e-alias/sub"), "alias not expanded");
  ok(fin.logTail.includes("ARGS=--alpha two words"), "args input not passed");
  ok(fin.logTail.includes("[stderr] this goes to stderr"), "stderr marker missing");
  await sleep(500);
  const errLine = qa('[data-testid="log-line"][data-stream="stderr"]');
  ok(errLine.length >= 1, "no stderr-colored line in console");
  const arts = await H.ipc.listArtifacts(a.id, run.id);
  const kinds = arts.map((x) => x.kind + ":" + x.filePath.split("/").pop());
  ok(kinds.includes("output:result.json") && kinds.includes("scratch:tmp_scratch.part"), "artifact classification: " + kinds);
  const freed = await H.ipc.cleanScratch(a.id);
  ok(freed > 0, "clean scratch freed " + freed);
  await H.ipc.deleteSecret("E2E_SECRET");
  ok(!(await H.ipc.listSecretNames()).includes("E2E_SECRET"), "secret not deleted");
});

await t("venv-app: unmanaged venv scan + dedupe migration + run", async () => {
  const a = apps["venv-app"];
  const bytes = await H.ipc.scanVenv(a.id);
  ok(bytes > 50 * 1024 * 1024, "scan bytes " + bytes + " (fixture must exceed the 50 MB badge threshold)");
  await H.openApp(a.id);
  const badge = () => q('[data-testid="sidebar-app-' + a.id + '"] [data-testid="venv-dedupe"]'); // scoped: other imported apps may have their own badge
  ok(badge(), "no dedupe badge");
  const after = await H.ipc.migrateVenv(a.id);
  await R("  venv before=" + bytes + " after=" + after);
  const rescan = await H.ipc.scanVenv(a.id);
  ok(rescan === 0, "migrated venv should scan as managed (0), got " + rescan);
  H.store.getState().upsertApp(await H.ipc.getApp(a.id)); // what the Sidebar's Deduplicate button does after migrating
  await H.waitFor(() => !badge(), 5000);
  const run = await H.ipc.startRun(a.id, {});
  const fin = await waitRun(run.id, ["success", "failed", "killed"], 60000);
  ok(fin.status === "success" && fin.logTail.includes("six ok"), "post-migration run: " + fin.status + " " + fin.logTail);
});

await t("webapp-app: mode=webapp embeds http://127.0.0.1:<port> from @homemade:port; RAM advisory badge", async () => {
  const a = apps["webapp-app"];
  await H.openApp(a.id);
  ok(q('[data-testid="ram-limit"]'), "ram-limit badge missing");
  ok(q('[data-testid="webapp-frame"]'), "webapp frame placeholder missing");
  const run = await H.ipc.startRun(a.id, {});
  const el = await H.waitFor(() => { const f = q('[data-testid="webapp-frame"]'); return f && f.dataset.port ? f : null; }, 15000);
  const port = Number(el.dataset.port);
  ok(port > 0, "port not announced");
  // iframe must only appear once the backend confirmed the port accepts connections (run://webapp_ready)
  const readyEl = await H.waitFor(() => { const f = q('[data-testid="webapp-frame"]'); return f && f.dataset.phase === "ready" ? f : null; }, 15000);
  ok(readyEl, "webapp never reached ready phase");
  ok(H.store.getState().webappReady[run.id] === true, "store webappReady not set");
  const fr = q('[data-testid="webapp-frame"] iframe');
  ok(fr && fr.src === "http://127.0.0.1:" + port + "/", "iframe src " + (fr && fr.src));
  await R("  webapp port=" + port);
  await H.ipc.stopRun(run.id);
  const fin = await waitRun(run.id, ["killed", "success", "failed"], 15000);
  ok(fin.status === "killed", "webapp stop status " + fin.status);
  await H.waitFor(() => { const f = q('[data-testid="webapp-frame"]'); return f && !f.dataset.port; }, 5000);
});

await t("git: init, checkpoint, log, revert", async () => {
  const a = apps["hello-cli"];
  let s = await H.ipc.gitInit(a.id);
  ok(s.initialized && s.log.length >= 1, "init " + JSON.stringify(s));
  const first = s.log[0].sha;
  const app = await H.ipc.getApp(a.id);
  const toml = await H.ipc.readTextFile(app.rootDir + "/homemade.toml");
  await H.ipc.saveManifest(a.id, toml + "\n# e2e change\n");
  s = await H.ipc.gitStatus(a.id);
  ok(s.dirty, "should be dirty after manifest edit");
  const sha = await H.ipc.gitCheckpoint(a.id, "e2e checkpoint");
  s = await H.ipc.gitStatus(a.id);
  ok(!s.dirty && s.head === sha && s.log.length >= 2, "checkpoint " + JSON.stringify(s).slice(0, 200));
  s = await H.ipc.gitRevertTo(a.id, first);
  ok(s.head === first, "revert head " + s.head + " != " + first);
  H.store.getState().setApps(await H.ipc.listApps());
  await H.openApp(a.id);
  await H.waitFor(() => q('[data-testid="git-checkpoint"]') || q('[data-testid="git-init"]'), 5000);
});

await t("agent sessions: discovery of fake Claude session", async () => {
  const a = apps["hello-cli"];
  const ss = await H.ipc.listAgentSessions(a.id);
  await R("  sessions: " + JSON.stringify(ss).slice(0, 300));
  const f = ss.find((x) => x.sessionId === "11111111-2222-3333-4444-555555555555");
  ok(f && f.agentType === "claude_code" && (f.title || "").includes("E2E fake session"), "fake session not discovered");
});

await t("pty: custom shell spawn, data, exit, terminal UI", async () => {
  const a = apps["hello-cli"];
  await H.openApp(a.id);
  let buf = "";
  const un = await H.on("pty://data", (p) => { buf += p.data; });
  let exited = null;
  const un2 = await H.on("pty://exit", (p) => { exited = p; });
  const info = await H.ipc.ptySpawn(a.id, "custom_shell", { command: "echo PTY_OK_123; sleep 1", cols: 80, rows: 24 });
  await H.waitFor(() => buf.includes("PTY_OK_123"), 10000);
  await H.waitFor(() => exited && exited.ptyId === info.ptyId, 15000);
  un(); un2();
  H.click('[data-testid="edit-with-agent"]');
  await H.waitFor(() => q('[data-testid="agent-terminal"]'), 10000);
  ok(q('[data-testid="terminal-agent-select"]'), "agent select missing");
});

await t("pty: claude_code new session writes provenance row and spawns", async () => {
  const a = apps["hello-cli"];
  const info = await H.ipc.ptySpawn(a.id, "claude_code", { cols: 100, rows: 30 });
  ok(info.sessionId, "no session id");
  await sleep(4000);
  const ss = await H.ipc.listAgentSessions(a.id);
  const row = ss.find((x) => x.sessionId === info.sessionId);
  ok(row, "provenance row missing for new claude session");
  await R("  provenance: " + JSON.stringify(row));
  const ptys = await H.ipc.listPtys();
  ok(ptys.some((p) => p.ptyId === info.ptyId), "pty not active");
  await H.ipc.ptyKill(info.ptyId);
  await H.waitFor(async () => !(await H.ipc.listPtys()).some((p) => p.ptyId === info.ptyId), 10000);
});

await t("hot reload: ui_reload and manifest changed events (file touched externally)", async () => {
  const a = apps["hello-ui"];
  let ui = false, ch = false;
  const u1 = await H.on("app://ui_reload", (p) => { if (p.appId === a.id) ui = true; });
  const u2 = await H.on("app://changed", (p) => { if (p.appId === a.id) ch = true; });
  await R("TOUCH_NOW " + a.id);
  await H.waitFor(() => ui && ch, 20000);
  u1(); u2();
});

await t("layout: split_right and window modes", async () => {
  const a = apps["hello-cli"];
  const app = await H.ipc.getApp(a.id);
  const toml = await H.ipc.readTextFile(app.rootDir + "/homemade.toml");
  await H.ipc.saveManifest(a.id, toml.replace(/\[artifacts\]/, '[ui]\ndefault_layout = "split_right"\n\n[artifacts]'));
  await sleep(800);
  await H.openApp(a.id);
  await H.ipc.saveManifest(a.id, toml);
  await R("  split_right opened without error");
});

await t("remove app (purge scratch)", async () => {
  const a = apps["no-manifest"];
  await H.openApp(a.id);
  ok(q('[data-testid="remove-app"]'), "remove button missing");
  await H.ipc.removeApp(a.id, true, false);
  const list = await H.ipc.listApps();
  ok(!list.some((x) => x.id === a.id), "app still listed");
});

await t("resources + storage summary + settings", async () => {
  const r = await H.ipc.resourceSnapshot();
  ok(r.ramTotalMb > 0 && r.gpuTotalMb > 0, "snapshot " + JSON.stringify(r).slice(0, 120));
  const s = await H.ipc.storageSummary();
  ok(typeof s.uvCacheBytes === "number", "storage summary");
  H.click('[data-testid="settings"]');
  await H.waitFor(() => q('[data-testid="settings-secrets"]'), 5000);
});

await t("monitor pane: graphs, per-run table, footer shows current program's CPU/RAM", async () => {
  const a = apps["hello-worker"]; // long-lived warm worker gives a stable running row
  await H.openApp(a.id);
  const run = await H.ipc.startRun(a.id, {});
  await waitRun(run.id, ["running"], 10000);
  // footer: selected app's run must show CPU and RAM, not only memory
  const cur = await H.waitFor(() => { const el = q('[data-testid="resource-current"]'); return el && /CPU \d+%/.test(el.textContent) && /RAM /.test(el.textContent) ? el : null; }, 15000);
  await R("  footer current: " + cur.textContent.trim());
  H.click('[data-testid="monitor-nav"]');
  await H.waitFor(() => q('[data-testid="monitor-cpu"]') && q('[data-testid="monitor-ram"]') && q('[data-testid="monitor-gpu"]'), 5000);
  ok(q('[data-testid="monitor-cpu"] polyline'), "cpu graph has no polyline");
  const n0 = H.store.getState().resourceHistory.length;
  await H.waitFor(() => H.store.getState().resourceHistory.length > n0 + 1, 5000);
  ok(H.store.getState().resourceHistory.length > n0 + 1, "history not growing");
  const row = await H.waitFor(() => qa('[data-testid="monitor-run"]').find((r) => r.textContent.includes(a.name)), 5000);
  ok(/\d+%/.test(row.textContent) && /(MB|GB|KB)/.test(row.textContent), "run row lacks cpu/ram: " + row.textContent);
  await H.ipc.stopRun(run.id);
  await waitRun(run.id, ["killed", "success", "failed"], 15000);
  await H.waitFor(() => !qa('[data-testid="monitor-run"]').some((r) => r.textContent.includes(a.name)), 10000);
  H.click('[data-testid="monitor-nav"]'); // back to workbench
});

await t("orphan from previous session is reaped at startup and recorded as killed", async () => {
  // planted by orphan.sh before launch: detached http server + runs row 'orphan-e2e' (status running, pgid, port)
  const rec = await H.waitFor(async () => { const r = await H.ipc.getRun("orphan-e2e"); return r && r.status === "killed" ? r : null; }, 15000);
  ok(/orphaned process group \d+ .*stopped at startup/.test(rec.logTail || ""), "log tail lacks orphan note: " + rec.logTail);
  const info = JSON.parse(await H.ipc.readTextFile(F + "/orphan.json"));
  const ports = await H.ipc.listListeningPorts();
  ok(!ports.some((p) => p.port === info.port), "orphan port " + info.port + " still listening: " + JSON.stringify(ports.filter((p) => p.port === info.port)));
});

await t("listening ports: webapp run attributed to its run; stop via port table", async () => {
  const a = apps["webapp-app"];
  await H.openApp(a.id);
  const run = await H.ipc.startRun(a.id, {});
  await H.waitFor(() => { const f = q('[data-testid="webapp-frame"]'); return f && f.dataset.phase === "ready" ? f : null; }, 15000);
  const port = Number(q('[data-testid="webapp-frame"]').dataset.port);
  const hit = await H.waitFor(async () => (await H.ipc.listListeningPorts()).find((p) => p.port === port) || null, 10000);
  ok(hit.runId === run.id && hit.appId === a.id && hit.active, "port not attributed: " + JSON.stringify(hit));
  H.click('[data-testid="monitor-nav"]');
  const row = await H.waitFor(() => q('[data-testid="monitor-port"][data-run="' + run.id + '"]'), 10000);
  ok(row.textContent.includes("active"), "row not marked active: " + row.textContent);
  await H.ipc.killPortOwner(hit.pid);
  const fin = await waitRun(run.id, ["killed", "success", "failed"], 15000);
  ok(fin.status === "killed", "kill via port owner → " + fin.status);
  await H.waitFor(async () => !(await H.ipc.listListeningPorts()).some((p) => p.port === port), 10000);
  // a non-Homemade pid must be refused
  let refused = false;
  try { await H.ipc.killPortOwner(1); } catch (e) { refused = /not started by Homemade/.test(String(e && e.message || e)); }
  ok(refused, "killing a foreign pid was not refused");
  H.click('[data-testid="monitor-nav"]');
});

await t("local model: status, settings section, test prompt", async () => {
  const s = await H.ipc.modelStatus();
  await R("  model status: " + JSON.stringify(s));
  ok(s.provider === "apple" && s.available === true, "Apple model not available: " + JSON.stringify(s));
  H.click('[data-testid="settings"]');
  await H.waitFor(() => q('[data-testid="settings-model"]'), 5000);
  H.click('[data-testid="model-test"]');
  const res = await H.waitFor(() => { const el = q('[data-testid="model-test-result"]'); return el && el.textContent.trim().length > 3 ? el : null; }, 60000);
  await R("  model test: " + res.textContent.trim().slice(0, 120));
  ok(!/error/i.test(res.textContent), "model test errored: " + res.textContent);
  H.click('[data-testid="settings"]');
});

await t("local model: draft manifest from README (no argparse), agent prompt, apply", async () => {
  const d = await H.ipc.draftManifest(F + "/readme-app");
  await R("  draft usedModel=" + d.usedModel + " errors=" + JSON.stringify(d.validationErrors) + " notes=" + JSON.stringify(d.notes));
  await R("  draft toml:\n" + d.toml.split("\n").map((l) => "    " + l).join("\n"));
  ok(d.usedModel === true, "draft did not use the model");
  ok(d.validationErrors.length === 0, "draft has validation errors");
  ok(/thumbs\.py/.test(d.toml), "exec does not reference thumbs.py");
  ok(/\[inputs\./.test(d.toml) && /--src|--size/.test(d.toml), "inputs missing from draft (README documents --src/--size)");
  ok(d.agentPrompt.includes("Thumbnailer") && d.agentPrompt.includes("[app]"), "agent prompt lacks README facts or draft toml");
  // UI path: Draft modal via importing the folder first, then the header button
  const a = await H.ipc.importApp(F + "/readme-app");
  await H.openApp(a.id);
  H.click('[data-testid="draft-manifest"]');
  await H.waitFor(() => q('[data-testid="draft-modal"]'), 5000);
  const ta = await H.waitFor(() => { const el = q('[data-testid="draft-toml"]'); return el && el.value.includes("[app]") ? el : null; }, 90000);
  ok(q('[data-testid="draft-prompt"]') || qa('[data-testid="draft-modal"] button').some((b) => /prompt/i.test(b.textContent)), "agent prompt tab missing");
  H.click('[data-testid="draft-apply"]');
  await H.waitFor(() => !q('[data-testid="draft-modal"]'), 20000);
  const app2 = await H.ipc.getApp(a.id);
  ok(Object.keys(app2.manifest.inputs).length >= 2, "applied manifest has too few inputs: " + JSON.stringify(Object.keys(app2.manifest.inputs)));
  const run = await H.ipc.startRun(a.id, {});
  const fin = await waitRun(run.id, ["success", "failed", "killed"], 60000);
  ok(fin.status === "success", "readme-app run after apply: " + fin.status + " " + (fin.logTail || "").slice(-200));
  apps["readme-app"] = app2;
});

await t("local model: failed run gets an explanation (run://health) and Explain button", async () => {
  apps["fail-app"] = await H.ipc.importApp(F + "/fail-app");
  const a = apps["fail-app"];
  await H.openApp(a.id);
  const run = await H.ipc.startRun(a.id, {});
  const fin = await waitRun(run.id, ["failed", "success", "killed"], 30000);
  ok(fin.status === "failed", "fail-app should fail, got " + fin.status);
  const banner = await H.waitFor(() => { const el = q('[data-testid="run-health"]'); return el && el.textContent.trim().length > 10 ? el : null; }, 60000);
  await R("  health: [" + banner.dataset.severity + "] " + banner.textContent.trim().slice(0, 160));
  ok(/config\.yaml|missing|not found|file/i.test(banner.textContent), "explanation does not mention the missing file: " + banner.textContent);
  const ex = await H.ipc.explainRun(run.id);
  ok(ex && ex.message && ex.message.length > 10, "explainRun returned nothing");
});

await t("dedicated window: Open in new window pops the app out, dock event brings it back", async () => {
  const a = apps["hello-cli"];
  await H.openApp(a.id);
  const label = "app-" + a.id.replace(/[^a-zA-Z0-9_-]/g, "_");
  const panel = `[data-app-panel="${a.id}"] `;
  H.click(panel + '[data-testid="app-more"]');
  H.click(panel + '[data-testid="open-in-window"]');
  await H.waitFor(async () => (await H.windows()).includes(label), 15000);
  await H.waitFor(() => !H.store.getState().openApps.includes(a.id), 5000);
  ok(!q(panel), "tab still open in main window after pop-out");
  // Sidebar click while the window exists must focus the window, not open a second copy as a tab.
  H.store.getState().openApp(a.id);
  await sleep(500);
  ok(!H.store.getState().openApps.includes(a.id), "re-open created a tab while the window was open");
  await H.emit("app://dock", { appId: a.id });
  await H.waitFor(() => H.store.getState().openApps.includes(a.id) && q(panel), 5000);
  await H.closeWindow(label);
  await H.waitFor(async () => !(await H.windows()).includes(label), 10000);
});

await t("launch view: tiles list programs and open them in a solo window", async () => {
  H.store.getState().setView("launch");
  await H.waitFor(() => q('[data-testid="launch-view"]'), 5000);
  const tiles = qa('[data-testid="launch-tile"]');
  ok(tiles.length >= 8, "tiles " + tiles.length);
  const a = apps["hello-ui"];
  const label = "app-" + a.id.replace(/[^a-zA-Z0-9_-]/g, "_");
  H.click(`[data-testid="launch-tile"][data-app="${a.id}"]`);
  await H.waitFor(async () => (await H.windows()).includes(label), 15000);
  await H.closeWindow(label);
  await H.waitFor(async () => !(await H.windows()).includes(label), 10000);
  H.store.getState().setView("workbench");
});

await t("close guard: confirm-exit modal appears while a run is active", async () => {
  const a = apps["gpu-app"];
  const r = await H.ipc.startRun(a.id, {});
  await waitRun(r.id, ["running"], 10000);
  await window.__TAURI_INTERNALS__.invoke("plugin:window|close", { label: "main" }).catch((e) => R("  close invoke err: " + e));
  await H.waitFor(() => q('[data-testid="confirm-exit-modal"]'), 15000);
  await H.ipc.stopRun(r.id);
  await waitRun(r.id, ["killed"], 15000);
});

await R(`SUMMARY passes=${passes} fails=${fails}`);
await H.done(fails ? 1 : 0);
