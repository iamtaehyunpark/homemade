import { useEffect, useRef, useState, type ButtonHTMLAttributes, type ReactNode } from "react";

type Variant = "default" | "primary" | "danger" | "ghost";
const variants: Record<Variant, string> = {
  default: "bg-panel-2 border-line hover:bg-line text-fg",
  primary: "bg-accent/90 border-accent text-on-accent hover:bg-accent font-medium",
  danger: "bg-danger/90 border-danger text-on-danger hover:bg-danger",
  ghost: "bg-transparent border-transparent hover:bg-panel-2 text-fg",
};

export function Btn({ variant = "default", className = "", ...p }: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  return <button {...p} className={`inline-flex items-center gap-1 rounded border px-2 py-[3px] text-[12px] leading-5 whitespace-nowrap disabled:opacity-40 disabled:pointer-events-none ${variants[variant]} ${className}`} />;
}

export function Badge({ children, color = "gray", title }: { children: ReactNode; color?: "gray" | "accent" | "green" | "amber" | "red"; title?: string }) {
  const c = { gray: "bg-panel-2 text-muted", accent: "bg-accent/15 text-accent", green: "bg-ok/15 text-ok", amber: "bg-warn/15 text-warn", red: "bg-danger/15 text-danger" }[color];
  return <span title={title} className={`rounded px-1.5 py-px text-[10px] font-medium uppercase tracking-wide ${c}`}>{children}</span>;
}

export function Modal({ title, onClose, children, wide, testid }: { title: ReactNode; onClose: () => void; children: ReactNode; wide?: boolean; testid?: string }) {
  useEffect(() => {
    const k = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", k);
    return () => window.removeEventListener("keydown", k);
  }, [onClose]);
  return (
    <div data-testid={testid} className="fixed inset-0 z-50 flex items-center justify-center bg-backdrop" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div className={`flex max-h-[90vh] flex-col rounded-lg border border-line bg-panel shadow-2xl ${wide ? "w-[min(90vw,1100px)]" : "w-[420px]"}`}>
        <div className="flex items-center justify-between border-b border-line px-3 py-2">
          <div className="truncate font-medium">{title}</div>
          <button className="text-muted hover:text-fg" onClick={onClose} aria-label="Close" title="Close (Esc)">✕</button>
        </div>
        <div className="min-h-0 flex-1 overflow-auto p-3">{children}</div>
      </div>
    </div>
  );
}

/** Small text-input dialog (window.prompt is unreliable in webviews). */
export function PromptModal({ title, label, initial = "", okLabel = "Save", onOk, onClose }: { title: string; label: string; initial?: string; okLabel?: string; onOk: (v: string) => void; onClose: () => void }) {
  const [v, setV] = useState(initial);
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => ref.current?.select(), []);
  const submit = () => { if (v.trim()) { onOk(v.trim()); onClose(); } };
  return (
    <Modal title={title} onClose={onClose}>
      <label className="mb-1 block text-muted">{label}</label>
      <input ref={ref} autoFocus className="field w-full" value={v} onChange={(e) => setV(e.target.value)} onKeyDown={(e) => e.key === "Enter" && submit()} />
      <div className="mt-3 flex justify-end gap-2">
        <Btn onClick={onClose}>Cancel</Btn>
        <Btn variant="primary" onClick={submit} disabled={!v.trim()}>{okLabel}</Btn>
      </div>
    </Modal>
  );
}

/** Click-outside dropdown menu. */
export function Dropdown({ label, children, align = "left", disabled, testid }: { label: ReactNode; children: (close: () => void) => ReactNode; align?: "left" | "right"; disabled?: boolean; testid?: string }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const h = (e: MouseEvent) => ref.current && !ref.current.contains(e.target as Node) && setOpen(false);
    document.addEventListener("mousedown", h);
    return () => document.removeEventListener("mousedown", h);
  }, [open]);
  return (
    <div ref={ref} className="relative">
      <Btn data-testid={testid} disabled={disabled} onClick={() => setOpen(!open)}>{label} <span className="text-[9px] text-muted">▼</span></Btn>
      {open && (
        <div className={`absolute top-full z-40 mt-1 max-h-80 min-w-48 overflow-auto rounded border border-line bg-panel py-1 shadow-xl ${align === "right" ? "right-0" : "left-0"}`}>
          {children(() => setOpen(false))}
        </div>
      )}
    </div>
  );
}

export function MenuItem({ onClick, children, disabled, testid }: { onClick: () => void; children: ReactNode; disabled?: boolean; testid?: string }) {
  return <button data-testid={testid} disabled={disabled} onClick={onClick} className="flex w-full items-center gap-2 px-3 py-1 text-left hover:bg-panel-2 disabled:opacity-40">{children}</button>;
}

/** Run status pill (Underbar strip). */
export function RunStatus({ status, exitCode }: { status: string; exitCode: number | null }) {
  const color = ({ running: "green", stopping: "amber", queued_vram: "amber", success: "accent", failed: "red", killed: "gray" } as const)[status as "running"] ?? "gray";
  const label = status === "queued_vram" ? "queued" : status;
  return <span data-testid="run-status" data-status={status}><Badge color={color}>{label}{exitCode != null && status !== "success" ? ` · exit ${exitCode}` : ""}</Badge></span>;
}
