// Module-level registries used by the window.__hm test API (no React deps).
import type { Terminal } from "@xterm/xterm";

/** Open xterm instances keyed by backend ptyId. */
export const terminals = new Map<string, Terminal>();
/** Per-app evaluator that runs code inside the app's custom-UI iframe (bridge `e2e_eval`). */
export const sandboxes = new Map<string, (code: string) => Promise<unknown>>();
/** Per-app workspace controls (one dockview per opened app), keyed by appId. */
export const workspaces = new Map<string, { addShell: () => void; closeActive: () => void }>();
