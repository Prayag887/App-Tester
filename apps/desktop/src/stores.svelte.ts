//! Shared reactive state for the desktop shell. One module owns every piece
//! of mutable UI state and every native interaction, so components stay dumb
//! and the polling/listener wiring lives in exactly one place.
//!
//! Module-level `$state` bindings cannot be both reassigned and exported, so
//! the mutable state lives in a single `ui` object; components read `ui.*`
//! (property access is reactive) and mutate it through the action functions
//! below. Derived values are exported read-only.

import * as api from "./api";
import {
  copyToClipboard,
  curlCommand,
  endpointId,
  timeLabel,
  transactionState,
  type Screen,
  type Tab,
} from "./lib";
import type {
  AndroidApp,
  AndroidDevice,
  HttpTransaction,
  LogIncident,
  ManualRequest,
  ProxyStatus,
} from "./types";

class UiState {
  screen = $state<Screen>("traffic");
  tab = $state<Tab>("Request");
  // A request handed to the composer from another screen (e.g. "Send in
  // Composer" on a captured transaction); consumed on composer mount.
  composerDraft = $state<ManualRequest | null>(null);
  proxy = $state<ProxyStatus>("stopped");
  devices = $state.raw<AndroidDevice[]>([]);
  device = $state("");
  apps = $state.raw<AndroidApp[]>([]);
  packageName = $state("");
  packageSearch = $state("");
  packagePickerOpen = $state(false);
  devicesOpen = $state(false);
  // Native payloads are immutable snapshots. Raw state avoids recursively
  // proxying hundreds of headers, differences, and body-preview bytes.
  transactions = $state.raw<HttpTransaction[]>([]);
  transactionDetail = $state.raw<HttpTransaction | null>(null);
  detailLoading = $state(false);
  selectedId = $state("");
  incidents = $state.raw<LogIncident[]>([]);
  query = $state("");
  changedOnly = $state(false);
  errorsOnly = $state(false);
  capturing = $state(false);
  busy = $state(false);
  notice = $state("");
  desktopHost = $state("ADB reverse · USB");
  activeSessionId = $state<string | undefined>(undefined);
  expandedIssue = $state("");
  mirrorOpen = $state(false);
  mirrorData = $state("");
  mirrorError = $state("");
  confirmDeleteAll = $state(false);
}

export const ui = new UiState();

let transactionRefreshInFlight = false;
let detailRequest = 0;
export const UI_TRANSACTION_LIMIT = 250;

export function getCapturedTransactions() {
  return ui.transactions.filter(
    (tx) => tx.request.method.toUpperCase() !== "CONNECT",
  );
}

export function getVisibleTransactions() {
  const captured = getCapturedTransactions();
  return captured.filter((tx) => {
    const searchable =
      `${tx.request.method} ${tx.request.host} ${tx.request.path} ${tx.response?.status ?? ""}`.toLowerCase();
    return (
      searchable.includes(ui.query.toLowerCase()) &&
      (!ui.changedOnly || transactionState(tx) === "Changed") &&
      (!ui.errorsOnly ||
        transactionState(tx) === "Failed" ||
        tx.correlated_incidents.length > 0)
    );
  });
}

export function getSelectedTransaction() {
  const summary =
    getCapturedTransactions().find((tx) => tx.id === ui.selectedId) ??
    getVisibleTransactions()[0];
  return ui.transactionDetail?.id === summary?.id
    ? ui.transactionDetail
    : summary;
}

export async function selectTransaction(id: string) {
  ui.selectedId = id;
  ui.transactionDetail = null;
  ui.detailLoading = true;
  const request = ++detailRequest;
  try {
    const detail = await api.getTransaction(id);
    if (request === detailRequest && ui.selectedId === id) {
      ui.transactionDetail = detail;
    }
  } catch {
    // The metadata row remains usable if the detail read fails.
  } finally {
    if (request === detailRequest) ui.detailLoading = false;
  }
}

/// Memoized per-row classification so the request list never recomputes
/// `transactionState` for every row on every keystroke.
export function getRowStates() {
  return new Map(
    getVisibleTransactions().map((tx) => [tx.id, transactionState(tx)]),
  );
}

export function getSelectedDevice() {
  return ui.devices.find((item) => item.serial === ui.device);
}

export function getMatchingApps() {
  const search = ui.packageSearch.trim().toLowerCase();
  return ui.apps
    .filter(
      (app) =>
        !search ||
        `${app.package_name} ${app.version_name ?? ""}`
          .toLowerCase()
          .includes(search),
    )
    .slice(0, search ? 50 : 8);
}

