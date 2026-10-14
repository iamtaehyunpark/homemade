// Settings: secrets vault, storage summary, agent skill install, detected agents.
import { useEffect, useState, type ReactNode } from "react";
import { ipc } from "../lib/ipc";
import type { DetectedAgent, StorageSummary } from "../lib/types";
import { errMsg, fmtBytes } from "../lib/format";
import { useStore } from "../state/store";
import { Btn } from "./ui";

function Section({ title, children, action }: { title: string; children: ReactNode; action?: ReactNode }) {
  return (
    <section className="mb-6 rounded-lg border border-line bg-panel">
      <div className="flex items-center justify-between border-b border-line px-3 py-2">
        <h2 className="text-[12px] font-semibold uppercase tracking-wider text-gray-400">{title}</h2>{action}
      </div>
      <div className="p-3">{children}</div>
    </section>
  );
}

export default function Settings() {
  const notify = useStore((s) => s.notify);
  const [secrets, setSecrets] = useState<string[]>([]);
  const [name, setName] = useState("");
  const [value, setValue] = useState("");
  const [storage, setStorage] = useState<StorageSummary | null>(null);
  const [agents, setAgents] = useState<DetectedAgent[] | null>(null);
  const [skillPath, setSkillPath] = useState<string | null>(null);
  const [home, setHome] = useState("");

  const loadSecrets = () => ipc.listSecretNames().then(setSecrets).catch((e) => notify(errMsg(e)));
  useEffect(() => {
    loadSecrets();
    ipc.storageSummary().then(setStorage).catch(() => {});
    ipc.detectAgents().then(setAgents).catch(() => setAgents([]));
    ipc.homemadeHome().then(setHome).catch(() => {});
  }, []);

  const add = async () => {
    if (!name.trim() || !value) return;
    try { await ipc.setSecret(name.trim(), value); setName(""); setValue(""); loadSecrets(); } catch (e) { notify(errMsg(e)); }
  };

  return (
    <div data-testid="settings-view" className="h-full overflow-y-auto">
      <div className="mx-auto max-w-2xl px-6 py-6">
        <h1 className="mb-1 text-lg font-semibold">Settings</h1>
        <p className="mb-5 text-[12px] text-gray-500">{home}</p>

        <Section title="Secrets vault">
          <p className="mb-2 text-[12px] text-gray-500">Stored in the system keychain. Apps request them by name in <code>environment.secrets</code>.</p>
          <div data-testid="settings-secrets" className="mb-3 divide-y divide-line rounded border border-line">
            {secrets.map((s) => (
              <div key={s} className="flex items-center px-2 py-1.5">
                <code className="flex-1">{s}</code><span className="mr-3 text-gray-600">••••••••</span>
                <Btn variant="ghost" onClick={() => ipc.deleteSecret(s).then(loadSecrets).catch((e) => notify(errMsg(e)))}>Delete</Btn>
              </div>
            ))}
            {!secrets.length && <div className="px-2 py-1.5 text-gray-500">No secrets stored.</div>}
          </div>
          <div className="flex gap-2">
            <input className="field w-48 font-mono" placeholder="OPENAI_API_KEY" value={name} onChange={(e) => setName(e.target.value.toUpperCase().replace(/[^A-Z0-9_]/g, "_"))} />
            <input className="field flex-1" type="password" placeholder="Value" value={value} onChange={(e) => setValue(e.target.value)} onKeyDown={(e) => e.key === "Enter" && add()} />
            <Btn variant="primary" onClick={add} disabled={!name.trim() || !value}>Add</Btn>
          </div>
        </Section>

        <Section title="Storage" action={<Btn variant="ghost" onClick={() => ipc.storageSummary().then(setStorage).catch(() => {})}>Refresh</Btn>}>
          {storage ? (
            <dl className="grid grid-cols-[180px_1fr] gap-y-1 text-[12px]">
              <dt className="text-gray-500">Store root</dt><dd className="truncate font-mono" title={storage.storeRoot}>{storage.storeRoot}</dd>
              <dt className="text-gray-500">uv cache</dt><dd>{fmtBytes(storage.uvCacheBytes)}</dd>
              <dt className="text-gray-500">pnpm store</dt><dd>{fmtBytes(storage.pnpmStoreBytes)}</dd>
              <dt className="text-gray-500">Shared model cache</dt><dd>{fmtBytes(storage.sharedCacheBytes)}</dd>
              <dt className="text-gray-500">Unmanaged venvs</dt><dd className={storage.unmanagedVenvTotalBytes ? "text-amber-400" : ""}>{fmtBytes(storage.unmanagedVenvTotalBytes)}</dd>
            </dl>
          ) : <div className="text-gray-500">Loading…</div>}
        </Section>

        <ModelSection />

        <Section title="Coding agents">
          <div className="mb-3 space-y-1">
            {agents === null && <div className="text-gray-500">Detecting…</div>}
            {agents?.map((a) => (
              <div key={a.agentType} className="flex items-center gap-2"><span className="h-1.5 w-1.5 rounded-full bg-emerald-400" /><span className="w-32">{a.displayName}</span><code className="truncate text-gray-500">{a.binaryPath}</code></div>
            ))}
            {agents?.length === 0 && <div className="text-gray-500">No supported agents found on PATH.</div>}
          </div>
          <div className="flex items-center gap-3">
            <Btn onClick={() => ipc.installSkill().then(setSkillPath).catch((e) => notify(errMsg(e)))}>Install agent skill</Btn>
            {skillPath && <span className="truncate text-[12px] text-emerald-400">Installed to {skillPath}</span>}
          </div>
        </Section>
      </div>
    </div>
  );
}

