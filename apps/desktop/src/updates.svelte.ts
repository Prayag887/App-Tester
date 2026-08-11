import { relaunch } from "@tauri-apps/plugin-process";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";

type UpdateStatus =
  | "idle"
  | "checking"
  | "available"
  | "downloading"
  | "up-to-date"
  | "error";

export const updater = $state({
  status: "idle" as UpdateStatus,
  version: "",
  notes: "",
  progress: 0,
  message: "",
  candidate: undefined as Update | undefined,
});

const isNative = () =>
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

let statusTimer: number | undefined;

const clearStatusTimer = () => {
  if (typeof window !== "undefined") window.clearTimeout(statusTimer);
  statusTimer = undefined;
};

const showTemporaryStatus = (status: "up-to-date" | "error", message: string) => {
  clearStatusTimer();
  updater.status = status;
  updater.message = message;
  if (typeof window !== "undefined") {
    statusTimer = window.setTimeout(() => {
      if (updater.status === status) updater.status = "idle";
    }, 8000);
  }
};

export async function checkForUpdates(manual = false) {
  if (!isNative() || updater.status === "checking" || updater.status === "downloading") return;
  clearStatusTimer();
  updater.status = "checking";
  updater.message = "";
  try {
    const candidate = await check({ timeout: 20_000 });
    if (!candidate) {
      if (manual) showTemporaryStatus("up-to-date", "App Tester is up to date.");
      else updater.status = "idle";
      return;
    }
    updater.candidate?.close();
    updater.candidate = candidate;
    updater.version = candidate.version;
    updater.notes = candidate.body ?? "";
    updater.progress = 0;
    updater.status = "available";
  } catch (error) {
    if (manual) showTemporaryStatus("error", `Could not check for updates: ${String(error)}`);
    else updater.status = "idle";
  }
}

export async function installUpdate() {
  const candidate = updater.candidate;
  if (!candidate || updater.status === "downloading") return;
  clearStatusTimer();
  updater.status = "downloading";
  updater.progress = 0;
  updater.message = "Downloading update…";
  let downloaded = 0;
  let total = 0;
  const onDownload = (event: DownloadEvent) => {
    if (event.event === "Started") total = event.data.contentLength ?? 0;
    if (event.event === "Progress") downloaded += event.data.chunkLength;
    updater.progress = total > 0 ? Math.min(100, Math.round((downloaded / total) * 100)) : 0;
  };
  try {
    await candidate.downloadAndInstall(onDownload);
    updater.message = "Update installed. Restarting App Tester…";
    await relaunch();
  } catch (error) {
    updater.status = "available";
    updater.message = `Update failed: ${String(error)}`;
  }
}

export function dismissUpdate() {
  clearStatusTimer();
  updater.candidate?.close();
  updater.candidate = undefined;
  updater.status = "idle";
  updater.message = "";
  updater.progress = 0;
}
