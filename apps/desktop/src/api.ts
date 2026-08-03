import { invoke } from "@tauri-apps/api/core";
import type {
  CleanupResult,
  Collection,
  Environment,
  RetentionPolicy,
  Run,
  AndroidDevice,
  AndroidApp,
  HttpTransaction,
  LogLine,
  Diagnostic,
} from "./types";

export const isTauri = () => "__TAURI_INTERNALS__" in window;

export async function listCollections(): Promise<Collection[]> {
  if (!isTauri()) return [];
  return invoke("list_collections");
}

export const discoverAndroidDevices = (): Promise<AndroidDevice[]> =>
  isTauri() ? invoke("discover_android_devices") : Promise.resolve([]);
export const listDebuggableApps = (serial: string): Promise<AndroidApp[]> =>
  invoke("list_debuggable_apps", { serial });
export const generateCaptureCa = (): Promise<{
  path: string;
  fingerprint_sha256: string;
}> => invoke("generate_capture_ca");
export const prepareAndroidCa = (serial: string): Promise<void> =>
  invoke("prepare_android_ca", { serial });
export const enableUsbWifi = (serial: string): Promise<string> =>
  invoke("enable_usb_wifi", { serial });
export const startCapture = (
  serial: string,
  connectionType: string,
  packageName: string,
): Promise<string> =>
  invoke("start_capture", { serial, connectionType, packageName });
export const stopCapture = (): Promise<void> => invoke("stop_capture");
export const captureStatus = (): Promise<string> =>
  isTauri() ? invoke("capture_status") : Promise.resolve("stopped");
export const captureActive = (): Promise<boolean> =>
  isTauri() ? invoke("capture_active") : Promise.resolve(false);
export const captureTransactions = (): Promise<HttpTransaction[]> =>
  isTauri() ? invoke("capture_transactions") : Promise.resolve([]);
export const captureLogs = (): Promise<LogLine[]> =>
  isTauri() ? invoke("capture_logs") : Promise.resolve([]);
export const captureDiagnostics = (): Promise<Diagnostic[]> =>
  isTauri() ? invoke("capture_diagnostics") : Promise.resolve([]);

export async function importCollection(source: string): Promise<Collection> {
  if (!isTauri()) throw new Error("Import is available in the desktop app");
  return invoke("import_collection", { source });
}

export async function saveCollection(collection: Collection): Promise<void> {
  return invoke("save_collection", { collection });
}

export async function runCollection(
  collectionId: string,
  baselineRunId?: string,
  environmentId?: string,
): Promise<Run> {
  return invoke("run_collection", {
    collectionId,
    baselineRunId,
    environmentId,
  });
}

export async function runRequest(
  collectionId: string,
  requestId: string,
  baselineRunId?: string,
  environmentId?: string,
): Promise<Run> {
  return invoke("run_request", {
    collectionId,
    requestId,
    baselineRunId,
    environmentId,
  });
}

export async function listRuns(collectionId?: string): Promise<Run[]> {
  if (!isTauri()) return [];
  return invoke("list_runs", { collectionId });
}

export async function getRun(id: string): Promise<Run> {
  return invoke("get_run", { id });
}

export async function listEnvironments(): Promise<Environment[]> {
  if (!isTauri()) return [];
  return invoke("list_environments");
}

export async function importEnvironment(source: string): Promise<Environment> {
  if (!isTauri())
    throw new Error("Environment import is available in the desktop app");
  return invoke("import_environment", { source });
}

export async function saveEnvironment(environment: Environment): Promise<void> {
  return invoke("save_environment", { environment });
}

export async function setRunPinned(id: string, pinned: boolean): Promise<void> {
  return invoke("set_run_pinned", { id, pinned });
}

export async function retentionPolicy(): Promise<RetentionPolicy> {
  return invoke("retention_policy");
}

export async function saveRetentionPolicy(
  policy: RetentionPolicy,
): Promise<void> {
  return invoke("save_retention_policy", { policy });
}

export async function cleanupHistory(): Promise<CleanupResult> {
  return invoke("cleanup_history");
}

export async function exportWorkspace(): Promise<string> {
  return invoke("export_workspace_bundle");
}

export async function exportWorkspaceFile(): Promise<string> {
  return invoke("export_workspace_file");
}

export async function importWorkspace(source: string): Promise<Collection[]> {
  return invoke("import_workspace_bundle", { source });
}
