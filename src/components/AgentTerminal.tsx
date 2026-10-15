// OWNER: agent F. xterm.js terminal bound to a backend PTY running a coding agent (or a shell) in the app folder.
import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { events, ipc, on } from "../lib/ipc";
import type { AgentSessionRecord, AgentType, DetectedAgent, PtyDataEvent, PtyExitEvent, PtyNoticeEvent } from "../lib/types";
import { errMsg, relTime } from "../lib/format";
import { Btn } from "./ui";
import { terminals } from "../lib/registry";
import { useStore } from "../state/store";
import { cssVar, useTheme } from "../lib/theme";

/** The terminal is always dark, whatever the app theme: agents print ANSI colors tuned for black backgrounds, and a
 * light terminal made bright-white / bright-yellow text unreadable. Classic 16-color palette with high contrast. */
function xtermTheme() {
  const accent = cssVar("--color-accent") || "#3b82f6";
  return {
    background: "#0b0d10", foreground: "#e6e6e6", cursor: accent, cursorAccent: "#0b0d10", selectionBackground: accent + "66",
    black: "#1b1f24", red: "#ff5f57", green: "#5af78e", yellow: "#f3f99d", blue: "#57c7ff", magenta: "#ff6ac1", cyan: "#9aedfe", white: "#f1f1f0",
    brightBlack: "#686868", brightRed: "#ff8a85", brightGreen: "#8dffb1", brightYellow: "#fbffb8", brightBlue: "#8fdaff", brightMagenta: "#ff9bd6", brightCyan: "#c6f5ff", brightWhite: "#ffffff",
  };
}

interface Launch { agentType: AgentType; sessionId?: string; n: number }

