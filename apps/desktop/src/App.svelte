<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import { Activity, AlertCircle, ChevronDown, Circle, Copy, Download, FolderUp, ListTree, LogOut, Pause, Play, Search, ShieldCheck, Smartphone, Square, TerminalSquare, Trash2, Wifi, X } from "lucide-svelte";
  import * as api from "./api";
  import type { AndroidApp, AndroidDevice, HttpTransaction, LogIncident, ProxyStatus } from "./types";

  type Screen = "traffic" | "logs";
  type Tab = "Overview" | "Request" | "Response" | "Compare" | "cURL" | "Timeline";
  type InspectorEvent<T> = { kind: string; payload: T };
  let screen: Screen = "traffic";
  let tab: Tab = "Overview";
  let proxy: ProxyStatus = "stopped";
  let devices: AndroidDevice[] = [];
  let device = "";
  let apps: AndroidApp[] = [];
  let packageName = "";
  let packageSearch = "";
  let packagePickerOpen = false;
  let devicesOpen = false;
  let transactions: HttpTransaction[] = [];
  let selectedId = "";
  let incidents: LogIncident[] = [];
  let query = "";
  let changedOnly = false;
  let errorsOnly = false;
  let capturing = false;
  let paused = false;
  let busy = false;
  let notice = "";
  let desktopHost = "Resolving…";
  let activeSessionId: string | undefined;
  let transactionRefreshInFlight = false;
  let expandedIssue = "";
  let importInput: HTMLInputElement;
  let capturedTransactions: HttpTransaction[] = [];
  let visibleTransactions: HttpTransaction[] = [];
  let selectedTransaction: HttpTransaction | undefined;

  const duration = (tx: HttpTransaction) => tx.timing.response_complete_ms == null ? undefined : tx.timing.response_complete_ms - tx.timing.request_started_ms;
  const state = (tx: HttpTransaction) => !tx.response ? "Pending" : tx.response.status >= 400 ? "Failed" : tx.comparison?.differences.some(d => !d.ignored) ? "Changed" : "Captured";
  const body = (value: HttpTransaction["request"]["body"] | undefined) => {
    if (!value || value.storage === "empty") return "No body";
    if (value.storage === "unavailable") return value.reason;
    return new TextDecoder().decode(new Uint8Array(value.storage === "inline" ? value.bytes : value.preview));
  };
  const pretty = (value: string) => { try { return JSON.stringify(JSON.parse(value), null, 2); } catch { return value; } };
  const statusLabel = () => capturing ? "Capturing live" : proxy === "running" ? "Proxy ready" : "Ready to capture";
  $: capturedTransactions = transactions.filter(tx => tx.request.method.toUpperCase() !== "CONNECT");
  $: visibleTransactions = capturedTransactions.filter(tx => {
    const searchable = `${tx.request.method} ${tx.request.host} ${tx.request.path} ${tx.response?.status ?? ""}`.toLowerCase();
    return searchable.includes(query.toLowerCase()) && (!changedOnly || state(tx) === "Changed") && (!errorsOnly || state(tx) === "Failed" || tx.correlated_incidents.length > 0);
  });
  $: selectedTransaction = capturedTransactions.find(tx => tx.id === selectedId) ?? visibleTransactions[0];
  const selected = () => selectedTransaction;
  const selectedDevice = () => devices.find(item => item.serial === device);
  const matchingApps = () => {
    const search = packageSearch.trim().toLowerCase();
    return apps
      .filter(app => !search || `${app.package_name} ${app.version_name ?? ""}`.toLowerCase().includes(search))
      .slice(0, search ? 50 : 8);
  };
  const copy = (value: string) => { void navigator.clipboard.writeText(value); notice = "Copied to clipboard"; };
  const closePickers = () => { packagePickerOpen = false; devicesOpen = false; packageSearch = ""; };
  const endpointId = (tx: HttpTransaction) => tx.endpoint_identity && `${tx.endpoint_identity.method} ${tx.endpoint_identity.host} ${tx.endpoint_identity.path_template}`;
  async function approveBaseline(tx: HttpTransaction) {
    const id = endpointId(tx);
    if (!id) { notice = "This response does not have a comparable endpoint identity yet."; return; }
    try { await api.approveBaseline(id, tx.id); notice = "Response saved as the JSON-key baseline for this endpoint."; }
    catch (error) { notice = `Could not save baseline: ${String(error)}`; }
  }
  function upsertIncident(issue: LogIncident) {
    const existing = incidents.find(item => item.signature === issue.signature);
    const occurrence_count = Math.max(issue.occurrence_count, existing ? existing.occurrence_count + 1 : 1);
    incidents = [{ ...issue, occurrence_count }, ...incidents.filter(item => item.signature !== issue.signature)].slice(0, 100);
  }
  const upsertTransaction = (transaction: HttpTransaction) => {
    // The native proxy is the authority for the active capture.  The WebView's
    // cached session can lag after a reconnect or restore; dropping its event
    // leaves a database row invisible until some unrelated UI refresh.
    transactions = [transaction, ...transactions.filter(item => item.id !== transaction.id)];
  };
  const reconcileTransactions = (fresh: HttpTransaction[]) => {
    // Event delivery gives us the lowest-latency update, while the database
    // read repairs anything delivered while the WebView was unavailable. Do
    // not let an empty/stale read erase a transaction that has just arrived.
    const byId = new Map(transactions.map(item => [item.id, item]));
    fresh.forEach(item => byId.set(item.id, item));
    transactions = [...byId.values()].sort((left, right) =>
      right.created_at.localeCompare(left.created_at),
    );
  };
  async function refreshTransactions() {
    if (transactionRefreshInFlight) return;
    transactionRefreshInFlight = true;
    try { reconcileTransactions(await api.listTransactions(activeSessionId)); }
    catch { /* Live events remain the primary update path. */ }
    finally { transactionRefreshInFlight = false; }
  }
  async function refreshProxyStatus() {
    try {
      const next = await api.getProxyStatus();
      if (capturing && proxy === "running" && next !== "running") {
        capturing = false;
        paused = false;
        notice = "Capture proxy stopped unexpectedly. Reopen the companion to start a fresh capture.";
      }
      proxy = next;
    } catch { /* Keep the last known state while the native bridge reconnects. */ }
  }

  async function refreshDevices() {
    try {
      devices = await api.discoverDevices();
      const nextDevice = devices.some(item => item.serial === device && item.authorization_status === "authorized")
        ? device
        : devices.find(item => item.connection_type === "usb" && item.authorization_status === "authorized")?.serial
          ?? devices.find(item => item.authorization_status === "authorized")?.serial
          ?? "";
      if (nextDevice !== device) {
        device = nextDevice;
        // A USB device is normally selected automatically, so load its
        // debuggable apps here rather than waiting for a second click.
        void loadApps();
        void resolveHost();
      }
    } catch (error) { notice = `Could not refresh Android devices: ${String(error)}`; }
  }
  async function loadApps() {
    if (!device) { apps = []; packageName = ""; return; }
    try {
      apps = await api.listInstalledApps(device);
      // Package discovery can briefly return an incomplete list while ADB is
      // reconnecting. Never clear the target from a live capture because the
      // capture session remains scoped to that package.
      if (packageName && !capturing && !apps.some(item => item.package_name === packageName)) packageName = "";
    } catch (error) { notice = `Could not load debuggable packages: ${String(error)}`; }
  }
  function chooseDevice(serial: string) { device = serial; closePickers(); void loadApps(); void resolveHost(); }
  function choosePackage(name: string) { packageName = name; packageSearch = ""; packagePickerOpen = false; notice = `Selected ${name}`; }
  async function resolveHost() {
    try { desktopHost = await api.getProxyHost(selectedDevice()?.connection_type ?? "usb"); } catch { desktopHost = "Unavailable"; }
  }
  async function start() {
    if (selectedDevice()?.connection_type === "usb") { await connectCompanion(); return; }
    if (!packageName) { notice = "Choose a package before starting capture."; packagePickerOpen = true; return; }
    busy = true;
    try {
      const current = selectedDevice();
      if (!current) throw new Error("Choose an authorized Android device first.");
      if (current.connection_type !== "emulator") {
        await connectCompanion();
        return;
      }
      activeSessionId = await api.startProxy();
      const host = await api.getProxyHost(current.connection_type);
      desktopHost = host;
      if (current.connection_type === "emulator") {
        const config = await api.getProxyConfiguration();
        await api.configureAndroidProxy(device, host, config.port);
      }
      await api.startLogcatCapture(device, packageName).catch(() => undefined);
      transactions = []; incidents = []; selectedId = ""; capturing = true; paused = false;
      notice = `Capture active for ${packageName}. Navigate the app to see traffic.`;
    } catch (error) { await api.stopProxy().catch(() => undefined); notice = String(error); }
    finally { busy = false; }
  }
  async function stop() {
    busy = true;
    try {
      if (device && selectedDevice()?.connection_type === "emulator") await api.clearAndroidProxy(device);
      await api.stopProxy(); capturing = false; paused = false; notice = "Capture stopped.";
    } catch (error) { notice = `Could not stop capture: ${String(error)}`; }
    finally { busy = false; }
  }
  async function connectCompanion() {
    busy = true;
    try {
      const current = selectedDevice();
      if (!current) throw new Error("Choose an authorized Android device first.");
      if (current.connection_type !== "usb") throw new Error("Companion capture requires a USB-connected Android device.");
      if (!packageName) throw new Error("Choose the target package before opening the companion.");
      const connection = await api.openUsbCompanion(device, packageName);
      activeSessionId = connection.session_id;
      capturing = true;
      transactions = [];
      await refreshTransactions();
      await api.startLogcatCapture(device, packageName).catch(() => undefined);
      notice = `Desktop capture endpoint is ready on port ${connection.port}. On your phone, stop and reconnect VPN once to apply this endpoint.`;
    } catch (error) { notice = `Could not open companion: ${String(error)}`; }
    finally { busy = false; }
  }
  async function exportCapture() { try { notice = `Exported capture to ${await api.exportCaptureToFile()}`; } catch (error) { notice = String(error); } }
  async function deleteAll() {
    if (!window.confirm("Delete every captured request and diagnostic from this Mac? This cannot be undone.")) return;
    busy = true;
    try {
      await api.deleteAllTransactions();
      transactions = [];
      incidents = [];
      selectedId = "";
      notice = "Deleted all captured traffic and diagnostics.";
    } catch (error) { notice = `Could not delete captures: ${String(error)}`; }
    finally { busy = false; }
  }
  async function importCapture(event: Event) {
    const file = (event.target as HTMLInputElement).files?.[0];
    if (!file) return;
    try { await api.importCapture(await file.text()); transactions = await api.listTransactions(); notice = "Imported redacted capture."; } catch (error) { notice = String(error); }
    (event.target as HTMLInputElement).value = "";
  }
  onMount(() => {
    void refreshProxyStatus();
    void refreshDevices();
    // Discovery is now a single lightweight ADB command, so poll promptly and
    // surface a newly attached USB device without a noticeable delay.
    const deviceTimer = window.setInterval(refreshDevices, 750);
    // The native event bridge can miss a status change after a window restore.
    // Reconcile it so the controls never claim that a stopped proxy is live.
    const proxyTimer = window.setInterval(() => void refreshProxyStatus(), 1000);
    // Tauri events can be missed while the WebView is suspended or restored.
    // Reconcile the current capture session frequently so new requests appear
    // without requiring a view change.
    const transactionTimer = window.setInterval(() => void refreshTransactions(), 500);
    const unlisteners = [
      listen<InspectorEvent<ProxyStatus>>("proxy-status-changed", event => proxy = event.payload.payload),
      listen<InspectorEvent<HttpTransaction>>("transaction-created", event => upsertTransaction(event.payload.payload)),
      listen<InspectorEvent<HttpTransaction>>("transaction-updated", event => upsertTransaction(event.payload.payload)),
      listen<InspectorEvent<HttpTransaction>>("transaction-completed", event => upsertTransaction(event.payload.payload)),
      listen<InspectorEvent<LogIncident>>("incident-created", event => upsertIncident(event.payload.payload))
    ];
    return () => { window.clearInterval(deviceTimer); window.clearInterval(proxyTimer); window.clearInterval(transactionTimer); void Promise.all(unlisteners).then(items => items.forEach(stop => stop())); };
  });
