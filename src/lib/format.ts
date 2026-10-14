export function fmtBytes(n: number | null | undefined): string {
  if (n == null || !isFinite(n)) return "–";
  const u = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = n;
  while (v >= 1024 && i < u.length - 1) { v /= 1024; i++; }
  return `${v >= 100 || i === 0 ? v.toFixed(0) : v.toFixed(1)} ${u[i]}`;
}
export const fmtMb = (mb: number) => fmtBytes(mb * 1024 * 1024);

/** Accepts seconds or milliseconds since epoch. */
export function toMs(t: number): number { return t < 1e12 ? t * 1000 : t; }

export function relTime(t: number | null | undefined): string {
  if (!t) return "";
  const s = Math.round((Date.now() - toMs(t)) / 1000);
  if (s < 45) return "just now";
  if (s < 3600) return `${Math.round(s / 60)}m ago`;
  if (s < 86400) return `${Math.round(s / 3600)}h ago`;
  if (s < 86400 * 30) return `${Math.round(s / 86400)}d ago`;
  return new Date(toMs(t)).toLocaleDateString();
}

export const basename = (p: string) => p.split(/[\\/]/).filter(Boolean).pop() ?? p;

export const isActive = (s?: string | null) => s === "running" || s === "stopping" || s === "queued_vram";
export const isTerminal = (s?: string | null) => s === "success" || s === "failed" || s === "killed";

export function platformName(): string {
  const p = (navigator.platform || navigator.userAgent).toLowerCase();
  if (p.includes("mac")) return "macos";
  if (p.includes("win")) return "windows";
  return "linux";
}

export function errMsg(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  try { return JSON.stringify(e); } catch { return String(e); }
}
