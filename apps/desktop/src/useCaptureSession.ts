import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useState } from "react";
import {
  captureActive,
  captureDiagnostics,
  captureLogs,
  captureStatus,
  captureTransactions,
  discoverAndroidDevices,
  enableUsbWifi,
  generateCaptureCa,
  isTauri,
  listDebuggableApps,
  prepareAndroidCa,
  startCapture,
  stopCapture,
} from "./api";
import type {
  AndroidApp,
  AndroidDevice,
  Diagnostic,
  HttpTransaction,
  LogLine,
} from "./types";

const TRANSACTION_LIMIT = 250;
// The UI only renders the latest log entry. Keep a small local window while the
// core service remains the authoritative 2,000-line diagnostic buffer.
const LOG_LIMIT = 80;
const DIAGNOSTIC_LIMIT = 100;

interface TauriEvent<T> {
  kind: string;
  payload: T;
}

function appendBounded<T>(items: T[], item: T, limit: number): T[] {
  return [...items, item].slice(-limit);
}

interface CaptureSession {
  active: boolean;
  apps: AndroidApp[];
  busy: boolean;
  device?: AndroidDevice;
  devices: AndroidDevice[];
  diagnostics: Diagnostic[];
  logs: LogLine[];
  selectedApp?: string;
  status: string;
  transactions: HttpTransaction[];
  refresh: () => Promise<void>;
  selectApp: (packageName?: string) => void;
  selectDevice: (serial: string) => Promise<void>;
  installCertificate: () => Promise<void>;
  switchUsbToWifi: () => Promise<void>;
  start: () => Promise<void>;
  stop: () => Promise<void>;
}

export function useCaptureSession(
  onError: (error: string) => void,
): CaptureSession {
  const [devices, setDevices] = useState<AndroidDevice[]>([]);
  const [device, setDevice] = useState<AndroidDevice>();
  const [apps, setApps] = useState<AndroidApp[]>([]);
  const [selectedApp, setSelectedApp] = useState<string>();
  const [status, setStatus] = useState("stopped");
  const [active, setActive] = useState(false);
  const [transactions, setTransactions] = useState<HttpTransaction[]>([]);
  const [logs, setLogs] = useState<LogLine[]>([]);
  const [diagnostics, setDiagnostics] = useState<Diagnostic[]>([]);
  const [busy, setBusy] = useState(false);

  const refresh = useCallback(async () => {
    const [nextDevices, nextStatus, isActive] = await Promise.all([
      discoverAndroidDevices(),
      captureStatus(),
      captureActive(),
    ]);
    setDevices(nextDevices);
    setStatus(nextStatus);
    setActive(isActive);
    setDevice((current) =>
      current && nextDevices.some((item) => item.serial === current.serial)
        ? current
        : undefined,
    );

    if (!isActive) return;
    const [nextTransactions, nextLogs, nextDiagnostics] = await Promise.all([
      captureTransactions(),
      captureLogs(),
      captureDiagnostics(),
    ]);
    setTransactions(nextTransactions);
    setLogs(nextLogs.slice(-LOG_LIMIT));
    setDiagnostics(nextDiagnostics);
  }, []);

  useEffect(() => {
    void refresh().catch((error) => onError(String(error)));
  }, [onError, refresh]);

  useEffect(() => {
    if (!isTauri()) return;
    let disposed = false;
    let unlisten = () => {};
    void Promise.all([
      listen<TauriEvent<string>>("capture-status", (event) => {
        setStatus(event.payload.payload);
        setActive(event.payload.payload === "running");
      }),
      listen<TauriEvent<HttpTransaction>>("capture-transaction", (event) => {
        const transaction = event.payload.payload;
        setTransactions((current) => {
          const withoutCurrent = current.filter(
            (item) => item.id !== transaction.id,
          );
          return appendBounded(withoutCurrent, transaction, TRANSACTION_LIMIT);
        });
      }),
      listen<TauriEvent<LogLine>>("capture-log-line", (event) => {
        setLogs((current) =>
          appendBounded(current, event.payload.payload, LOG_LIMIT),
        );
      }),
      listen<TauriEvent<Diagnostic>>("capture-diagnostic", (event) => {
        const diagnostic = event.payload.payload;
        setDiagnostics((current) => {
          const withoutCurrent = current.filter(
            (item) => item.signature !== diagnostic.signature,
          );
          return appendBounded(withoutCurrent, diagnostic, DIAGNOSTIC_LIMIT);
        });
      }),
    ])
      .then((listeners) => {
        if (disposed) {
          listeners.forEach((stop) => stop());
          return;
        }
        unlisten = () => listeners.forEach((stop) => stop());
      })
      .catch((error) => onError(String(error)));
    return () => {
      disposed = true;
      unlisten();
    };
  }, [onError]);

  const selectDevice = useCallback(
    async (serial: string) => {
      const nextDevice = devices.find((item) => item.serial === serial);
      setDevice(nextDevice);
      setSelectedApp(undefined);
      setApps(
        nextDevice?.authorization_status === "authorized"
          ? await listDebuggableApps(serial)
          : [],
      );
    },
    [devices],
  );

  const installCertificate = useCallback(async () => {
    if (!device) return;
    setBusy(true);
    try {
      await generateCaptureCa();
      await prepareAndroidCa(device.serial);
    } catch (error) {
      onError(String(error));
    } finally {
      setBusy(false);
    }
  }, [device, onError]);

  const switchUsbToWifi = useCallback(async () => {
    if (!device || device.connection_type !== "usb") return;
    setBusy(true);
    try {
      const endpoint = await enableUsbWifi(device.serial);
      await refresh();
      setDevice((current) =>
        current?.serial === device.serial
          ? { ...device, serial: endpoint, connection_type: "wireless" }
          : current,
      );
      setApps(await listDebuggableApps(endpoint));
    } catch (error) {
      onError(String(error));
    } finally {
      setBusy(false);
    }
  }, [device, onError, refresh]);

  const start = useCallback(async () => {
    if (!device || !selectedApp) return;
    setBusy(true);
    try {
      await generateCaptureCa();
      setStatus(
        await startCapture(device.serial, device.connection_type, selectedApp),
      );
      setActive(true);
      await refresh();
    } catch (error) {
      onError(String(error));
      await refresh();
    } finally {
      setBusy(false);
    }
  }, [device, onError, refresh, selectedApp]);

  const stop = useCallback(async () => {
    setBusy(true);
    try {
      await stopCapture();
    } catch (error) {
      onError(String(error));
    } finally {
      await refresh().catch((error) => onError(String(error)));
      setBusy(false);
    }
  }, [onError, refresh]);

  return {
    active,
    apps,
    busy,
    device,
    devices,
    diagnostics,
    logs,
    selectedApp,
    status,
    transactions,
    refresh,
    selectApp: setSelectedApp,
    selectDevice,
    installCertificate,
    switchUsbToWifi,
    start,
    stop,
  };
}