function ModelSection() {
  const status = useStore((s) => s.modelStatus);
  const setStatus = useStore((s) => s.setModelStatus);
  const [testing, setTesting] = useState(false);
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  const refresh = () => ipc.modelStatus().then(setStatus).catch(() => {});
  useEffect(() => { refresh(); }, []);
  const test = async () => {
    setTesting(true); setResult(null);
    const t0 = performance.now();
    try {
      const out = await ipc.modelComplete("You are a terse assistant.", "Reply with the single word: ok");
      setResult({ ok: true, text: `${Math.round(performance.now() - t0)} ms · ${out.trim().slice(0, 80)}` });
    } catch (e) { setResult({ ok: false, text: errMsg(e) }); }
    setTesting(false);
  };
  return (
    <Section title="Local model" action={<Btn variant="ghost" onClick={refresh}>Refresh</Btn>}>
      <div data-testid="settings-model" data-available={status ? String(status.available) : ""}>
        {status ? (
          <dl className="grid grid-cols-[180px_1fr] gap-y-1 text-[12px]">
            <dt className="text-gray-500">Provider</dt><dd>{status.provider === "apple" ? "Apple on-device Foundation Model" : status.provider}</dd>
            <dt className="text-gray-500">Availability</dt>
            <dd className={status.available ? "text-emerald-400" : "text-amber-400"}>{status.available ? "Available" : "Unavailable"}</dd>
            {status.reason && (<>
              <dt className="text-gray-500">Reason</dt>
              <dd>{status.reason}{status.reason === "appleIntelligenceNotEnabled" && <span className="ml-2 text-gray-400">— Enable Apple Intelligence in System Settings</span>}</dd>
            </>)}
            <dt className="text-gray-500">Helper</dt><dd className="truncate font-mono" title={status.helperPath ?? ""}>{status.helperPath ?? "—"}</dd>
          </dl>
        ) : <div className="text-gray-500">Checking…</div>}
        <div className="mt-3 flex items-center gap-3">
          <Btn data-testid="model-test" disabled={testing} onClick={test}>{testing ? "Testing…" : "Test"}</Btn>
          {result && <span data-testid="model-test-result" data-ok={String(result.ok)} className={`truncate text-[12px] ${result.ok ? "text-emerald-400" : "text-red-400"}`}>{result.text}</span>}
        </div>
      </div>
    </Section>
  );
}
