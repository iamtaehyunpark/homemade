// OWNER: agent F. Renders manifest.inputs (declaration order) as a dense form.
import { open } from "@tauri-apps/plugin-dialog";
import type { HomemadeManifest, InputFieldSpec } from "../lib/types";
import { fieldLabel } from "../lib/form";
import { basename } from "../lib/format";
import { useDropTarget } from "../lib/dropRouter";
import { Btn } from "./ui";

interface Props { manifest: HomemadeManifest; values: Record<string, unknown>; onChange: (v: Record<string, unknown>) => void; errors: Record<string, string> }

export default function AutoForm({ manifest, values, onChange, errors }: Props) {
  const entries = Object.entries(manifest.inputs);
  const set = (k: string, v: unknown) => onChange({ ...values, [k]: v });
  return (
    <div className="mx-auto max-w-3xl px-4 py-3">
      {manifest.app.description && <p className="mb-3 text-[12px] text-muted">{manifest.app.description}</p>}
      {!entries.length && <p className="text-muted">This app takes no inputs. Press Run.</p>}
      <div className="grid grid-cols-[minmax(110px,180px)_1fr] items-start gap-x-3 gap-y-2.5">
        {entries.map(([k, f]) => (
          <Row key={k} name={k} spec={f} value={values[k]} error={errors[k]} onChange={(v) => set(k, v)} />
        ))}
      </div>
    </div>
  );
}

function Row({ name, spec, value, error, onChange }: { name: string; spec: InputFieldSpec; value: unknown; error?: string; onChange: (v: unknown) => void }) {
  return (
    <>
      <label htmlFor={`f-${name}`} className="pt-1 text-right text-[12px] text-fg" title={spec.cli_flag ?? undefined}>
        {fieldLabel(name, spec)}{spec.required && <span className="text-danger"> *</span>}
      </label>
      <div className="min-w-0">
        <Widget name={name} spec={spec} value={value} onChange={onChange} invalid={!!error} />
        {(error || spec.description) && <div className={`mt-0.5 text-[11px] ${error ? "text-danger" : "text-muted"}`}>{error ?? spec.description}</div>}
      </div>
    </>
  );
}

function Widget({ name, spec, value, onChange, invalid }: { name: string; spec: InputFieldSpec; value: unknown; onChange: (v: unknown) => void; invalid: boolean }) {
  const id = `f-${name}`;
  switch (spec.type) {
    case "file": case "directory": case "file_list":
      return <PathField spec={spec} value={value} onChange={onChange} invalid={invalid} id={id} />;
    case "bool":
      return (
        <button id={id} role="switch" aria-checked={!!value} onClick={() => onChange(!value)}
          className={`mt-1 h-4 w-7 rounded-full p-0.5 transition-colors ${value ? "bg-accent" : "bg-line"}`}>
          <span className={`block h-3 w-3 rounded-full bg-panel transition-transform ${value ? "translate-x-3" : ""}`} />
        </button>
      );
    case "select":
      return (
        <select id={id} className="field w-full" value={String(value ?? "")} onChange={(e) => onChange(e.target.value)} aria-invalid={invalid}>
          {!spec.required && !spec.options?.includes(String(value ?? "")) && <option value="" />}
          {spec.options?.map((o) => <option key={o} value={o}>{o}</option>)}
        </select>
      );
    case "range": {
      const n = Number(value ?? spec.min ?? 0);
      return (
        <div className="flex items-center gap-2">
          <input id={id} type="range" className="flex-1 accent-accent" min={spec.min ?? 0} max={spec.max ?? 100} step={spec.step ?? 1} value={n} onChange={(e) => onChange(Number(e.target.value))} />
          <span className="w-12 text-right font-mono text-[12px] tabular-nums">{n}</span>
        </div>
      );
    }
    case "int": case "float":
      return (
        <input id={id} type="number" className="field w-40" aria-invalid={invalid} value={value === undefined || value === null ? "" : String(value)}
          min={spec.min ?? undefined} max={spec.max ?? undefined} step={spec.step ?? (spec.type === "int" ? 1 : "any")}
          onChange={(e) => onChange(e.target.value === "" ? "" : Number(e.target.value))} />
      );
    default:
      return <input id={id} className="field w-full" aria-invalid={invalid} value={String(value ?? "")} onChange={(e) => onChange(e.target.value)} />;
  }
}

function PathField({ spec, value, onChange, invalid, id }: { spec: InputFieldSpec; value: unknown; onChange: (v: unknown) => void; invalid: boolean; id: string }) {
  const multi = spec.type === "file_list";
  const list = multi ? (Array.isArray(value) ? (value as string[]) : []) : [];
  const ref = useDropTarget<HTMLDivElement>((paths) => onChange(multi ? [...list, ...paths] : paths[0]));
  const browse = async () => {
    const exts = spec.extensions?.map((e) => e.replace(/^\./, "")).filter(Boolean);
    const r = await open({
      directory: spec.type === "directory",
      multiple: multi,
      filters: exts?.length && spec.type !== "directory" ? [{ name: exts.join(", "), extensions: exts }] : undefined,
    });
    if (r == null) return;
    if (multi) onChange([...list, ...(Array.isArray(r) ? r : [r])]);
    else onChange(Array.isArray(r) ? r[0] : r);
  };
  return (
    <div ref={ref} className="rounded">
      <div className="flex gap-1.5">
        {multi
          ? <div id={id} className="field flex-1 truncate text-muted" aria-invalid={invalid}>{list.length ? `${list.length} file${list.length > 1 ? "s" : ""}` : "Drop files or browse"}</div>
          : <input id={id} readOnly className="field flex-1 font-mono" aria-invalid={invalid} value={String(value ?? "")} placeholder={spec.type === "directory" ? "Drop a folder or browse" : "Drop a file or browse"} title={String(value ?? "")} />}
        <Btn onClick={browse}>Browse</Btn>
        {(multi ? list.length > 0 : !!value) && <Btn variant="ghost" onClick={() => onChange(multi ? [] : "")} title="Clear">✕</Btn>}
      </div>
      {multi && list.length > 0 && (
        <ul className="mt-1 max-h-28 overflow-y-auto rounded border border-line text-[11px]">
          {list.map((p, i) => (
            <li key={p + i} className="flex items-center px-2 py-0.5 hover:bg-panel-2" title={p}>
              <span className="flex-1 truncate font-mono">{basename(p)}</span>
              <button className="text-faint hover:text-danger" title="Remove" onClick={() => onChange(list.filter((_, j) => j !== i))}>✕</button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
