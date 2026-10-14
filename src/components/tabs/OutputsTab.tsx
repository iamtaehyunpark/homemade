// Underbar → Outputs: the OutputShelf for the viewed run.
import OutputShelf from "../OutputShelf";

export default function OutputsTab({ appId, runId, onCount }: { appId: string; runId: string | null; onCount: (n: number) => void }) {
  return <OutputShelf appId={appId} runId={runId} onCount={onCount} />;
}