export default function AgentTerminal({ appId, initialAgentType }: { appId: string; initialAgentType?: AgentType }) {
  const host = useRef<HTMLDivElement>(null);
  const term = useRef<Terminal | null>(null);
  const fit = useRef<FitAddon | null>(null);
  const ptyId = useRef<string | null>(null);
  const pendingData = useRef(new Map<string, string[]>());
  const [agents, setAgents] = useState<DetectedAgent[] | null>(null);
  const [agentType, setAgentType] = useState<AgentType>("custom_shell");
  const [sessions, setSessions] = useState<AgentSessionRecord[]>([]);
  const [sessionId, setSessionId] = useState("");
  const [launch, setLaunch] = useState<Launch | null>(null);
  const [exited, setExited] = useState(false);
  const [curPty, setCurPty] = useState<string | null>(null);
  // `[homemade]` status lines are never typed by themselves: they wait here until the user presses Send.
  const [notices, setNotices] = useState<{ id: number; text: string }[]>([]);
  const dismiss = (id: number) => setNotices((n) => n.filter((x) => x.id !== id));
  const bindPty = (id: string | null) => {
    if (ptyId.current) terminals.delete(ptyId.current);
    ptyId.current = id;
    if (id && term.current) terminals.set(id, term.current);
    setCurPty(id);
  };

  // Explicit shell requests start immediately; agent discovery only fills the selector.
  useEffect(() => {
    let disposed = false;
    if (initialAgentType) {
      setAgentType(initialAgentType);
      setLaunch({ agentType: initialAgentType, n: 0 });
    }
    ipc.detectAgents().then((agents) => {
      if (disposed) return;
      setAgents(agents);
      if (!initialAgentType) {
        const type = agents[0]?.agentType ?? "custom_shell";
        setAgentType(type);
        setLaunch({ agentType: type, n: 0 });
      }
    }).catch(() => {
      if (disposed) return;
      setAgents([]);
      if (!initialAgentType) setLaunch({ agentType: "custom_shell", n: 0 });
    });
    return () => { disposed = true; };
  }, [initialAgentType]);
  const summaryTick = useStore((s) => s.sessionSummaryTick);
  useEffect(() => {
    ipc.listAgentSessions(appId).then(setSessions).catch(() => setSessions([]));
  }, [appId, launch, summaryTick]);

  // terminal + event plumbing (once)
  useEffect(() => {
    const t = new Terminal({
      fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace", fontSize: 12, cursorBlink: true, allowProposedApi: true, scrollback: 5000,
      theme: xtermTheme(),
      // Option acts as Meta (⌥b / ⌥f word moves, ⌥⏎ newline in agents) instead of typing ∫ ƒ.
      macOptionIsMeta: true,
    });
    // ⌘-combos belong to the launcher (⌘1…6 tabs, ⌘⇧C select, ⌘⇧D dock, ⌘C/⌘V native copy/paste); everything else —
    // Esc, Option/Meta, Ctrl — is the program's and must reach the PTY untouched.
    t.attachCustomKeyEventHandler((e) => !e.metaKey);
    const f = new FitAddon();
    t.loadAddon(f);
    t.open(host.current!);
    term.current = t; fit.current = f;
    try { f.fit(); } catch { /* not laid out yet */ }
    const sub = t.onData((d) => ptyId.current && ipc.ptyWrite(ptyId.current, d).catch(() => {}));

    let disposed = false;
    const un: UnlistenFn[] = [];
    on<PtyDataEvent>(events.ptyData, (p) => {
      if (p.ptyId === ptyId.current) t.write(p.data);
      else if (!ptyId.current) { const m = pendingData.current; m.set(p.ptyId, [...(m.get(p.ptyId) ?? []), p.data]); }
    }).then((u) => (disposed ? u() : un.push(u)));
    on<PtyNoticeEvent>(events.ptyNotice, (p) => {
      if (p.ptyId !== ptyId.current) return;
      const id = Date.now() + Math.random();
      setNotices((n) => [...n.slice(-2), { id, text: p.text }]);
      window.setTimeout(() => setNotices((n) => n.filter((x) => x.id !== id)), 15000); // toast: gone after 15 s unless sent
    }).then((u) => (disposed ? u() : un.push(u)));
    on<PtyExitEvent>(events.ptyExit, (p) => {
      if (p.ptyId !== ptyId.current) return;
      t.write(`\r\n\x1b[90m[process exited${p.exitCode != null ? ` with code ${p.exitCode}` : ""}]\x1b[0m\r\n`);
      setExited(true);
    }).then((u) => (disposed ? u() : un.push(u)));

    const ro = new ResizeObserver(() => {
      try { f.fit(); } catch { return; }
      if (ptyId.current) ipc.ptyResize(ptyId.current, t.cols, t.rows).catch(() => {});
    });
    ro.observe(host.current!);
    return () => {
      disposed = true; un.forEach((u) => u()); ro.disconnect(); sub.dispose();
      if (ptyId.current) { ipc.ptyKill(ptyId.current).catch(() => {}); terminals.delete(ptyId.current); }
      ptyId.current = null;
      t.dispose();
    };
  }, []);

  // re-apply colors when the app theme changes
  const theme = useTheme();
  useEffect(() => { if (term.current) term.current.options.theme = xtermTheme(); }, [theme]);

  // (re)spawn on launch change
  useEffect(() => {
    const t = term.current;
    if (!launch || !t) return;
    let cancelled = false;
    const prev = ptyId.current;
    bindPty(null);
    if (prev) ipc.ptyKill(prev).catch(() => {});
    t.reset();
    setExited(false);
    setNotices([]);
    pendingData.current.clear();
    try { fit.current?.fit(); } catch { /* ignore */ }
    ipc.ptySpawn(appId, launch.agentType, { sessionId: launch.sessionId, cols: t.cols, rows: t.rows }).then((info) => {
      if (cancelled) { ipc.ptyKill(info.ptyId).catch(() => {}); return; }
      bindPty(info.ptyId);
      for (const d of pendingData.current.get(info.ptyId) ?? []) t.write(d);
      pendingData.current.clear();
      t.focus();
    }).catch((e) => { t.write(`\x1b[31mFailed to start: ${errMsg(e)}\x1b[0m\r\n`); setExited(true); });
    return () => { cancelled = true; };
  }, [launch, appId]);

  const start = (resume: boolean) => setLaunch((l) => ({ agentType, sessionId: resume && sessionId ? sessionId : undefined, n: (l?.n ?? 0) + 1 }));
  const isShell = agentType === "custom_shell";

  return (
    <div data-testid="agent-terminal" data-pty-id={curPty ?? ""} className="flex h-full flex-col bg-bg">
      <div className="flex flex-wrap items-center gap-1.5 border-b border-line bg-panel px-2 py-1.5">
        <select data-testid="terminal-agent-select" className="field" value={agentType} onChange={(e) => { setAgentType(e.target.value as AgentType); setSessionId(""); }}>
          {agents?.map((a) => <option key={a.agentType} value={a.agentType}>{a.displayName}</option>)}
          <option value="custom_shell">Shell</option>
        </select>
        {!isShell && (
          <select data-testid="terminal-session-select" className="field max-w-56" value={sessionId} onChange={(e) => setSessionId(e.target.value)}>
            <option value="">New session</option>
            {sessions.filter((s) => s.agentType === agentType).map((s) => (
              <option key={s.sessionId} value={s.sessionId}>{(s.title || s.sessionId.slice(0, 8))} · {relTime(s.updatedAt)}</option>
            ))}
          </select>
        )}
        <Btn data-testid="terminal-new" onClick={() => start(false)}>{isShell ? "New shell" : "New session"}</Btn>
        {!isShell && <Btn disabled={!sessionId} onClick={() => start(true)}>Resume</Btn>}
        {exited && <Btn variant="primary" onClick={() => start(!!sessionId)}>Restart</Btn>}
      </div>
      <div className="relative min-h-0 flex-1 p-1.5" style={{ background: "#0b0d10" }}>
        <div ref={host} className="h-full w-full" />
        {notices.length > 0 && (
          <div data-testid="terminal-notices" className="pointer-events-none absolute inset-x-2 bottom-2 flex flex-col gap-1">
            {notices.map((n) => (
              <div key={n.id} className="pointer-events-auto flex items-center gap-2 rounded-md border border-line bg-panel/95 px-2 py-1.5 text-[11px] shadow-lg backdrop-blur">
                <span className="min-w-0 flex-1 truncate font-mono text-muted" title={n.text}>{n.text}</span>
                <Btn data-testid="notice-send" variant="primary" className="!px-2 !py-0.5 !text-[11px]" disabled={!curPty}
                  onClick={() => { if (curPty) ipc.ptyWrite(curPty, n.text + "\r").catch(() => {}); dismiss(n.id); }}>Send to agent</Btn>
                <button aria-label="Dismiss" className="text-muted hover:text-fg" onClick={() => dismiss(n.id)}>×</button>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
