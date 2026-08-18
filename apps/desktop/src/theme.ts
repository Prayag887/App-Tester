import { invoke } from "@tauri-apps/api/core";

export const THEMES = [
  { id: "default", label: "Default" },
  { id: "catppuccin-latte", label: "Catppuccin Latte · Light" },
  { id: "catppuccin-frappe", label: "Catppuccin Frappé" },
  { id: "catppuccin-macchiato", label: "Catppuccin Macchiato" },
  { id: "catppuccin-mocha", label: "Catppuccin Mocha" },
  { id: "dracula", label: "Dracula" },
  { id: "nord-dark", label: "Nord · Dark" },
  { id: "nord-light", label: "Nord · Light" },
  { id: "gruvbox-dark", label: "Gruvbox · Dark" },
  { id: "gruvbox-light", label: "Gruvbox · Light" },
  { id: "tokyo-night", label: "Tokyo Night" },
  { id: "tokyo-day", label: "Tokyo Day · Light" },
  { id: "rose-pine", label: "Rosé Pine" },
  { id: "rose-pine-dawn", label: "Rosé Pine Dawn · Light" },
  { id: "solarized-dark", label: "Solarized · Dark" },
  { id: "solarized-light", label: "Solarized · Light" },
  { id: "github-dark", label: "GitHub · Dark" },
  { id: "github-light", label: "GitHub · Light" },
  { id: "kanagawa", label: "Kanagawa" },
  { id: "everforest", label: "Everforest" },
] as const;

export type ThemeId = (typeof THEMES)[number]["id"];

const LIGHT_THEMES = new Set<ThemeId>([
  "catppuccin-latte",
  "nord-light",
  "gruvbox-light",
  "tokyo-day",
  "rose-pine-dawn",
  "solarized-light",
  "github-light",
]);

const THEME_STORAGE_KEY = "app-tester.color-theme";
const DEFAULT_THEME: ThemeId = "default";
const LEGACY_DEFAULT_THEME = "app-tester";
let themeStyles: Promise<unknown> | undefined;

function ensureThemeStyles(): void {
  themeStyles ??= import("./themes.css");
}

export function waitForThemeStyles(): Promise<unknown> {
  return themeStyles ?? Promise.resolve();
}

export function themeMode(theme: ThemeId): "light" | "dark" {
  return LIGHT_THEMES.has(theme) ? "light" : "dark";
}

function syncNativeTheme(theme: ThemeId): void {
  if (typeof window === "undefined" || !("__TAURI_INTERNALS__" in window)) return;
  void import("@tauri-apps/api/window")
    .then(({ getCurrentWindow }) => Promise.all([
      getCurrentWindow().setTheme(themeMode(theme)),
      invoke("remember_startup_theme", { theme }),
    ]))
    .catch(() => undefined);
}

export function isThemeId(value: string | null): value is ThemeId {
  return THEMES.some((theme) => theme.id === value);
}

export function readTheme(): ThemeId {
  if (typeof window === "undefined") return DEFAULT_THEME;
  const saved = window.localStorage.getItem(THEME_STORAGE_KEY);
  if (saved === LEGACY_DEFAULT_THEME) return DEFAULT_THEME;
  return isThemeId(saved) ? saved : DEFAULT_THEME;
}

export function applyTheme(theme: ThemeId): void {
  ensureThemeStyles();
  document.documentElement.dataset.theme = theme;
  window.localStorage.setItem(THEME_STORAGE_KEY, theme);
  syncNativeTheme(theme);
}

export function initializeTheme(): ThemeId {
  const theme = readTheme();
  ensureThemeStyles();
  document.documentElement.dataset.theme = theme;
  syncNativeTheme(theme);
  return theme;
}
