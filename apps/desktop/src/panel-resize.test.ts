// @vitest-environment jsdom
import { beforeEach, describe, expect, it } from "vitest";
import { clampPanelSize, readPanelSize, storePanelSize } from "./panel-resize";

beforeEach(() => window.localStorage.clear());

describe("panel preferences", () => {
  it("persists a resized panel and restores it on the next view", () => {
    storePanelSize("panel", 612.4);

    expect(readPanelSize("panel", 400)).toBe(612);
  });

  it("falls back for invalid values and clamps resize limits", () => {
    window.localStorage.setItem("panel", "not-a-number");

    expect(readPanelSize("panel", 400)).toBe(400);
    expect(clampPanelSize(100, 200, 500)).toBe(200);
    expect(clampPanelSize(700, 200, 500)).toBe(500);
  });
});
