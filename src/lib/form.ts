import type { HomemadeManifest, InputFieldSpec } from "./types";

export function fieldDefault(f: InputFieldSpec): unknown {
  if (f.default !== undefined && f.default !== null) return f.default;
  switch (f.type) {
    case "bool": return false;
    case "file_list": return [];
    case "select": return f.options?.[0] ?? "";
    case "range": return f.min ?? 0;
    default: return "";
  }
}

export function defaultValues(m: HomemadeManifest): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const [k, f] of Object.entries(m.inputs)) out[k] = fieldDefault(f);
  return out;
}

const empty = (v: unknown) => v === undefined || v === null || v === "" || (Array.isArray(v) && v.length === 0);

export function validate(m: HomemadeManifest, values: Record<string, unknown>): Record<string, string> {
  const errs: Record<string, string> = {};
  for (const [k, f] of Object.entries(m.inputs)) {
    const v = values[k];
    if (f.required && f.type !== "bool" && empty(v)) { errs[k] = "Required"; continue; }
    if ((f.type === "int" || f.type === "float") && !empty(v)) {
      const n = Number(v);
      if (!isFinite(n)) errs[k] = "Not a number";
      else if (f.type === "int" && !Number.isInteger(n)) errs[k] = "Must be an integer";
      else if (f.min != null && n < f.min) errs[k] = `Min ${f.min}`;
      else if (f.max != null && n > f.max) errs[k] = `Max ${f.max}`;
    }
  }
  return errs;
}

export const fieldLabel = (key: string, f: InputFieldSpec) => f.label || key.replace(/[_-]+/g, " ").replace(/^\w/, (c) => c.toUpperCase());
