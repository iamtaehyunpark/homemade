import type { CSSProperties } from "react";

const paths = {
  home: "m3 10 9-7 9 7v10a1 1 0 0 1-1 1h-5v-7H9v7H4a1 1 0 0 1-1-1Z",
  grid: "M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h7v7h-7z",
  activity: "M3 12h4l3-8 4 16 3-8h4",
  settings: "M12 8a4 4 0 1 0 0 8 4 4 0 0 0 0-8 M12 2v3m0 14v3M2 12h3m14 0h3M5 5l2 2m10 10 2 2M5 19l2-2M17 7l2-2",
  search: "M10.5 3a7.5 7.5 0 1 0 0 15 7.5 7.5 0 0 0 0-15 M16 16l5 5",
  plus: "M12 5v14M5 12h14",
  arrow: "M5 12h14m-6-6 6 6-6 6",
  folder: "M3 7V5a1 1 0 0 1 1-1h6l2 3h8a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1Z",
  code: "m8 6-6 6 6 6m8-12 6 6-6 6m-3-15-2 18",
  window: "M3 8h18M7 5h.01M10 5h.01M4 2h16a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2Z",
  sidebar: "M9 3v18M4 3h16a1 1 0 0 1 1 1v16a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1Z M6 7h.01M6 11h.01",
  cursor: "m4 3 6 18 3-7 7-3Z M13 14l6 6",
  terminal: "m5 6 5 6-5 6m8 0h6",
  close: "m6 6 12 12M6 18 18 6",
  more: "M5 12h.01M12 12h.01M19 12h.01",
  pin: "M12 17v5M9 3h6l-1 7 3 3H7l3-3-1-7z",
  trash: "M4 7h16M10 11v6M14 11v6M6 7l1 13h10l1-13M9 7V4h6v3",
} as const;
export type IconName = keyof typeof paths;
export default function Icon({ name, size = 18, className = "", style }: { name: IconName; size?: number; className?: string; style?: CSSProperties }) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.65" strokeLinecap="round" strokeLinejoin="round" className={`shrink-0 ${className}`} style={style} aria-hidden="true"><path d={paths[name]} /></svg>;
}
