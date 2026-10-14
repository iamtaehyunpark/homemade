// window.__hm: test/automation API used by the HOMEMADE_E2E harness (CONTRACT.md "Gap-closing pass"). Harmless in production.
import { ipc, events, on } from "./ipc";
import { useStore } from "../state/store";
import { useLive } from "../state/live";
import { sandboxes, terminals } from "./registry";

function waitFor<T>(fn: () => T, timeoutMs = 10000): Promise<NonNullable<T>> {
  const t0 = Date.now();
  return new Promise((resolve, reject) => {
    const tick = () => {
      let v: T | undefined;
      try { v = fn(); } catch { v = undefined; }
      if (v) return resolve(v as NonNullable<T>);
      if (Date.now() - t0 > timeoutMs) return reject(new Error(`waitFor timed out after ${timeoutMs}ms: ${fn.toString().slice(0, 120)}`));
      setTimeout(tick, 100);
    };
    tick();
  });
}

function q(sel: string): HTMLElement {
  const el = document.querySelector<HTMLElement>(sel);
  if (!el) throw new Error(`No element matches ${sel}`);
  return el;
}

function setInput(sel: string, value: unknown) {
  const el = q(sel) as HTMLInputElement | HTMLTextAreaElement | HTMLSelectElement;
  if (el instanceof HTMLInputElement && (el.type === "checkbox" || el.type === "radio")) {
    if (el.checked !== !!value) el.click();
    return;
  }
  const proto = el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : el instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(proto, "value")!.set!.call(el, String(value));
  el.dispatchEvent(new Event("input", { bubbles: true }));
  el.dispatchEvent(new Event("change", { bubbles: true }));
}

async function openApp(appId: string): Promise<void> {
  const st = useStore.getState();
  // Refresh this app from the backend so the store reflects what importApp/scanVenv just persisted
  // (the real Sidebar does this upsert itself after an import).
  try { st.upsertApp(await ipc.getApp(appId)); } catch { if (!st.apps.some((a) => a.id === appId)) { st.setApps(await ipc.listApps()); } }
  await waitFor(() => useStore.getState().apps.some((a) => a.id === appId), 10000);
  const app = useStore.getState().apps.find((a) => a.id === appId);
  useStore.getState().openApp(appId);
  if (app?.manifest.ui.default_layout === "window") return; // lives in its own webview window
  const p = `[data-app-panel="${CSS.escape(appId)}"]`;
  await waitFor(() => document.querySelector(`${p} [data-testid="run-button"], ${p} [data-testid="stop-button"]`) ?? document.querySelector(p), 15000);
}

async function sandboxEval(appId: string, code: string): Promise<unknown> {
  const sel = `[data-app-panel="${CSS.escape(appId)}"] [data-testid="app-sandbox"][data-bridge="ready"]`;
  await waitFor(() => sandboxes.has(appId) && document.querySelector(sel), 15000);
  return sandboxes.get(appId)!(code);
}

function termText(ptyId: string): string {
  const t = terminals.get(ptyId);
  if (!t) return "";
  const buf = t.buffer.active;
  const out: string[] = [];
  for (let i = 0; i < buf.length; i++) out.push(buf.getLine(i)?.translateToString(true) ?? "");
  return out.join("\n").replace(/\n+$/, "");
}

export function installTestApi() {
  (window as unknown as { __hm: unknown }).__hm = {
    ipc, events, on,
    store: useStore, live: useLive,
    openApp,
    click: (sel: string) => q(sel).click(),
    setInput,
    waitFor,
    sandboxEval,
    termText,
    report: (msg: unknown) => ipc.e2eReport(typeof msg === "string" ? msg : JSON.stringify(msg)),
    done: (code: number) => ipc.e2eDone(code),
  };
}
