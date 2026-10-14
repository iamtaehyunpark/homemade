// Theme preference (light | dark | system), persisted in localStorage "homemade.theme", applied as data-theme on <html>.
import { useEffect, useState } from "react";

export type ThemePref = "light" | "dark" | "system";
export type Theme = "light" | "dark";
const KEY = "homemade.theme";
const EVT = "homemade-theme";

export function getThemePref(): ThemePref {
  try { const v = localStorage.getItem(KEY); if (v === "light" || v === "dark" || v === "system") return v; } catch { /* ignore */ }
  return "light";
}
const media = () => window.matchMedia?.("(prefers-color-scheme: dark)");
export function resolveTheme(p: ThemePref = getThemePref()): Theme {
  return p === "system" ? (media()?.matches ? "dark" : "light") : p;
}
export function applyTheme() {
  const t = resolveTheme();
  document.documentElement.setAttribute("data-theme", t);
  window.dispatchEvent(new CustomEvent(EVT, { detail: t }));
}
export function setThemePref(p: ThemePref) {
  try { localStorage.setItem(KEY, p); } catch { /* ignore */ }
  applyTheme();
}

let mediaBound = false;
function bindMedia() {
  if (mediaBound) return;
  mediaBound = true;
  media()?.addEventListener?.("change", () => { if (getThemePref() === "system") applyTheme(); });
}

/** The currently applied theme; re-renders when it changes. */
export function useTheme(): Theme {
  const [t, setT] = useState<Theme>(() => resolveTheme());
  useEffect(() => {
    bindMedia();
    const h = () => setT(resolveTheme());
    window.addEventListener(EVT, h);
    return () => window.removeEventListener(EVT, h);
  }, []);
  return t;
}

/** Reads a CSS token (e.g. "--color-bg") from <html>. */
export function cssVar(name: string): string {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}