</script>

<svelte:window onclick={closePickers} />
<main class="svelte-app">
  <aside class="sidenav">
    <div class="wordmark"><Activity size={22}/><span><small>Android diagnostics</small></span></div>
    <nav aria-label="Primary navigation">
      <button class:active={screen === "traffic"} onclick={() => screen = "traffic"}><ListTree/><span><b>Traffic lab</b><small>Requests & schema changes</small></span></button>
      <button class:active={screen === "logs"} onclick={() => screen = "logs"}><TerminalSquare/><span><b>Log inspector</b><small>Errors & warnings</small></span>{#if incidents.length}<i>{incidents.length}</i>{/if}</button>
    </nav>
    <div class:live={capturing} class="capture-status"><Circle size={10}/><span><b>{capturing ? "Capture active" : "Ready"}</b><small>{packageName || "Select a package"}</small></span></div>
  </aside>

  <section class:logs-mode={screen === "logs"} class="svelte-main">
    <header class="topbar">
      <div class:running={proxy === "running"} class="connection-pill"><span class="pulse"></span>{statusLabel()}</div>
      <div class="picker">
        <button class="select-trigger" onclick={(event) => { event.stopPropagation(); devicesOpen = !devicesOpen; packagePickerOpen = false; }}><Smartphone/>{selectedDevice()?.model || device || "Select device"}<ChevronDown/></button>
        {#if devicesOpen}<div class="menu device-menu">{#each devices as item}<button class:selected={device === item.serial} onclick={() => chooseDevice(item.serial)}><span><b>{item.model || item.serial}</b><small>{item.connection_type} · {item.authorization_status}</small></span></button>{/each}{#if !devices.length}<span class="menu-empty">No Android devices found</span>{/if}</div>{/if}
      </div>
      <div class="picker package-picker">
        <button class="select-trigger package-trigger" disabled={busy || !device} onclick={(event) => { event.stopPropagation(); packagePickerOpen = !packagePickerOpen; devicesOpen = false; packageSearch = ""; }}><span class="app-dot"></span><span>{packageName || "Select package"}</span><ChevronDown/></button>
        {#if packagePickerOpen}<div class="menu package-menu"><label class="package-filter"><Search size={16}/><input aria-label="Search Android packages" bind:value={packageSearch} placeholder="Search package name" onclick={(event) => event.stopPropagation()} /></label>{#each matchingApps() as app}<button class:selected={packageName === app.package_name} onclick={() => choosePackage(app.package_name)}><span><b>{app.package_name}</b><small>{app.version_name ? `${app.version_name} · ` : ""}{app.debuggable ? "Debug build" : "Installed package"}</small></span>{#if packageName === app.package_name}<ShieldCheck size={15}/>{/if}</button>{/each}{#if packageSearch && !matchingApps().length}<button class="use-package" onclick={() => choosePackage(packageSearch)}><span><b>Use “{packageSearch}”</b><small>Enter this package directly</small></span></button>{:else if !packageSearch && apps.length > 8}<span class="menu-empty">Search {apps.length} packages.</span>{/if}</div>{/if}
      </div>
      {#if capturing}<button class="quiet" onclick={() => void connectCompanion()} disabled={busy}><Wifi/>{busy ? "Opening companion…" : "Open companion"}</button><button class="quiet" onclick={() => paused = !paused}>{#if paused}<Play/>{:else}<Pause/>{/if}{paused ? "Resume" : "Pause"}</button><button class="stop" onclick={() => void stop()} disabled={busy}><Square/>Stop</button>{:else}<button class="primary" onclick={() => void start()} disabled={busy}><Play/>{busy ? "Preparing…" : selectedDevice()?.connection_type === "usb" ? "Open companion" : "Start capture"}</button>{/if}
    </header>

    {#if notice}<div class="notice-bar"><AlertCircle size={16}/><span>{notice}</span><button aria-label="Dismiss notice" onclick={() => notice = ""}><X size={15}/></button></div>{/if}
    {#if screen === "traffic"}
      <section class="hero"><div><span>Traffic lab</span><h1>See what your app is really doing.</h1><p>Capture scoped traffic from <b>{packageName || "your selected package"}</b>.</p></div><div class="hero-host"><small>DESKTOP HOST</small><b>{desktopHost}</b></div></section>
      <section class="toolbar">
        <label class="search-field"><Search size={17}/><input bind:value={query} placeholder="Search requests, hosts, paths…" /></label>
        <button class:active={changedOnly} onclick={() => changedOnly = !changedOnly}>Changed <b>{capturedTransactions.filter(tx => state(tx) === "Changed").length}</b></button>
        <button class:active={errorsOnly} onclick={() => errorsOnly = !errorsOnly}>Errors <b>{capturedTransactions.filter(tx => state(tx) === "Failed").length}</b></button>
        <div class="toolbar-spacer"></div><button class="icon-button destructive" title="Delete all captured traffic and diagnostics" aria-label="Delete all captured traffic and diagnostics" onclick={() => void deleteAll()} disabled={busy}><Trash2/></button><button class="icon-button" title="Export redacted capture" onclick={() => void exportCapture()}><Download/></button><input class="hidden" bind:this={importInput} type="file" accept="application/json,.json" onchange={importCapture}/><button class="icon-button" title="Import capture" onclick={() => importInput?.click()}><FolderUp/></button>
      </section>
      <section class="workbench">
        {#key selectedTransaction}
        <div class="request-list"><div class="list-heading"><span>LIVE REQUESTS</span><small>{visibleTransactions.length} visible</small></div><div class="request-scroll">{#each visibleTransactions as tx (tx.id)}<button class:selected={selectedTransaction?.id === tx.id} class:failed={state(tx) === "Failed"} class:changed={state(tx) === "Changed"} class="request-row" onclick={() => { selectedId = tx.id; tab = "Overview"; }}><time>{new Date(tx.created_at).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" })}</time><b class="method">{tx.request.method}</b><span class="request-target"><b>{tx.request.host}</b><small>{tx.request.path}</small></span><span>{tx.response?.status ?? "…"}</span><span class="request-state">{state(tx)}</span></button>{/each}{#if !visibleTransactions.length}<div class="empty-state"><Activity size={28}/><b>No traffic yet</b><span>{capturing ? "Use the selected app and requests will stream in here." : "Start a capture when you are ready."}</span></div>{/if}</div></div>
        <aside class="inspector">{#if selected()} {@const tx = selected()!}<div class="inspector-heading"><div><span>{tx.request.method}</span><b>{tx.request.host}{tx.request.path}</b></div><button class="icon-button" onclick={() => copy(`${tx.request.scheme}://${tx.request.host}${tx.request.path}`)}><Copy/></button></div><nav class="tabs">{#each ["Overview", "Request", "Response", "Compare", "cURL", "Timeline"] as name}<button class:active={tab === name} onclick={() => tab = name as Tab}>{name}</button>{/each}</nav><div class="detail-panel">{#if tab === "Overview"}<div class="overview-grid"><label>Status<b>{tx.response?.status ?? "Pending"}</b></label><label>Duration<b>{duration(tx) ?? "—"} ms</b></label><label>Content type<b>{tx.response?.content_type || tx.request.content_type || "Unknown"}</b></label><label>Capture quality<b>{tx.capture_quality}</b></label></div>{:else if tab === "Request" || tab === "Response"}{@const message = tab === "Request" ? tx.request : tx.response}<h3>{tab} headers</h3><div class="header-list">{#each message?.headers || [] as header}<div><b>{header.name}</b><span>{header.value}</span></div>{/each}</div><h3>Body</h3><pre>{pretty(body(message?.body))}</pre>{:else if tab === "Compare"}<div class="compare-summary"><div><span>JSON shape comparison</span><b>{tx.comparison ? tx.comparison.compatibility.replaceAll("_", " ") : "Waiting for a comparable response"}</b><small>Scalar values are ignored; only JSON keys, nesting, array item shapes, and types are compared.</small></div><button class="quiet" onclick={() => void approveBaseline(tx)}>Set baseline</button></div>{#if tx.comparison?.differences.length}<div class="difference-list">{#each tx.comparison.differences as difference}<article class:critical={difference.severity === "critical"} class:ignored={difference.ignored}><div><b>{difference.kind.replaceAll("_", " ")}</b><code>{difference.path || "Response"}</code></div><p>{difference.explanation}</p>{#if difference.previous || difference.current}<small><span>Before: {difference.previous || "—"}</span><span>After: {difference.current || "—"}</span></small>{/if}</article>{/each}</div>{:else}<div class="compare-empty"><ShieldCheck/><b>{tx.comparison ? "No JSON-key changes" : "No comparison yet"}</b><span>{tx.comparison ? "This response matches the observed JSON shape." : "Set a baseline or wait for another response to this endpoint."}</span></div>{/if}{:else if tab === "cURL"}<pre>{tx.curl?.multiline || "cURL will be generated once the request is complete."}</pre>{:else}<ol class="timeline"><li>Request started <time>{new Date(tx.timing.request_started_ms).toLocaleTimeString()}</time></li>{#if tx.timing.request_complete_ms}<li>Request sent</li>{/if}{#if tx.timing.response_started_ms}<li>Response headers received</li>{/if}{#if tx.timing.response_complete_ms}<li>Response complete</li>{/if}</ol>{/if}</div>{:else}<div class="empty-state inspector-empty"><ListTree size={30}/><b>Select a request</b><span>Its headers, body and timing will appear here.</span></div>{/if}</aside>
        {/key}
      </section>
    {:else}
      <section class="hero logs-hero"><div><span>Log inspector</span><h1>Every actionable error, in context.</h1><p>Live diagnostic evidence for <b>{packageName || "your selected package"}</b>.</p></div>{#if !capturing}<button class="primary" onclick={() => void start()}><Play/>Start monitoring</button>{/if}</section>
      <section class="log-metrics"><article><small>Detected</small><b>{incidents.length}</b></article><article><small>Errors</small><b class="danger-text">{incidents.filter(item => ["crash", "error", "anr"].includes(item.category)).length}</b></article><article><small>Monitoring</small><b>{capturing ? "Live" : "Paused"}</b></article></section>
      <section class="log-feed">
        {#each incidents as issue (issue.signature)}
          {@const expanded = expandedIssue === issue.signature}
          <article class:expanded class="issue-card">
            <div class="issue-kind"><AlertCircle size={18}/>{issue.category.replaceAll("_", " ")}</div>
            <div class="issue-body">
              <button class="issue-toggle" aria-expanded={expanded} onclick={() => expandedIssue = expanded ? "" : issue.signature}>
                <div class="issue-title"><div><h2>{issue.title}</h2><p>{issue.summary}</p></div><div class="issue-meta">{#if issue.occurrence_count > 1}<b class="occurrences">×{issue.occurrence_count}</b>{/if}<time>{new Date(issue.occurred_at).toLocaleTimeString()}</time><ChevronDown class="issue-chevron"/></div></div>
                <div class="issue-context">{issue.where_occurred || issue.foreground_activity || "App context unavailable"}</div>
              </button>
            </div>
          </article>
          {#if expanded}
            <section class="issue-expanded-content">
              <div class="issue-summary"><div><small>Summary</small><p>{issue.summary}</p></div><div><small>Root cause</small><p>{issue.root_cause || issue.likely_cause || "Still being classified from the captured evidence."}</p></div><div><small>How it happened</small><p>{issue.how_occurred || issue.likely_cause || "No additional execution context was recorded."}</p></div></div>
              <div class="issue-evidence"><div><small>Reproduction / context</small><ol>{#each issue.reproduction_steps as step}<li>{step}</li>{/each}</ol></div><div><small>Captured logs</small><pre>{issue.lines.map(line => `${line.timestamp_ms}  ${line.level.padEnd(5)}  ${line.tag}: ${line.message}`).join("\n") || "No raw Logcat lines were retained for this issue."}</pre></div></div>
              <button class="quiet" onclick={() => copy(issue.lines.map(line => `${line.level} ${line.tag}: ${line.message}`).join("\n"))}><Copy/>Copy logs</button>
            </section>
          {/if}
        {/each}
        {#if !incidents.length}<div class="empty-state log-empty"><ShieldCheck size={32}/><b>No issues detected</b><span>{capturing ? "App Tester is watching Logcat for actionable problems." : "Start monitoring to see errors and warnings here."}</span></div>{/if}
      </section>
    {/if}
  </section>
</main>
