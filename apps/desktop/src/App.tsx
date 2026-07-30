import { useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { Activity, AlertCircle, CalendarDays, Circle, Copy, Download, Filter, ListTree, Pause, Play, Search, Settings, ShieldCheck, SlidersHorizontal, Square, TerminalSquare, Trash2, Upload, Wifi, X } from "lucide-react";
import * as api from "./api";
import type { AndroidApp, AndroidCaStatus, AndroidCertificateInstall, AndroidDevice, BodyStorage, ComparisonRules, CompanionConnection, CompanionInstall, HttpTransaction, LogIncident, ProxyStatus } from "./types";

type InspectorTab = "Overview" | "Request" | "Response" | "Compare" | "cURL" | "Logs" | "Timeline";
type Screen = "toolkit" | "logs";
export const duration = (tx: HttpTransaction) => tx.timing.response_complete_ms == null ? undefined :
  tx.timing.response_complete_ms - tx.timing.request_started_ms;
export const bodyText = (body?: BodyStorage) => {
  if (!body || body.storage === "empty") return "";
  if (body.storage === "unavailable") return body.reason;
  const bytes = body.storage === "inline" ? body.bytes : body.preview;
  return new TextDecoder().decode(new Uint8Array(bytes));
};
export const displayState = (tx: HttpTransaction) => {
  if (!tx.response) return "Pending";
  if (tx.response.status >= 400) return "Failed";
  if (!tx.comparison?.baseline_transaction_id) return "New";
  if (tx.comparison?.differences.some((difference) => !difference.ignored)) return "Changed";
  return "Unchanged";
};
export const fullEndpoint = (tx: HttpTransaction) =>
  `${tx.request.scheme}://${tx.request.host}${tx.request.path}`;
export const baselineKey = (tx: HttpTransaction) => tx.endpoint_identity
  ? `${tx.endpoint_identity.method} ${tx.endpoint_identity.host} ${tx.endpoint_identity.path_template}`
  : undefined;
export const compactEndpoint = (endpoint: string) => {
  const withoutProtocol = endpoint.replace(/^https?:\/\//i, "").replace(/^www\./i, "");
  const slash = withoutProtocol.indexOf("/");
  const host = slash < 0 ? withoutProtocol : withoutProtocol.slice(0, slash);
  const rest = slash < 0 ? "" : withoutProtocol.slice(slash);
  const compactHost = host.replace(/\.(com|org|net|io|co|dev|app)(?=:\d+$|$)/i, "");
  return `${compactHost}${rest}` || endpoint;
};
export const endpointIsExcluded = (tx: HttpTransaction, exclusions: string[]) => {
  const endpoint = fullEndpoint(tx).trim().toLowerCase();
  const host = tx.request.host.trim().toLowerCase();
  return exclusions.some(value => {
    const exclusion = value.trim().toLowerCase().replace(/\/$/, "");
    if (!exclusion) return false;
    if (exclusion.includes("://")) {
      return endpoint === exclusion || endpoint.startsWith(`${exclusion}/`) ||
        endpoint.startsWith(`${exclusion}?`);
    }
    const [excludedHost, ...pathParts] = exclusion.split("/");
    const hostMatches = host === excludedHost || host.endsWith(`.${excludedHost}`);
    if (!hostMatches) return false;
    const excludedPath = pathParts.length ? `/${pathParts.join("/")}` : "";
    return !excludedPath || tx.request.path.toLowerCase().startsWith(excludedPath);
  });
};
export const endpointSuggestions = (transactions: HttpTransaction[], input: string, limit = 8) => {
  const query = input.trim().toLowerCase();
  if (!query) return [];
  return [...new Set(transactions.map(fullEndpoint))]
    .filter(endpoint => endpoint.toLowerCase().includes(query))
    .slice(0, limit);
};
export const preferredDevice = (current: string, devices: AndroidDevice[]) => {
  if (devices.some(device => device.serial === current && device.authorization_status === "authorized")) {
    return current;
  }
  return devices.find(device => device.connection_type === "usb" && device.authorization_status === "authorized")?.serial ??
    devices.find(device => device.authorization_status === "authorized")?.serial ?? "";
};
export const usbWifiHandoff = (endpoint: string, packageName: string, captureActive: boolean) => ({
  endpoint,
  refreshProxyOwnership: captureActive,
  restartLogcat: captureActive && Boolean(packageName),
  cleanupDevice: captureActive ? endpoint : undefined,
});
export const captureCleanupDevice = (configuredDevice: string | undefined, selectedDevice: string) =>
  configuredDevice || selectedDevice;
export const incidentLocation = (incident: LogIncident, packageName: string) =>
  incident.where_occurred ?? incident.foreground_activity ?? incident.first_app_frame ?? `${incident.lines[0]?.tag ?? packageName} · Logcat`;
export const developerIncidentReport = (incident:LogIncident, packageName:string) => `# ${incident.title}

Package: ${packageName || "unknown"}
Category: ${incident.category}
Occurrences: ${incident.occurrence_count}
First seen: ${new Date(incident.first_occurred_at).toISOString()}
Last seen: ${new Date(incident.occurred_at).toISOString()}
Where: ${incidentLocation(incident, packageName)}

## What happened
${incident.summary}

## How it happened
${incident.how_occurred}

## Likely cause
${incident.likely_cause}

## Reproduce
${incident.reproduction_steps.map((step,index)=>`${index+1}. ${step}`).join("\n")}

## Evidence
\`\`\`
${incident.lines.map(line=>`${line.level} ${line.tag}: ${line.message}`).join("\n")}
\`\`\``;
const jsonView = (value: string) => {
  try { return JSON.stringify(JSON.parse(value), null, 2); } catch { return value; }
};

export function App() {
  const [screen, setScreen] = useState<Screen>("toolkit");
  const [transactions, setTransactions] = useState<HttpTransaction[]>([]);
  const [selectedId, setSelectedId] = useState<string>();
  const [proxy, setProxy] = useState<ProxyStatus>("stopped");
  const [capturing, setCapturing] = useState(false);
  const [paused, setPaused] = useState(false);
  const [query, setQuery] = useState("");
  const [changedOnly, setChangedOnly] = useState(false);
  const [errorsOnly, setErrorsOnly] = useState(false);
  const [excludeInput, setExcludeInput] = useState("");
  const [excludedEndpoints, setExcludedEndpoints] = useState<string[]>([]);
  const [tab, setTab] = useState<InspectorTab>("Overview");
  const [devices, setDevices] = useState<AndroidDevice[]>([]);
  const [device, setDevice] = useState("");
  const [apps, setApps] = useState<AndroidApp[]>([]);
  const [appsLoading, setAppsLoading] = useState(false);
  const [packageName, setPackageName] = useState("");
  const [desktopHost, setDesktopHost] = useState("Resolving…");
  const [notice, setNotice] = useState("");
  const [connecting, setConnecting] = useState(false);
  const [enablingUsbWifi, setEnablingUsbWifi] = useState(false);
  const [certificateInstall, setCertificateInstall] = useState<AndroidCertificateInstall>();
  const [companionInstall, setCompanionInstall] = useState<CompanionInstall>();
  const [companionConnection, setCompanionConnection] = useState<CompanionConnection>();
  const [preparingCompanionConnection, setPreparingCompanionConnection] = useState(false);
  const [companionConnected, setCompanionConnected] = useState(false);
  const [installingCompanion, setInstallingCompanion] = useState(false);
  const [caStatus, setCaStatus] = useState<AndroidCaStatus>();
  const [caChanging, setCaChanging] = useState(false);
  const [incidents, setIncidents] = useState<LogIncident[]>([]);
  const hiddenTransactionIds = useRef(new Set<string>());
  const activeSessionId = useRef<string | undefined>(undefined);
  const configuredCaptureDevice = useRef<string | undefined>(undefined);
  const importInput = useRef<HTMLInputElement>(null);

  useEffect(() => {
    void api.getProxyStatus().then(setProxy);
    const refreshDevices = () => {
      void api.discoverDevices().then(items => {
        setDevices(items);
        setDevice(current => preferredDevice(current, items));
      }).catch(error => setNotice(`Could not refresh Android devices: ${String(error)}`));
    };
    refreshDevices();
    const deviceTimer = window.setInterval(refreshDevices, 2000);
    const stops = [
      listen<{payload: ProxyStatus}>("proxy-status-changed", e => setProxy(e.payload.payload)),
      listen<{payload: HttpTransaction}>("transaction-created", e =>
        setTransactions(current => activeSessionId.current !== e.payload.payload.session_id ||
          hiddenTransactionIds.current.has(e.payload.payload.id)
          ? current : [e.payload.payload, ...current])),
      listen<{payload: HttpTransaction}>("transaction-updated", e =>
        setTransactions(current => activeSessionId.current !== e.payload.payload.session_id ? current :
          current.map(tx => tx.id === e.payload.payload.id ? e.payload.payload : tx))),
      listen<{payload: HttpTransaction}>("transaction-completed", e =>
        setTransactions(current => activeSessionId.current !== e.payload.payload.session_id ||
          hiddenTransactionIds.current.has(e.payload.payload.id) ? current :
          current.some(tx => tx.id === e.payload.payload.id)
          ? current.map(tx => tx.id === e.payload.payload.id ? e.payload.payload : tx)
          : [e.payload.payload, ...current])),
      listen<{payload: LogIncident}>("incident-created", e => {
        const incident = e.payload.payload;
        setIncidents(current => {
          const previous = current.find(item => item.signature === incident.signature);
          if (!previous) return [incident, ...current].slice(0, 100);
          const merged = {...incident, id:previous.id, first_occurred_at:previous.first_occurred_at, occurrence_count:previous.occurrence_count + 1};
          return [merged, ...current.filter(item => item.signature !== incident.signature)].slice(0, 100);
        });
        setNotice(`Logcat: ${incident.title} — ${incident.summary}`);
      }),
    ];
    return () => {
      window.clearInterval(deviceTimer);
      void Promise.all(stops).then(unlisteners => unlisteners.forEach(stop => stop()));
    };
  }, []);
  useEffect(() => {
    if (!device) return;
    setAppsLoading(true);
    void api.listInstalledApps(device).then(items => {
      const devApps = items.filter(item => item.debuggable);
      setApps(devApps);
      setPackageName(current => devApps.some(item => item.package_name === current)
        ? current : devApps[0]?.package_name ?? "");
    }).catch(error => setNotice(`Could not load debuggable packages: ${String(error)}`))
      .finally(() => setAppsLoading(false));
  }, [device]);
  useEffect(() => {
    if (!device) { setCaStatus(undefined); return; }
    const connectionType = devices.find(item => item.serial === device)?.connection_type ?? "usb";
    const refresh = () => void api.getAndroidCaStatus(device, connectionType)
      .then(setCaStatus)
      .catch(error => setNotice(`Could not inspect Android CA status: ${String(error)}`));
    refresh();
    window.addEventListener("focus", refresh);
    return () => window.removeEventListener("focus", refresh);
  }, [device, devices]);
  useEffect(() => {
    if (!companionConnection) return;
    const refresh = () => void api.listCompanionApps(companionConnection.token).then(items => {
      if (!items.length) return;
      const remoteApps = items.map(item => ({package_name: item.package_name, version_name: item.label, debuggable: true}));
      setCompanionConnected(true);
      setApps(remoteApps);
      setPackageName(current => remoteApps.some(item => item.package_name === current) ? current : "");
      setNotice("Companion connected. Select an app, then start capture.");
    });
    refresh();
    const timer = window.setInterval(refresh, 1000);
    return () => window.clearInterval(timer);
  }, [companionConnection]);
  useEffect(() => {
    const connectionType = devices.find(item => item.serial === device)?.connection_type ?? "usb";
    void api.getProxyHost(connectionType)
      .then(setDesktopHost)
      .catch(() => setDesktopHost("Unavailable"));
  }, [device, devices]);
  useEffect(() => {
    if (!capturing || paused) return;
    const refresh = () => {
      void api.listTransactions().then(items =>
        setTransactions(items.filter(item => item.session_id === activeSessionId.current &&
          !hiddenTransactionIds.current.has(item.id)))
      ).catch(error =>
        setNotice(`Could not refresh captured traffic: ${String(error)}`));
    };
    refresh();
    const timer = window.setInterval(refresh, 750);
    return () => window.clearInterval(timer);
  }, [capturing, paused]);

  const visible = useMemo(() => transactions.filter(tx => {
    const haystack = `${tx.request.method} ${tx.request.host} ${tx.request.path} ${tx.response?.status ?? ""}`.toLowerCase();
    const activeExclusions = excludeInput.trim() ? [...excludedEndpoints, excludeInput] : excludedEndpoints;
    return haystack.includes(query.toLowerCase()) && (!changedOnly || displayState(tx) === "Changed") &&
      (!errorsOnly || displayState(tx) === "Failed" || tx.correlated_incidents.length > 0) &&
      !endpointIsExcluded(tx, activeExclusions);
  }), [transactions, query, changedOnly, errorsOnly, excludeInput, excludedEndpoints]);
  const suggestions = useMemo(() => endpointSuggestions(
    transactions.filter(tx => !endpointIsExcluded(tx, excludedEndpoints)), excludeInput
  ), [transactions, excludeInput, excludedEndpoints]);
  const selected = visible.find(tx => tx.id === selectedId) ?? visible[0];
  const changedCount = transactions.filter(tx => displayState(tx) === "Changed").length;
  const errorCount = transactions.filter(tx => displayState(tx) === "Failed" || tx.correlated_incidents.length > 0).length;
  const pendingCount = transactions.filter(tx => displayState(tx) === "Pending").length;
  const completedDurations = transactions.map(duration).filter((value): value is number => value != null);
  const averageDuration = completedDurations.length
    ? Math.round(completedDurations.reduce((sum, value) => sum + value, 0) / completedDurations.length)
    : 0;

  async function start(packageOverride?: string) {
    const capturePackage = packageOverride ?? packageName;
    let deviceProxyConfigured = false;
    try {
      if (!capturePackage) {
        setNotice("Select a debuggable package before starting capture.");
        return;
      }
      hiddenTransactionIds.current.clear();
      setTransactions([]);
      setSelectedId(undefined);
      setIncidents([]);
      activeSessionId.current = await api.startProxy();
      const proxyConfiguration = await api.getProxyConfiguration();
      if (companionConnection && companionConnected) {
        await api.selectCompanionPackage(companionConnection.token, capturePackage);
      } else if (device) {
        const selectedDevice = devices.find(item => item.serial === device);
        const host = await api.getProxyHost(selectedDevice?.connection_type ?? "usb");
        setDesktopHost(host);
        await api.configureAndroidProxy(device, host, proxyConfiguration.port);
        deviceProxyConfigured = true;
        configuredCaptureDevice.current = device;
      }
      if (device && !companionConnected) await api.startLogcatCapture(device, capturePackage);
      setCapturing(true); setNotice("Capture active. Navigate the Android app manually.");
    } catch (error) {
      if (deviceProxyConfigured && device) {
        await api.clearAndroidProxy(device).catch(() => undefined);
        configuredCaptureDevice.current = undefined;
      }
      await api.stopProxy().catch(() => undefined);
      if (String(error).includes("CA certificate")) await setupHttpsCapture();
      else setNotice(String(error));
    }
  }
  async function stop() {
    const failures:string[] = [];
    const cleanupDevice = captureCleanupDevice(configuredCaptureDevice.current, device);
    if (cleanupDevice) await api.clearAndroidProxy(cleanupDevice).catch(error => failures.push(String(error)));
    if (!failures.length) configuredCaptureDevice.current = undefined;
    await api.stopProxy().catch(error => failures.push(String(error)));
    try {
      setCapturing(false); setPaused(false);
      setNotice(failures.length
        ? `Capture stopped, but cleanup needs attention: ${failures.join(" · ")}`
        : "Capture stopped and the Android system proxy was cleared.");
    } finally {
      setCapturing(false); setPaused(false);
    }
  }
  async function switchUsbToWifi() {
    const selectedDevice = devices.find(item => item.serial === device);
    if (!selectedDevice || selectedDevice.connection_type !== "usb") return;
    setEnablingUsbWifi(true);
    try {
      const result = await api.enableUsbWifi(selectedDevice.serial);
      const handoff = usbWifiHandoff(
        result.endpoint,
        packageName,
        capturing && !companionConnected,
      );
      // `adb tcpip` closes the USB transport. Update selection first so Stop and
      // app-exit cleanup always target the reachable Wi-Fi endpoint.
      setDevice(handoff.endpoint);
      if (handoff.refreshProxyOwnership) {
        // Once the ADB transport has moved, the USB serial is no longer a safe
        // cleanup target. Transfer ownership before any operation that can fail.
        configuredCaptureDevice.current = handoff.cleanupDevice;
        try {
          const host = await api.getProxyHost("wireless");
          setDesktopHost(host);
          const proxyConfiguration = await api.getProxyConfiguration();
          await api.configureAndroidProxy(handoff.endpoint, host, proxyConfiguration.port);
        } catch (handoffError) {
          const failures: string[] = [];
          await api.clearAndroidProxy(handoff.endpoint).catch(error => failures.push(String(error)));
          if (!failures.length) configuredCaptureDevice.current = undefined;
          await api.stopProxy().catch(error => failures.push(String(error)));
          setCapturing(false); setPaused(false);
          throw new Error(failures.length
            ? `Wi-Fi handoff failed and cleanup needs attention: ${failures.join(" · ")}`
            : `Wi-Fi handoff failed; capture stopped and the Android proxy was cleared: ${String(handoffError)}`);
        }
      }
      if (handoff.restartLogcat) {
        await api.startLogcatCapture(handoff.endpoint, packageName).catch(error =>
          setNotice(`Capture continues over Wi-Fi, but Logcat could not restart: ${String(error)}`));
      }
      setNotice(capturing
        ? `Capture continues over Wi-Fi at ${handoff.endpoint}. You can now unplug USB.`
        : `Wi-Fi debugging is ready at ${handoff.endpoint}. Keep the phone and Mac on the same Wi-Fi, then you can unplug USB.`);
    } catch (error) {
      setNotice(`Could not switch ${selectedDevice.serial} to Wi-Fi: ${String(error)}`);
    } finally {
      setEnablingUsbWifi(false);
    }
  }
  async function openCompanionInstall() {
    try {
      setCompanionInstall(await api.prepareCompanionInstall());
    } catch (error) {
      setNotice(`Could not prepare the companion installer: ${String(error)}`);
    }
  }
  async function openCompanionConnection() {
    setPreparingCompanionConnection(true);
    setNotice("");
    try {
      const connectionType = devices.find(item => item.serial === device)?.connection_type ?? "usb";
      const host = await api.getProxyHost(connectionType);
      setDesktopHost(host);
      if (proxy !== "running") {
        activeSessionId.current = await api.startProxy();
        setProxy("running");
      }
      setCompanionConnection(await api.prepareCompanionConnection(host));
      setCompanionConnected(false);
    } catch (error) {
      setNotice(`Could not prepare companion connection: ${String(error)}`);
    } finally {
      setPreparingCompanionConnection(false);
    }
  }
  async function installCompanionDirectly() {
    if (!device) return;
    setInstallingCompanion(true);
    try {
      await api.installCompanion(device);
      setCompanionInstall(undefined);
    } catch (error) {
      setNotice(`Could not install the companion: ${String(error)}`);
    } finally {
      setInstallingCompanion(false);
    }
  }
  async function selectPackage(nextPackage: string) {
    if (nextPackage === packageName) return;
    setConnecting(true);
    try {
      if (capturing) {
        await stop();
      }
      setPackageName(nextPackage);
      if (capturing && nextPackage) {
        await start(nextPackage);
        setNotice(`Capture switched to ${nextPackage}.`);
      }
    } finally {
      setConnecting(false);
    }
  }
  async function setupHttpsCapture() {
    if (!device) { setNotice("Select an authorized Android device before setting up HTTPS capture."); return; }
    setConnecting(true);
    try {
      const install = await api.prepareAndroidCertificateInstall(device);
      setCertificateInstall(install);
    } catch (error) { setNotice(String(error)); }
    finally { setConnecting(false); }
  }
  async function changeCaUsage(useCa: boolean) {
    if (!device) { setNotice("Select an authorized USB device or emulator first."); return; }
    const connectionType = devices.find(item => item.serial === device)?.connection_type ?? "usb";
    setCaChanging(true);
    try {
      const result = await api.setAndroidCaUsage(device, connectionType, useCa);
      setCaStatus(result.status);
      if (result.requires_user_confirmation) {
        if (useCa) {
          setCertificateInstall({remote_path:"/sdcard/Download/AppTester-HTTPS-CA.pem",installer_output:""});
        } else {
          setNotice("Android opened Trusted credentials. Remove App Tester there; the device proxy has already been disabled.");
        }
      } else {
        setNotice(result.status.detail);
      }
    } catch (error) {
      setNotice(`Could not ${useCa ? "use" : "disable"} the App Tester CA: ${String(error)}`);
    } finally {
      setCaChanging(false);
    }
  }
  async function deleteAll() {
    if (!window.confirm("Permanently delete every saved capture, comparison baseline, and diagnostic record from this computer? This cannot be undone.")) return;
    setConnecting(true);
    try {
      await api.deleteAllTransactions();
      hiddenTransactionIds.current.clear();
      setTransactions([]); setSelectedId(undefined); setIncidents([]);
      activeSessionId.current = undefined;
      setNotice("Permanently deleted all saved capture data from this computer.");
    } catch (error) {
      setNotice(`Could not delete saved capture data: ${String(error)}`);
    } finally {
      setConnecting(false);
    }
  }
  async function exportCurrentCapture() {
    try {
      const path = await api.exportCaptureToFile();
      setNotice(`Exported redacted capture metadata to ${path}. Bodies and cURL are intentionally omitted.`);
    } catch (error) {
      if (String(error).includes("export canceled")) return;
      setNotice(`Could not export capture: ${String(error)}`);
    }
  }
  async function importPortableCapture(event: React.ChangeEvent<HTMLInputElement>) {
    const file = event.target.files?.[0];
    event.target.value = "";
    if (!file) return;
    if (file.size > 25 * 1024 * 1024) {
      setNotice("Capture import must be 25 MiB or smaller.");
      return;
    }
    try {
      const count = await api.importCapture(await file.text());
      activeSessionId.current = undefined;
      setTransactions(await api.listTransactions());
      setSelectedId(undefined);
      setIncidents([]);
      setNotice(`Imported ${count} redacted transaction${count === 1 ? "" : "s"} into a new local capture.`);
    } catch (error) { setNotice(`Could not import capture: ${String(error)}`); }
  }
  async function pinBaseline(tx: HttpTransaction) {
    const endpointId = baselineKey(tx);
    if (!endpointId || !tx.response) { setNotice("Only completed requests with a recognized endpoint can become baselines."); return; }
    setConnecting(true);
    try {
      await api.approveBaseline(endpointId, tx.id);
      setNotice("Pinned this response as the baseline for future compatible captures.");
    } catch (error) { setNotice(`Could not pin baseline: ${String(error)}`); }
    finally { setConnecting(false); }
  }
  async function removeBaseline(tx: HttpTransaction) {
    const endpointId = baselineKey(tx);
    if (!endpointId) return;
    setConnecting(true);
    try {
      const removed = await api.deleteBaseline(endpointId);
      setNotice(removed ? "Removed the pinned baseline for this endpoint." : "No pinned baseline exists for this endpoint.");
    } catch (error) { setNotice(`Could not remove baseline: ${String(error)}`); }
    finally { setConnecting(false); }
  }
  async function testYesterday() {
    if (!window.confirm("Replay every testable API captured yesterday? Requests may include state-changing methods. Requests containing redacted credentials or data are skipped.")) return;
    setConnecting(true);
    try {
      const summary = await api.testYesterdaysApis();
      setNotice(`Yesterday replay: ${summary.completed} completed, ${summary.changed} changed, ${summary.failed} failed, ${summary.skipped} skipped.`);
    } catch (error) { setNotice(String(error)); }
    finally { setConnecting(false); }
  }
  function copy(value: string) { void navigator.clipboard.writeText(value); setNotice("Copied to clipboard"); }
  function addExclusion() {
    const value = excludeInput.trim();
    if (!value) return;
    setExcludedEndpoints(current => current.some(item => item.toLowerCase() === value.toLowerCase())
      ? current : [...current, value]);
    setExcludeInput("");
  }

  return <main className="app-shell">
    <aside className="app-sidebar">
      <div className="app-mark"><Activity/><span><b>App Tester</b><small>Android diagnostics</small></span></div>
      <nav aria-label="Primary navigation">
        <button className={screen === "toolkit" ? "active" : ""} onClick={() => setScreen("toolkit")}>
          <ListTree/><span><b>Toolkit</b><small>API traffic & schemas</small></span>
        </button>
        <button className={screen === "logs" ? "active" : ""} onClick={() => setScreen("logs")}>
          <TerminalSquare/><span><b>Inspect logs</b><small>Errors & warnings</small></span>
          {incidents.length > 0 && <i>{incidents.length}</i>}
        </button>
      </nav>
      <div className={`sidebar-status ${capturing ? "live" : ""}`}><Circle/>
        <span><b>{capturing ? "Capture active" : "Ready"}</b><small>{packageName || "Select a package"}</small></span>
      </div>
    </aside>
    <section className="shell">
    <header>
      <div className={`proxy ${proxy}`}><Circle/>{proxy === "running" ? "Connected · Capturing automatically" : `Proxy ${proxy.replaceAll("_"," ")}`}</div>
      <select aria-label="Device" value={device} onChange={e=>setDevice(e.target.value)}>
        <option value="">Select device</option>{devices.map(item=><option key={item.serial}>{item.serial}</option>)}
      </select>
      {devices.find(item => item.serial === device)?.connection_type === "usb" && <button
        className="wireless-device"
        disabled={enablingUsbWifi}
        title="Keep capture and debugging available after unplugging USB"
        onClick={()=>void switchUsbToWifi()}>
        <Wifi/>{enablingUsbWifi ? "Enabling Wi-Fi…" : "USB to Wi-Fi"}
      </button>}
      <button aria-label="Download Android companion" title="Download the Android companion" onClick={()=>void openCompanionInstall()}>
        <ShieldCheck/>Download app
      </button>
      <button aria-label="Connect Android companion" disabled={preparingCompanionConnection} title="Connect the installed companion without USB" onClick={()=>void openCompanionConnection()}>
        <ShieldCheck/>{preparingCompanionConnection ? "Preparing QR…" : "Connect companion"}
      </button>
      <div className={`ca-state ${caStatus?.state ?? "unknown"}`} title={caStatus?.detail ?? "Select a device to inspect CA status"}>
        <ShieldCheck/><span>CA {caStatus?.state.replace("_"," ") ?? "unknown"}</span>
      </div>
      <button disabled={caChanging || !device || caStatus?.state === "installed"}
        onClick={()=>void changeCaUsage(true)}><ShieldCheck/>{caChanging ? "Checking…" : "Use CA"}</button>
      <button disabled={caChanging || !device || caStatus?.state === "not_installed"}
        onClick={()=>void changeCaUsage(false)}><X/>Don’t use CA</button>
      <select aria-label="Package" value={packageName} disabled={appsLoading || connecting}
        onChange={e=>void selectPackage(e.target.value)}>
        <option value="">{appsLoading ? "Loading dev packages…" : "Select dev package"}</option>
        {apps.map(app=><option key={app.package_name}>{app.package_name}</option>)}
      </select>
      <button aria-label="Settings" title="Settings"><Settings/></button>
    </header>
    {screen === "toolkit" ? <><section className="screen-heading">
      <div><span>Toolkit</span><h1>API traffic</h1></div>
      <p><span className="desktop-host">Desktop host: {desktopHost}</span>Only traffic from the current <b>{packageName || "selected package"}</b> capture is shown.</p>
    </section>
    <section className="filters">
      <label className="search"><Search/><input aria-label="Search captured traffic" placeholder="Search method, host, path, status…" value={query} onChange={e=>setQuery(e.target.value)}/></label>
      <button aria-pressed={changedOnly} className={changedOnly?"active":""} onClick={()=>setChangedOnly(v=>!v)}><Filter/>Changed only</button>
      <button aria-pressed={errorsOnly} className={errorsOnly?"active":""} onClick={()=>setErrorsOnly(v=>!v)}><AlertCircle/>Errors only</button>
      <button title="Showing today’s captures"><CalendarDays/>Today</button>
      <button disabled={!transactions.length} onClick={()=>void exportCurrentCapture()}><Download/>Export redacted</button>
      <input ref={importInput} className="visually-hidden" type="file" accept="application/json,.json" onChange={event=>void importPortableCapture(event)} />
      <button onClick={()=>importInput.current?.click()}><Upload/>Import capture</button>
      <button className="danger" title="Permanently delete all saved capture data from this computer"
        onClick={()=>void deleteAll()}><Trash2/>Delete all</button>
      {capturing ? <><button onClick={()=>setPaused(v=>!v)}>{paused?<Play/>:<Pause/>}{paused?"Resume capture":"Pause capture"}</button>
        <button className="danger" onClick={()=>void stop()}><Square/>Stop capture</button></> :
        <button className="primary" onClick={()=>void start()}><Play/>Start capture</button>}
      <div className="metrics">
        <span>Requests<b>{transactions.length}</b></span><span>Changed<b>{changedCount}</b></span>
        <span>Errors<b className="metric-error">{errorCount}</b></span><span>Pending<b>{pendingCount}</b></span>
        <span>Avg duration<b>{averageDuration} ms</b></span>
      </div>
    </section>
    <section className="exclude-bar">
      <span>Exclude APIs (negative filter)</span>
      <div className="filter-chip-scroll">{excludedEndpoints.map(endpoint => <button className="filter-chip" key={endpoint}
        title={`Remove ${endpoint}`} onClick={()=>setExcludedEndpoints(current=>current.filter(item=>item!==endpoint))}>
        {compactEndpoint(endpoint)}<X/>
      </button>)}</div>
      <div className="exclude-field">
      <label className="exclude-input">
        <Search/><input aria-label="Exclude full endpoint" placeholder="Paste a full endpoint and press Enter…"
          value={excludeInput} onChange={event=>setExcludeInput(event.target.value)}
          onKeyDown={event=>{if(event.key==="Enter"){event.preventDefault();addExclusion();}}}/>
      </label>
      {suggestions.length > 0 && <div className="exclude-options" role="listbox" aria-label="Matching endpoints">
        {suggestions.map(endpoint => <button key={endpoint} role="option" onClick={()=>{
          setExcludedEndpoints(current => current.some(item => item.toLowerCase() === endpoint.toLowerCase())
            ? current : [...current, endpoint]);
          setExcludeInput("");
        }}>{endpoint}</button>)}
      </div>}
      </div>
      <button title="Add exclusion" disabled={!excludeInput.trim()} onClick={addExclusion}><SlidersHorizontal/>Exclude</button>
    </section>
    {notice && <div className="notice" role="status" aria-live="polite">{notice}</div>}
    <section className="workspace">
      <div className="traffic">
        <div className="cache-alert"><AlertCircle/><span><b>Schema comparison</b> · Changes are detected from JSON keys and types, not values.</span></div>
        <div className="table-head"><span>Time</span><span>Method</span><span>Host / Path</span><span>Status</span><span>Duration</span><span>Size</span><span>Change</span><span>Issues</span></div>
        <div className="rows">
          {visible.map(tx => <button key={tx.id} onClick={()=>setSelectedId(tx.id)}
            className={`row ${selected?.id===tx.id?"selected":""} ${displayState(tx).toLowerCase()}`}>
            <span>{new Date(tx.created_at).toLocaleTimeString([], {hour12:false})}</span>
            <b className={`method ${tx.request.method.toLowerCase()}`}>{tx.request.method}</b>
            <span className="target"><strong>{tx.request.host}</strong><small>{tx.request.path}</small></span>
            <span>{tx.response?.status ?? "—"}</span><span>{duration(tx) == null ? "Pending" : `${duration(tx)} ms`}</span>
            <span>{tx.response ? `${tx.response.decoded_size} B` : "—"}</span>
            <span className="change">{displayState(tx)}</span><span>{tx.correlated_incidents.length || "—"}</span>
          </button>)}
          {!visible.length && <div className="empty"><ShieldCheck/><strong>No captured traffic yet</strong>
            <span>{proxy==="running"?"Navigate the selected Android app manually.":"Start capture and configure the device proxy to see requests live."}</span></div>}
        </div>
      </div>
      <aside className="inspector">
        {selected ? <><div className="inspector-title"><div><b>{selected.request.method}</b><strong>{selected.request.host}{selected.request.path}</strong></div>
          <button aria-label="Copy request URL" onClick={()=>copy(`${selected.request.scheme}://${selected.request.host}${selected.request.path}`)}><Copy/>URL</button></div>
          <nav>{(["Overview","Request","Response","Compare","cURL","Logs","Timeline"] as InspectorTab[]).map(name=>
            <button className={tab===name?"active":""} onClick={()=>setTab(name)} key={name}>{name}</button>)}</nav>
          <div className="panel">{tab==="Overview" && <Overview tx={selected}/>}
            {tab==="Request" && <Message headers={selected.request.headers} body={bodyText(selected.request.body)} onCopy={copy}/>}
            {tab==="Response" && <Message headers={selected.response?.headers ?? []} body={bodyText(selected.response?.body)} onCopy={copy}/>}
            {tab==="Compare" && <Compare tx={selected} onPin={()=>void pinBaseline(selected)} onRemove={()=>void removeBaseline(selected)}/>}
            {tab==="cURL" && <Code value={selected.curl?.multiline ?? "cURL is generated when the request is captured."} onCopy={copy}/>}
            {tab==="Logs" && <Logs incidents={incidents.filter(incident => selected.correlated_incidents.includes(incident.id))}/>}
            {tab==="Timeline" && <Timeline tx={selected}/>}</div>
        </> : <div className="empty"><Activity/><strong>Select a request</strong><span>Request, response, comparison, cURL and correlated logs will appear here.</span></div>}
      </aside>
    </section></> : <LogInspector incidents={incidents} packageName={packageName} capturing={capturing}
      onStart={() => void start()} />}
    {certificateInstall && <div className="modal-backdrop" role="presentation">
      <section className="qr-dialog connection-dialog" role="dialog" aria-modal="true" aria-labelledby="certificate-title">
        <button className="close" aria-label="Close" onClick={()=>setCertificateInstall(undefined)}><X/></button>
        <div className="qr-heading"><ShieldCheck/><div><h2 id="certificate-title">Finish HTTPS capture setup</h2><p>One required Android security confirmation</p></div></div>
        <p>App Tester generated the local certificate, copied it to your device, and opened Android’s credential installer.</p>
        <ol><li>Choose <b>CA certificate</b> if Android asks for a credential type.</li><li>Select <b>AppTester-HTTPS-CA.pem</b> from Downloads.</li><li>Confirm the Android warning, then return here.</li></ol>
        <p className="warning">Android requires this approval so no desktop app can silently add a certificate that could inspect encrypted traffic.</p>
        <button className="primary submit" onClick={()=>{setCertificateInstall(undefined); void start();}}>I installed it — start capture</button>
      </section>
    </div>}
    {companionInstall && <div className="modal-backdrop" role="presentation">
      <section className="qr-dialog connection-dialog" role="dialog" aria-modal="true" aria-labelledby="companion-title">
        <button className="close" aria-label="Close" onClick={()=>setCompanionInstall(undefined)}><X/></button>
        <div className="qr-heading"><ShieldCheck/><div><h2 id="companion-title">Install App Tester Companion 0.2.2</h2><p>Includes mobile disconnect controls</p></div></div>
        <div className="qr-image" dangerouslySetInnerHTML={{__html:companionInstall.qr_svg}} />
        <ol><li>Scan this code with the phone camera.</li><li>Download the signed APK from the App Tester GitHub repository.</li><li>Approve Android’s one-time install confirmation, then open the companion.</li></ol>
        {device && <button className="primary submit" disabled={installingCompanion} onClick={()=>void installCompanionDirectly()}>{installingCompanion ? "Installing…" : "Install directly on selected device"}</button>}
        <p className="warning">If Android blocks browser downloads, select a paired Wi-Fi device above and use direct install.</p>
      </section>
    </div>}
    {companionConnection && <div className="modal-backdrop" role="presentation">
      <section className="qr-dialog connection-dialog" role="dialog" aria-modal="true" aria-labelledby="companion-connect-title">
        <button className="close" aria-label="Close" onClick={()=>setCompanionConnection(undefined)}><X/></button>
        <div className="qr-heading"><Wifi/><div><h2 id="companion-connect-title">Connect companion</h2><p>Requires Companion 0.2 or newer</p></div></div>
        <div className="qr-image" dangerouslySetInnerHTML={{__html:companionConnection.qr_svg}} />
        <p>Open App Tester Companion and scan. Installed apps will appear in the package picker automatically.</p>
        <button onClick={()=>{setCompanionConnection(undefined); void openCompanionInstall();}}>Install companion instead</button>
      </section>
    </div>}
  </section></main>;
}
function Overview({tx}:{tx:HttpTransaction}) { return <div className="overview">
  <label>Status<strong>{tx.response?.status ?? "Pending"}</strong></label><label>Duration<strong>{duration(tx) ?? "—"} ms</strong></label>
  <label>Content type<strong>{tx.response?.content_type ?? tx.request.content_type ?? "Unknown"}</strong></label>
  <label>HTTP<strong>{tx.response?.http_version ?? tx.request.http_version}</strong></label>
  <label>Capture quality<strong>{tx.capture_quality}</strong></label><label>Change<strong className={displayState(tx)==="Changed"?"red":""}>{displayState(tx)}</strong></label>
</div>; }
function Message({headers,body,onCopy}:{headers:{name:string;value:string}[];body:string;onCopy:(v:string)=>void}) {
  return <><h3>Headers <button onClick={()=>onCopy(headers.map(h=>`${h.name}: ${h.value}`).join("\n"))}><Copy/>Copy</button></h3>
    <div className="headers">{headers.map((h,i)=><div key={`${h.name}-${i}`}><b>{h.name}</b><span>{h.value}</span></div>)}</div>
    <h3>Body <button onClick={()=>onCopy(body)}><Copy/>Copy raw</button></h3><pre>{jsonView(body) || "No body"}</pre></>;
}
function Code({value,onCopy}:{value:string;onCopy:(v:string)=>void}) { return <div className="code"><button onClick={()=>onCopy(value)}><Copy/>Copy</button><pre>{value}</pre></div>; }
function Compare({tx,onPin,onRemove}:{tx:HttpTransaction;onPin:()=>void;onRemove:()=>void}) {
  const diffs=tx.comparison?.differences ?? [];
  const endpointId = baselineKey(tx);
  const canPin=Boolean(endpointId && tx.response);
  const [rules, setRules] = useState<ComparisonRules>({ignored_json_pointers:[], volatile_keys:[]});
  const [rulesMessage, setRulesMessage] = useState("");
  useEffect(() => {
    if (!endpointId) { setRules({ignored_json_pointers:[], volatile_keys:[]}); return; }
    void api.getComparisonRules(endpointId).then(setRules).catch(() => setRulesMessage("Could not load comparison rules."));
  }, [endpointId]);
  const saveRules = async () => {
    if (!endpointId) return;
    try { await api.saveComparisonRules(endpointId, rules); setRulesMessage("Rules saved for future comparisons."); }
    catch (error) { setRulesMessage(`Could not save rules: ${String(error)}`); }
  };
  return <div className="compare-panel">
  <div className="compare-toolbar"><div><b>DTO schema comparison</b><span>Values and array lengths are ignored</span></div><span className="compare-mode">Key structure</span></div>
  <div className="compare-toolbar"><span>{tx.comparison?.baseline_transaction_id ? "Compared with a saved or recent baseline" : "No saved baseline selected"}</span>
    <div><button disabled={!canPin} onClick={onPin}>Pin as baseline</button><button disabled={!canPin} onClick={onRemove}>Remove baseline</button></div></div>
  <section className="comparison-rules"><h3>Comparison rules</h3><p>Ignore exact JSON paths or volatile key names for this endpoint.</p>
    <label>Ignored JSON paths<input aria-label="Ignored JSON paths" value={rules.ignored_json_pointers.join(", ")} onChange={event=>setRules(current=>({...current,ignored_json_pointers:event.target.value.split(",").map(value=>value.trim()).filter(Boolean)}))} placeholder="$.updatedAt, $.meta.requestId"/></label>
    <label>Volatile key names<input aria-label="Volatile key names" value={rules.volatile_keys.join(", ")} onChange={event=>setRules(current=>({...current,volatile_keys:event.target.value.split(",").map(value=>value.trim()).filter(Boolean)}))} placeholder="timestamp, requestId"/></label>
    <button disabled={!endpointId} onClick={()=>void saveRules()}>Save rules</button>{rulesMessage && <span>{rulesMessage}</span>}
  </section>
  <h3>{diffs.length ? `${diffs.length} schema differences` : tx.comparison?.baseline_transaction_id ? "DTO shape unchanged" : "No compatible comparison available"}</h3>
  {diffs.map((diff,i)=><article className={`diff ${diff.severity}`} key={i}><b>{diff.path ?? diff.kind}</b><span>{diff.explanation}</span>
    <pre>Previous: {diff.previous ?? "—"}{"\n"}Current: {diff.current ?? "—"}</pre></article>)}</div>; }
function Logs({incidents}:{incidents:LogIncident[]}) { return incidents.length ? <div className="logs">{incidents.map(incident =>
  <article className="log-incident" key={incident.id}><b>{incident.title}</b><span>{incident.message}</span>
    {incident.lines.map((line, index)=><pre key={index}>{line.level} {line.tag}: {line.message}</pre>)}</article>)}</div> :
  <div className="empty compact">No errors, warnings, or actionable issues captured for the selected app yet.</div>; }
function LogInspector({incidents, packageName, capturing, onStart}:{
  incidents:LogIncident[]; packageName:string; capturing:boolean; onStart:()=>void;
}) {
  const errors = incidents.filter(item => ["crash","error","anr","dto_parsing"].includes(item.category)).length;
  const warnings = incidents.length - errors;
  const latest = incidents[0];
  return <section className="log-screen">
    <div className="screen-heading">
      <div><span>Inspect logs</span><h1>Detected app issues</h1></div>
      <p>Actionable Logcat errors for <b>{packageName || "the selected package"}</b>.</p>
      {!capturing && <button className="primary" onClick={onStart}><Play/>Start capture</button>}
    </div>
    <div className="log-summary">
      <article><span>Total detected</span><strong>{incidents.length}</strong><small>Actionable issues</small></article>
      <article className="error"><span>Errors</span><strong>{errors}</strong><small>Crashes and failures</small></article>
      <article><span>Warnings</span><strong>{warnings}</strong><small>Potential problems</small></article>
      <article><span>Package</span><b>{packageName || "Not selected"}</b><small>{capturing ? "Monitoring live" : "Capture stopped"}</small></article>
      {latest && <article className="issue-overview">
        <div><span>Latest issue · How it happened</span><strong>{latest.title}</strong><p>{latest.summary}</p>
          {latest.root_cause && <small>Root cause: {latest.root_cause}</small>}</div>
        <div><span>Screen &amp; navigation context</span><code>{incidentLocation(latest, packageName)}</code>
          <small>{new Date(latest.occurred_at).toLocaleString()}</small></div>
      </article>}
    </div>
    <div className="log-list">
      {incidents.map(incident => <article className="log-card" key={incident.id}>
        <div className="log-severity"><AlertCircle/><span>{incident.category.replaceAll("_"," ")}</span></div>
        <div className="log-content">
          <div className="log-title"><div><h2>{incident.title}</h2><p>{incident.summary}</p>
            <small>{incident.occurrence_count} occurrence{incident.occurrence_count === 1 ? "" : "s"}</small>
            {incident.root_cause && <small>Root cause: {incident.root_cause}</small>}</div>
            <time>{new Date(incident.occurred_at).toLocaleTimeString()}</time></div>
          <div className="detected-at"><span>Screen &amp; navigation context</span>
            <code>{incidentLocation(incident, packageName)}</code></div>
          <div className="issue-analysis"><h3>How it happened</h3><p>{incident.how_occurred}</p>
            <h3>Likely cause</h3><p>{incident.likely_cause}</p>
            <h3>How to reproduce</h3><ol>{incident.reproduction_steps.map((step,index)=><li key={index}>{step}</li>)}</ol>
            <button onClick={()=>void navigator.clipboard.writeText(developerIncidentReport(incident, packageName))}><Copy/>Copy developer report</button></div>
          <details><summary>View {incident.lines.length} captured log {incident.lines.length === 1 ? "line" : "lines"}</summary>
            {incident.lines.map((line,index)=><pre key={index}>{line.level} {line.tag}: {line.message}</pre>)}</details>
        </div>
      </article>)}
      {!incidents.length && <div className="empty log-empty"><ShieldCheck/><strong>No issues detected</strong>
        <span>{capturing ? `App Tester is monitoring ${packageName}. Detected errors will appear here with their source.`
          : "Start capture to monitor errors and warnings for the selected dev package."}</span></div>}
    </div>
  </section>;
}
function Timeline({tx}:{tx:HttpTransaction}) { return <ol className="timeline">
  <li>Request started <time>{new Date(tx.timing.request_started_ms).toLocaleTimeString()}</time></li>
  {tx.timing.request_complete_ms&&<li>Request complete</li>}{tx.timing.response_started_ms&&<li>Response headers</li>}
  {tx.timing.response_complete_ms&&<li>Response complete</li>}</ol>; }
