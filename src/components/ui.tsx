import { useEffect, useRef, useState, useId, type ButtonHTMLAttributes, type ReactNode } from "react";

type Variant = "default" | "primary" | "danger" | "ghost";
const variants: Record<Variant, string> = {
  default: "bg-panel border-line hover:bg-panel-2 text-fg",
  primary: "bg-accent border-accent text-on-accent hover:brightness-95 font-medium",
  danger: "bg-danger/90 border-danger text-on-danger hover:bg-danger",
  ghost: "bg-transparent border-transparent hover:bg-panel-2 text-fg",
};

export function Btn({ variant = "default", className = "", ...p }: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  return <button type="button" {...p} className={`ui-btn inline-flex justify-center items-center gap-1.5 rounded-md border px-2.5 py-[3px] text-[12px] leading-5 whitespace-nowrap disabled:opacity-40 disabled:pointer-events-none ${variants[variant]} ${className}`} />;
}

export function Badge({ children, color = "gray", title }: { children: ReactNode; color?: "gray" | "accent" | "green" | "amber" | "red"; title?: string }) {
  const c = { gray: "bg-panel-2 text-muted", accent: "bg-accent/15 text-accent", green: "bg-ok/15 text-ok", amber: "bg-warn/15 text-warn", red: "bg-danger/15 text-danger" }[color];
  return <span title={title} className={`inline-flex items-center rounded-md px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wide ${c}`}>{children}</span>;
}

export function Modal({ title, onClose, children, wide, testid }: { title: ReactNode; onClose: () => void; children: ReactNode; wide?: boolean; testid?: string }) {
  const titleId = useId();
  const [previousFocus] = useState(() => document.activeElement as HTMLElement | null);
  const dialogRef = useRef<HTMLDivElement>(null);
  const initialFocus = useRef<HTMLElement | null>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  useEffect(() => {
    const previous = previousFocus;
    const dialog = dialogRef.current;
    const focusable = () => Array.from(dialog?.querySelectorAll<HTMLElement>('button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), a[href], [tabindex="0"]') ?? []).filter((el) => el.getClientRects().length > 0);
    if (dialog?.contains(document.activeElement)) initialFocus.current = document.activeElement as HTMLElement;
    else (initialFocus.current ?? focusable()[0] ?? dialog)?.focus();
    const k = (e: KeyboardEvent) => {
      if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); closeRef.current(); }
      if (e.key === "Tab") {
        const items = focusable();
        const first = items[0], last = items[items.length - 1];
        if (!first) { e.preventDefault(); dialog?.focus(); }
        else if (e.shiftKey && (document.activeElement === first || !dialog?.contains(document.activeElement))) { e.preventDefault(); last.focus(); }
        else if (!e.shiftKey && (document.activeElement === last || !dialog?.contains(document.activeElement))) { e.preventDefault(); first.focus(); }
      }
    };
    document.addEventListener("keydown", k, true);
    return () => { document.removeEventListener("keydown", k, true); if (previous?.isConnected) previous.focus(); };
  }, [previousFocus]);
  return (
    <div data-testid={testid} className="fixed inset-0 z-50 flex items-center justify-center bg-backdrop p-4" onMouseDown={(e) => e.target === e.currentTarget && onClose()}>
      <div ref={dialogRef} role="dialog" aria-modal="true" aria-labelledby={titleId} tabIndex={-1} className={`ui-modal flex max-h-[90vh] max-w-full flex-col rounded-lg border border-line bg-panel shadow-2xl ${wide ? "w-[min(90vw,1100px)]" : "w-[420px]"}`}>
        <div className="flex items-center justify-between border-b border-line px-4 py-3">
          <div id={titleId} className="truncate text-[14px] font-semibold">{title}</div>
          <button className="rounded-md px-2 py-1 text-muted hover:bg-panel-2 hover:text-fg" onClick={onClose} aria-label="Close" title="Close (Esc)">✕</button>
        </div>
        <div className="min-h-0 flex-1 overflow-auto p-4">{children}</div>
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
export function Dropdown({ label, children, align = "left", disabled, testid, buttonClassName = "" }: { buttonClassName?: string; label: ReactNode; children: (close: () => void) => ReactNode; align?: "left" | "right"; disabled?: boolean; testid?: string }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const key = (e: KeyboardEvent) => { if (e.key === "Escape") { e.preventDefault(); e.stopPropagation(); setOpen(false); ref.current?.querySelector("button")?.focus(); } };
    document.addEventListener("keydown", key, true);
    const h = (e: MouseEvent) => ref.current && !ref.current.contains(e.target as Node) && setOpen(false);
    document.addEventListener("mousedown", h);
    return () => { document.removeEventListener("mousedown", h); document.removeEventListener("keydown", key, true); };
  }, [open]);
  return (
    <div ref={ref} className="relative">
      <Btn className={buttonClassName} data-testid={testid} disabled={disabled} aria-expanded={open} onClick={() => setOpen(!open)}>{label} <span className="text-[9px] text-muted">▼</span></Btn>
      {open && (
        <div className={`absolute top-full z-40 mt-1 max-h-80 min-w-48 overflow-auto rounded-md border border-line bg-panel p-1 shadow-xl ${align === "right" ? "right-0" : "left-0"}`}>
          {children(() => setOpen(false))}
        </div>
      )}
    </div>
  );
}

export function MenuItem({ onClick, children, disabled, testid }: { onClick: () => void; children: ReactNode; disabled?: boolean; testid?: string }) {
  return <button data-testid={testid} disabled={disabled} onClick={onClick} className="flex w-full items-center gap-2 rounded-md px-3 py-2 text-left hover:bg-panel-2 disabled:opacity-40">{children}</button>;
}

/** Run status pill (Underbar strip). */
export function RunStatus({ status, exitCode }: { status: string; exitCode: number | null }) {
  const color = ({ running: "green", stopping: "amber", queued_vram: "amber", success: "accent", failed: "red", killed: "gray" } as const)[status as "running"] ?? "gray";
  const label = status === "queued_vram" ? "queued" : status;
  return <span data-testid="run-status" data-status={status}><Badge color={color}>{label}{exitCode != null && status !== "success" ? ` · exit ${exitCode}` : ""}</Badge></span>;
}
