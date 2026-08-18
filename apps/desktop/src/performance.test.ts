import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const readDesktopCss = (name: string) =>
  readFileSync(resolve(process.cwd(), "src", name), "utf8");
const motion = readDesktopCss("motion.css");
const stylesheet = readDesktopCss("styles.css");

const desktopCss = `${stylesheet}\n${motion}`;

describe("desktop performance budgets", () => {
  it("keeps the startup stylesheet compact", () => {
    expect(new TextEncoder().encode(desktopCss).byteLength).toBeLessThan(
      68_000,
    );
  });

  it("does not add persistent backdrop blur layers", () => {
    expect(desktopCss).not.toContain("backdrop-filter");
  });

  it("keeps spatial and effect motion centralized", () => {
    expect(motion).toContain("--motion-spatial-fast");
    expect(motion).toContain("--motion-effect-fast");
    expect(motion).toContain("prefers-reduced-motion");
    expect(motion).toContain("motion-startup-veil");
    expect(stylesheet).toContain("--shell-accent");
    expect(stylesheet).toContain(".workbench.inspector-open");
  });
});
