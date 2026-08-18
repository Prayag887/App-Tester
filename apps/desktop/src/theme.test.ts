// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from "vitest";
import { applyTheme, initializeTheme, readTheme, themeMode, THEMES } from "./theme";

beforeEach(() => {
  localStorage.clear();
  delete document.documentElement.dataset.theme;
});

describe("color themes", () => {
  it("uses the Default palette when no preference is stored", () => {
    expect(initializeTheme()).toBe("default");
    expect(document.documentElement.dataset.theme).toBe("default");
  });

  it("applies and persists a selected palette", () => {
    applyTheme("dracula");

    expect(readTheme()).toBe("dracula");
    expect(document.documentElement.dataset.theme).toBe("dracula");
  });

  it("ignores an obsolete or invalid stored palette", () => {
    localStorage.setItem("app-tester.color-theme", "unknown");
    expect(readTheme()).toBe("default");
  });

  it("migrates the previous App Tester preference to Default", () => {
    localStorage.setItem("app-tester.color-theme", "app-tester");
    expect(readTheme()).toBe("default");
  });

  it("offers unique light and dark palettes", () => {
    expect(new Set(THEMES.map((theme) => theme.id)).size).toBe(THEMES.length);
    expect(THEMES[0]).toEqual({ id: "default", label: "Default" });
    expect(THEMES.map((theme) => theme.id)).toEqual(
      expect.arrayContaining([
        "catppuccin-latte",
        "catppuccin-mocha",
        "nord-light",
        "nord-dark",
        "gruvbox-light",
        "gruvbox-dark",
        "solarized-light",
        "solarized-dark",
      ]),
    );
    expect(themeMode("catppuccin-latte")).toBe("light");
    expect(themeMode("github-light")).toBe("light");
    expect(themeMode("dracula")).toBe("dark");
    expect(themeMode("default")).toBe("dark");
  });
});
