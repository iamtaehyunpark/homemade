// mode = "webapp": the app announces its port via `@homemade:port <n>` and we embed http://127.0.0.1:<port>.
// The bridge is NOT injected here (spec §7B); the page is a plain iframe.
import { useEffect, useState } from "react";
import type { AppRecord, RunRecord } from "../lib/types";
import { isActive } from "../lib/format";

export default function WebappFrame({ app, run, port }: { app: AppRecord; run: RunRecord | undefined; port: number | undefined }) {
  const [nonce, setNonce] = useState(0);
  useEffect(() => { setNonce((n) => n + 1); }, [port, run?.id]);
  const active = isActive(run?.status);
  if (!active || !port) {
    return (
      <div data-testid="webapp-frame" data-port="" className="flex h-full items-center justify-center text-gray-500">
        {active ? "Waiting for the app to announce its port (print `@homemade:port <n>`)…" : `Press Run to start ${app.name}.`}
      </div>
    );
  }
  const src = `http://127.0.0.1:${port}/`;
  return (
    <div data-testid="webapp-frame" data-port={String(port)} className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-line px-2 py-0.5 text-[11px] text-gray-500">
        <span className="font-mono">{src}</span>
        <div className="flex-1" />
        <button className="hover:text-gray-300" onClick={() => setNonce((n) => n + 1)} title="Reload">↻</button>
      </div>
      <iframe key={nonce} src={src} title={app.name} className="min-h-0 flex-1 border-0 bg-white" sandbox="allow-scripts allow-forms allow-same-origin allow-popups" />
    </div>
  );
}