export function getChangedCount() {
  return getCapturedTransactions().filter(
    (tx) => transactionState(tx) === "Changed",
  ).length;
}

export function getFailedCount() {
  return getCapturedTransactions().filter(
    (tx) => transactionState(tx) === "Failed",
  ).length;
}

export function getErrorCount() {
  return ui.incidents.filter((item) =>
    ["crash", "error", "anr"].includes(item.category),
  ).length;
}

export function getStatusLabel() {
  return ui.capturing
    ? "Capturing live"
    : ui.proxy === "running"
      ? "Proxy ready"
      : "Ready to capture";
}

export const rowTime = (tx: HttpTransaction) => timeLabel(tx.created_at);

export function closePickers() {
  ui.packagePickerOpen = false;
  ui.devicesOpen = false;
  ui.packageSearch = "";
}

export function chooseDevice(serial: string) {
  ui.device = serial;
  closePickers();
  void loadApps();
}

export function choosePackage(name: string) {
  ui.packageName = name;
  ui.packageSearch = "";
  ui.packagePickerOpen = false;
  ui.notice = `Selected ${name}`;
}

export function copy(value: string) {
  copyToClipboard(value, () => (ui.notice = "Copied to clipboard"));
}

/** Copies only the selected request's generated cURL command. */
export function copySelectedCurl() {
  const command = curlCommand(getSelectedTransaction());
  if (!command) {
    ui.notice = "No cURL is available for this request";
    return;
  }
  copy(command);
}

export function upsertTransaction(transaction: HttpTransaction) {
  // The native proxy is the authority for the active capture. The WebView's
  // cached session can lag after a WebView restore; dropping its event
  // leaves a database row invisible until some unrelated UI refresh.
  ui.transactions = [
    transaction,
    ...ui.transactions.filter((item) => item.id !== transaction.id),
  ].slice(0, UI_TRANSACTION_LIMIT);
}

export function upsertIncident(issue: LogIncident) {
  const existing = ui.incidents.find(
    (item) => item.signature === issue.signature,
  );
  const occurrence_count = Math.max(
    issue.occurrence_count,
    existing ? existing.occurrence_count + 1 : 1,
  );
  ui.incidents = [
    { ...issue, occurrence_count },
    ...ui.incidents.filter((item) => item.signature !== issue.signature),
  ].slice(0, 100);
}

export function reconcileTransactions(fresh: HttpTransaction[]) {
  // Event delivery gives us the lowest-latency update, while the database
  // read repairs anything delivered while the WebView was unavailable. Do
  // not let an empty/stale read erase a transaction that has just arrived.
  const byId = new Map(ui.transactions.map((item) => [item.id, item]));
  fresh.forEach((item) => byId.set(item.id, item));
  ui.transactions = [...byId.values()]
    .sort((left, right) => right.created_at.localeCompare(left.created_at))
    .slice(0, UI_TRANSACTION_LIMIT);
}

export async function refreshTransactions() {
  if (transactionRefreshInFlight) return;
  transactionRefreshInFlight = true;
  try {
    reconcileTransactions(await api.listTransactions(ui.activeSessionId));
  } catch {
    // Live events remain the primary update path.
  } finally {
    transactionRefreshInFlight = false;
  }
}

export async function refreshProxyStatus() {
  try {
    const next = await api.getProxyStatus();
    if (ui.capturing && ui.proxy === "running" && next !== "running") {
      ui.capturing = false;
      ui.notice =
        "USB capture stopped. Keep the device connected and start a fresh capture.";
    }
    ui.proxy = next;
  } catch {
    // Keep the last known state while the native bridge is unavailable.
  }
}

export async function refreshDevices() {
  try {
    ui.devices = await api.discoverDevices();
    const nextDevice = ui.devices.some(
      (item) =>
        item.serial === ui.device && item.authorization_status === "authorized",
    )
      ? ui.device
      : (ui.devices.find((item) => item.authorization_status === "authorized")
          ?.serial ?? "");
    if (nextDevice !== ui.device) {
      ui.device = nextDevice;
      // A USB device is normally selected automatically, so load its
      // apps here rather than waiting for a second click.
      void loadApps();
    }
  } catch (error) {
    ui.notice = `Could not refresh Android devices: ${String(error)}`;
  }
}

