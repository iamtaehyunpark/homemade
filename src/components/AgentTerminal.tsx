// OWNER: agent F. xterm.js terminal bound to a backend PTY running a coding agent (or a shell) in the app folder.
import { useEffect, useRef, useState } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { events, ipc, on } from "../lib/ipc";
import type { AgentSessionRecord, AgentType, DetectedAgent, PtyDataEvent, PtyExitEvent } from "../lib/types";
import { errMsg, relTime } from "../lib/format";
import { Btn } from "./ui";

interface Launch { agentType: AgentType; sessionId?: string; n: number }

export default function AgentTerminal({ appId }: { appId: string }) {
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

  // detect agents, then auto-launch the first one
  useEffect(() => {
    ipc.detectAgents().then((a) => {
      setAgents(a);
      const t = a[0]?.agentType ?? "custom_shell";
      setAgentType(t);
      setLaunch({ agentType: t, n: 0 });
    }).catch(() => { setAgents([]); setLaunch({ agentType: "custom_shell", n: 0 }); });
  }, []);
  useEffect(() => {
    ipc.listAgentSessions(appId).then(setSessions).catch(() => setSessions([]));
  }, [appId, launch]);

  // terminal + event plumbing (once)
  useEffect(() => {
    const t = new Terminal({
      fontFamily: "ui-monospace, SFMono-Regular, Menlo, monospace", fontSize: 12, cursorBlink: true, allowProposedApi: true, scrollback: 5000,
      theme: { background: "#0f1115", foreground: "#e6e6e6", cursor: "#6ea8fe", selectionBackground: "#6ea8fe44" },
    });
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
    on<PtyExitEvent>(events.ptyExit, (p) => {
      if (p.ptyId !== ptyId.current) return;
      t.write(`\r\n\x1b[90m[process exited${p.exitCode != null ? ` with code ${p.exitCode}` : ""}]\x1b[0m\r\n`);
      ptyId.current = null;
      setExited(true);
    }).then((u) => (disposed ? u() : un.push(u)));

    const ro = new ResizeObserver(() => {
      try { f.fit(); } catch { return; }
      if (ptyId.current) ipc.ptyResize(ptyId.current, t.cols, t.rows).catch(() => {});
    });
    ro.observe(host.current!);
    return () => {
      disposed = true; un.forEach((u) => u()); ro.disconnect(); sub.dispose();
      if (ptyId.current) ipc.ptyKill(ptyId.current).catch(() => {});
      ptyId.current = null;
      t.dispose();
    };
  }, []);

  // (re)spawn on launch change
  useEffect(() => {
    const t = term.current;
    if (!launch || !t) return;
    let cancelled = false;
    const prev = ptyId.current;
    ptyId.current = null;
    if (prev) ipc.ptyKill(prev).catch(() => {});
    t.reset();
    setExited(false);
    pendingData.current.clear();
    try { fit.current?.fit(); } catch { /* ignore */ }
    ipc.ptySpawn(appId, launch.agentType, { sessionId: launch.sessionId, cols: t.cols, rows: t.rows }).then((info) => {
      if (cancelled) { ipc.ptyKill(info.ptyId).catch(() => {}); return; }
      ptyId.current = info.ptyId;
      for (const d of pendingData.current.get(info.ptyId) ?? []) t.write(d);
      pendingData.current.clear();
      t.focus();
    }).catch((e) => { t.write(`\x1b[31mFailed to start: ${errMsg(e)}\x1b[0m\r\n`); setExited(true); });
    return () => { cancelled = true; };
  }, [launch, appId]);

  const start = (resume: boolean) => setLaunch((l) => ({ agentType, sessionId: resume && sessionId ? sessionId : undefined, n: (l?.n ?? 0) + 1 }));
  const isShell = agentType === "custom_shell";

  return (
    <div className="flex h-full flex-col bg-bg">
      <div className="flex flex-wrap items-center gap-1.5 border-b border-line bg-panel px-2 py-1.5">
        <select className="field" value={agentType} onChange={(e) => { setAgentType(e.target.value as AgentType); setSessionId(""); }}>
          {agents?.map((a) => <option key={a.agentType} value={a.agentType}>{a.displayName}</option>)}
          <option value="custom_shell">Shell</option>
        </select>
        {!isShell && (
          <select className="field max-w-56" value={sessionId} onChange={(e) => setSessionId(e.target.value)}>
            <option value="">New session</option>
            {sessions.filter((s) => s.agentType === agentType).map((s) => (
              <option key={s.sessionId} value={s.sessionId}>{(s.title || s.sessionId.slice(0, 8))} · {relTime(s.updatedAt)}</option>
            ))}
          </select>
        )}
        <Btn onClick={() => start(false)}>{isShell ? "New shell" : "New session"}</Btn>
        {!isShell && <Btn disabled={!sessionId} onClick={() => start(true)}>Resume</Btn>}
        {exited && <Btn variant="primary" onClick={() => start(!!sessionId)}>Restart</Btn>}
      </div>
      <div className="min-h-0 flex-1 p-1.5"><div ref={host} className="h-full w-full" /></div>
    </div>
  );
}
