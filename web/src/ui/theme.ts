/**
 * Colour themes for the editor chrome and both canvases.
 *
 * The UI reads them as CSS custom properties (`:root[data-theme=…]` in style.css); the 2D and
 * 3D renderers read `theme()` and re-render when `onThemeChange` fires.
 */

export type ThemeName = "light" | "dark" | "solarized" | "nier";

export interface Theme {
  readonly name: ThemeName;
  readonly label: string;
  /** Whether the canvas background is dark (drives shading and fill toning). */
  readonly dark: boolean;
  /** Canvas background. */
  readonly paper: string;
  /** Panels and label backgrounds. */
  readonly panel: string;
  /** Default stroke and text colour. */
  readonly ink: string;
  /** Secondary text: zone kind tags, hints. */
  readonly muted: string;
  /** Hairlines and panel borders. */
  readonly line: string;
  /** Grid dots. */
  readonly grid: string;
  /** Selection, active tools, previews. */
  readonly accent: string;
  /** Zone borders. */
  readonly zoneStroke: string;
  /** Errors and destructive actions. */
  readonly danger: string;
}

export const THEMES: Readonly<Record<ThemeName, Theme>> = {
  light: {
    name: "light",
    label: "Light",
    dark: false,
    paper: "#f8f9fa",
    panel: "#ffffff",
    ink: "#1f2328",
    muted: "#6a737d",
    line: "#e1e4e8",
    grid: "#d0d5db",
    accent: "#4f5bd5",
    zoneStroke: "#6a737d",
    danger: "#cf222e",
  },
  dark: {
    name: "dark",
    label: "Dark",
    dark: true,
    paper: "#15171c",
    panel: "#1e2127",
    ink: "#e6e8eb",
    muted: "#8b949e",
    line: "#30353d",
    grid: "#2c3139",
    accent: "#8c95ff",
    zoneStroke: "#8b949e",
    danger: "#ff6b6b",
  },
  solarized: {
    name: "solarized",
    label: "Solarized",
    dark: true,
    paper: "#002b36",
    panel: "#073642",
    ink: "#eee8d5",
    muted: "#839496",
    line: "#1c4b56",
    grid: "#0f4450",
    accent: "#268bd2",
    zoneStroke: "#93a1a1",
    danger: "#dc322f",
  },
  nier: {
    name: "nier",
    label: "NieR",
    dark: false,
    paper: "#d1cdb7",
    panel: "#dad4bb",
    ink: "#4e4b42",
    muted: "#7d796b",
    line: "#b4ae96",
    grid: "#bdb8a1",
    accent: "#4e4b42",
    zoneStroke: "#6b675a",
    danger: "#cd664d",
  },
};

const STORAGE_KEY = "infraplot:theme";

export function themeNames(): ThemeName[] {
  return Object.keys(THEMES) as ThemeName[];
}

function isThemeName(v: unknown): v is ThemeName {
  return typeof v === "string" && v in THEMES;
}

function initial(): ThemeName {
  const saved = typeof localStorage === "undefined" ? null : localStorage.getItem(STORAGE_KEY);
  if (isThemeName(saved)) return saved;
  return typeof matchMedia !== "undefined" && matchMedia("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

let current: Theme = THEMES[initial()];
const listeners = new Set<(t: Theme) => void>();

export function theme(): Theme {
  return current;
}

/** Switches theme, updates `<html data-theme>` and notifies renderers. */
export function setTheme(name: ThemeName, persist = true): void {
  current = THEMES[name];
  document.documentElement.dataset.theme = name;
  if (persist) localStorage.setItem(STORAGE_KEY, name);
  for (const fn of listeners) fn(current);
}

export function onThemeChange(fn: (t: Theme) => void): () => void {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/** `a` mixed with `b`; `k` = 0 gives `a`, 1 gives `b`. Both are `#rgb` / `#rrggbb`. */
export function mix(a: string, b: string, k: number): string {
  const pa = rgb(a);
  const pb = rgb(b);
  const c = pa.map((v, i) => Math.round(v + ((pb[i] ?? v) - v) * k));
  return `#${c.map((v) => v.toString(16).padStart(2, "0")).join("")}`;
}

function rgb(hex: string): number[] {
  let h = hex.replace("#", "");
  if (h.length === 3) h = [...h].map((c) => c + c).join("");
  return [0, 2, 4].map((i) => Number.parseInt(h.slice(i, i + 2), 16));
}

/**
 * A fill colour as it should appear on the current theme's paper. Pastel defaults glow on
 * dark backgrounds, so there they are blended towards the paper; light themes keep them.
 */
export function surface(color: string, t: Theme = current): string {
  return t.dark ? mix(color, t.paper, 0.55) : color;
}