export async function loadApps() {
  if (!ui.device) {
    ui.apps = [];
    ui.packageName = "";
    return;
  }
  try {
    ui.apps = await api.listInstalledApps(ui.device);
    // Package discovery can briefly return an incomplete list while ADB is
    // busy. Never clear the target from a live capture because the
    // capture session remains scoped to that package.
    if (
      ui.packageName &&
      !ui.capturing &&
      !ui.apps.some((item) => item.package_name === ui.packageName)
    ) {
      ui.packageName = "";
    }
  } catch (error) {
    ui.notice = `Could not load packages: ${String(error)}`;
  }
}

export async function start() {
  await connectCompanion();
}

export async function stop() {
  ui.busy = true;
  try {
    await api.stopProxy();
    ui.capturing = false;
    ui.notice = "Capture stopped.";
  } catch (error) {
    ui.notice = `Could not stop capture: ${String(error)}`;
  } finally {
    ui.busy = false;
  }
}

export async function connectCompanion() {
  ui.busy = true;
  try {
    const current = getSelectedDevice();
    if (!current)
      throw new Error(
        "Connect and authorize an Android device over USB first.",
      );
    if (!ui.packageName)
      throw new Error(
        "Choose the target package before opening the companion.",
      );
    const connection = await api.openUsbCompanion(ui.device, ui.packageName);
    ui.activeSessionId = connection.session_id;
    ui.capturing = true;
    ui.transactions = [];
    ui.transactionDetail = null;
    ui.incidents = [];
    ui.selectedId = "";
    await refreshTransactions();
    await api
      .startLogcatCapture(ui.device, ui.packageName)
      .catch(() => undefined);
    ui.notice = `USB capture active for ${ui.packageName}. Keep the cable connected and use the app normally.`;
  } catch (error) {
    ui.notice = `Could not open companion: ${String(error)}`;
  } finally {
    ui.busy = false;
  }
}

export async function exportCapture() {
  try {
    ui.notice = `Exported capture to ${await api.exportCaptureToFile()}`;
  } catch (error) {
    ui.notice = String(error);
  }
}

/// Two-step delete guard. Tauri's WebView does not implement
/// `window.confirm` (it silently returns false), so deletion is gated on a
/// second click within the confirmation window instead.
const DELETE_CONFIRM_TIMEOUT_MS = 4000;
let deleteConfirmTimer: number | undefined;

export function requestDeleteAll() {
  if (!ui.confirmDeleteAll) {
    ui.confirmDeleteAll = true;
    if (typeof window !== "undefined") {
      window.clearTimeout(deleteConfirmTimer);
      deleteConfirmTimer = window.setTimeout(() => {
        ui.confirmDeleteAll = false;
      }, DELETE_CONFIRM_TIMEOUT_MS);
    }
    return;
  }
  if (typeof window !== "undefined") {
    window.clearTimeout(deleteConfirmTimer);
  }
  ui.confirmDeleteAll = false;
  void performDeleteAll();
}

async function performDeleteAll() {
  ui.busy = true;
  try {
    await api.deleteAllTransactions();
    ui.transactions = [];
    ui.transactionDetail = null;
    ui.incidents = [];
    ui.selectedId = "";
    ui.notice = "Deleted all captured traffic and diagnostics.";
  } catch (error) {
    ui.notice = `Could not delete captures: ${String(error)}`;
  } finally {
    ui.busy = false;
  }
}

export async function importCapture(event: Event) {
  const file = (event.target as HTMLInputElement).files?.[0];
  if (!file) return;
  try {
    await api.importCapture(await file.text());
    ui.transactions = await api.listTransactions();
    ui.transactionDetail = null;
    ui.notice = "Imported redacted capture.";
  } catch (error) {
    ui.notice = String(error);
  }
  (event.target as HTMLInputElement).value = "";
}

export function setMirrorOpen(next: boolean) {
  ui.mirrorOpen = next;
  if (!next) {
    ui.mirrorData = "";
    ui.mirrorError = "";
  }
}

export async function captureScreen() {
  if (!ui.device) {
    ui.mirrorError = "Select an Android device to mirror.";
    return;
  }
  try {
    ui.mirrorData = await api.captureScreen(ui.device);
    ui.mirrorError = "";
  } catch (error) {
    ui.mirrorError = String(error);
  }
}

export async function approveBaseline(tx: HttpTransaction) {
  const id = endpointId(tx);
  if (!id) {
    ui.notice =
      "This response does not have a comparable endpoint identity yet.";
    return;
  }
  try {
    await api.approveBaseline(id, tx.id);
    ui.notice = "Response saved as the JSON-key baseline for this endpoint.";
  } catch (error) {
    ui.notice = `Could not save baseline: ${String(error)}`;
  }
}
