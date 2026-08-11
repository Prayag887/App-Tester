import { beforeEach, describe, expect, it, vi } from "vitest";

const updaterApi = vi.hoisted(() => ({
  check: vi.fn(),
  relaunch: vi.fn(),
}));

vi.mock("@tauri-apps/plugin-updater", () => ({ check: updaterApi.check }));
vi.mock("@tauri-apps/plugin-process", () => ({ relaunch: updaterApi.relaunch }));

import {
  checkForUpdates,
  dismissUpdate,
  installUpdate,
  updater,
} from "./updates.svelte";

describe("desktop updater", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    Object.assign(window, { __TAURI_INTERNALS__: {} });
    dismissUpdate();
  });

  it("reports when a manual check finds no update", async () => {
    updaterApi.check.mockResolvedValue(null);

    await checkForUpdates(true);

    expect(updater.status).toBe("up-to-date");
    expect(updater.message).toBe("App Tester is up to date.");
  });

  it("offers an available signed release", async () => {
    updaterApi.check.mockResolvedValue({
      version: "0.3.0",
      body: "QA timeline improvements",
      close: vi.fn(),
    });

    await checkForUpdates();

    expect(updater.status).toBe("available");
    expect(updater.version).toBe("0.3.0");
    expect(updater.notes).toBe("QA timeline improvements");
  });

  it("installs, reports download progress, and relaunches", async () => {
    const downloadAndInstall = vi.fn(async (onDownload) => {
      onDownload({ event: "Started", data: { contentLength: 100 } });
      onDownload({ event: "Progress", data: { chunkLength: 60 } });
      onDownload({ event: "Progress", data: { chunkLength: 40 } });
      onDownload({ event: "Finished" });
    });
    updaterApi.check.mockResolvedValue({
      version: "0.3.0",
      body: "",
      close: vi.fn(),
      downloadAndInstall,
    });
    updaterApi.relaunch.mockResolvedValue(undefined);
    await checkForUpdates();

    await installUpdate();

    expect(downloadAndInstall).toHaveBeenCalledOnce();
    expect(updater.progress).toBe(100);
    expect(updaterApi.relaunch).toHaveBeenCalledOnce();
  });
});
