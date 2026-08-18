import { afterEach, describe, expect, it, vi } from "vitest";
import { POPOVER_DURATION_MS, popover } from "./motion";

describe("popover motion", () => {
  afterEach(() => vi.unstubAllGlobals());

  it("settles at full opacity and scale", () => {
    const transition = popover(document.createElement("div"));
    expect(transition.duration).toBe(POPOVER_DURATION_MS);
    expect(transition.css(0)).toContain("translate3d(0,-8px,0) scale(0.95)");
    expect(transition.css(1)).toContain("translate3d(0,0px,0) scale(1)");
  });

  it("finishes immediately when reduced motion is requested", () => {
    vi.stubGlobal("matchMedia", () => ({ matches: true }));
    expect(popover(document.createElement("div")).duration).toBe(0);
  });
});
