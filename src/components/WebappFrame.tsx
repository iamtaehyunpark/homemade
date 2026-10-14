// mode = "webapp": the app announces its port via `@homemade:port <n>`; the backend probes the port and emits
// `run://webapp_ready` once it accepts connections. Until then we show a loading state instead of a blank iframe
// (WebKit renders nothing on connection refused and never retries). The bridge is NOT injected here (spec §7B).
import { useEffect, useState } from "react";
import type { AppRecord, RunRecord } from "../lib/types";
import { isActive } from "../lib/format";

const Placeholder = ({ children, spinning, testPhase }: { children: React.ReactNode; spinning?: boolean; testPhase: string }) => (
  <div data-testid="webapp-frame" data-port="" data-phase={testPhase} className="flex h-full flex-col items-center justify-center gap-3 text-muted">
    {spinning && <div className="h-6 w-6 animate-spin rounded-full border-2 border-faint border-t-accent" />}
    <div className="max-w-md text-center">{children}</div>
  </div>
);

export default function WebappFrame({ app, run, port, ready, chrome = true }: { app: AppRecord; run: RunRecord | undefined; port: number | undefined; ready: boolean; chrome?: boolean }) {
  const [nonce, setNonce] = useState(0);
  useEffect(() => { setNonce((n) => n + 1); }, [port, run?.id, ready]);
  const active = isActive(run?.status);
  if (!active) return <Placeholder testPhase="idle">Press Run to start {app.name}.</Placeholder>;
  if (!port) return <Placeholder spinning testPhase="waiting-port">Starting… waiting for the app to announce its port (<code>@homemade:port &lt;n&gt;</code>).</Placeholder>;
  const src = `http://127.0.0.1:${port}/`;
  if (!ready) {
    return (
      <div data-testid="webapp-frame" data-port={String(port)} data-phase="waiting-server" className="flex h-full flex-col items-center justify-center gap-3 text-muted">
        <div className="h-6 w-6 animate-spin rounded-full border-2 border-faint border-t-accent" />
        <div>Loading <span className="font-mono">{src}</span> — waiting for the server to accept connections…</div>
      </div>
    );
  }
  return (
    <div data-testid="webapp-frame" data-port={String(port)} data-phase="ready" className="flex h-full flex-col">
      {chrome && <div className="flex items-center gap-2 border-b border-line px-2 py-0.5 text-[11px] text-muted">
        <span className="font-mono">{src}</span>
        <div className="flex-1" />
        <button data-testid="webapp-reload" className="hover:text-fg" onClick={() => setNonce((n) => n + 1)} title="Reload">↻</button>
      </div>}
      <iframe key={nonce} src={src} title={app.name} className="min-h-0 flex-1 border-0 bg-white" sandbox="allow-scripts allow-forms allow-same-origin allow-popups" />
    </div>
  );
}
