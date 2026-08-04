<script lang="ts">
  import { ChevronDown, Pause, Play, Search, ShieldCheck, Smartphone, Square, Wifi } from "lucide-svelte";
  import {
    chooseDevice,
    choosePackage,
    connectCompanion,
    getMatchingApps,
    getSelectedDevice,
    getStatusLabel,
    start,
    stop,
    ui,
  } from "../stores.svelte";
  const matchingApps = $derived(getMatchingApps());
  const selectedDevice = $derived(getSelectedDevice());
  const statusLabel = $derived(getStatusLabel());
  const apps = $derived(ui.apps);
  const busy = $derived(ui.busy);
  const capturing = $derived(ui.capturing);
  const devices = $derived(ui.devices);
  const device = $derived(ui.device);
  const devicesOpen = $derived(ui.devicesOpen);
  const packageName = $derived(ui.packageName);
  const packagePickerOpen = $derived(ui.packagePickerOpen);
  const packageSearch = $derived(ui.packageSearch);
  const paused = $derived(ui.paused);
  const proxy = $derived(ui.proxy);
  const setDevicesOpen = (next: boolean) => ui.devicesOpen = next;
  const setPackagePickerOpen = (next: boolean) => ui.packagePickerOpen = next;
  const setPackageSearch = (next: string) => ui.packageSearch = next;
  const setPaused = (next: boolean) => ui.paused = next;
</script>

<header class="topbar">
  <div class:running={proxy === "running"} class="connection-pill"><span class="pulse"></span>{statusLabel}</div>
  <div class="picker">
    <button class="select-trigger" onclick={(event) => { event.stopPropagation(); setDevicesOpen(!devicesOpen); setPackagePickerOpen(false); }}><Smartphone/>{selectedDevice?.model || device || "Select device"}<ChevronDown/></button>
    {#if devicesOpen}<div class="menu device-menu">{#each devices as item}<button class:selected={device === item.serial} onclick={() => chooseDevice(item.serial)}><span><b>{item.model || item.serial}</b><small>{item.connection_type} · {item.authorization_status}</small></span></button>{/each}{#if !devices.length}<span class="menu-empty">No Android devices found</span>{/if}</div>{/if}
  </div>
  <div class="picker package-picker">
    <button class="select-trigger package-trigger" disabled={busy || !device} onclick={(event) => { event.stopPropagation(); setPackagePickerOpen(!packagePickerOpen); setDevicesOpen(false); setPackageSearch(""); }}><span class="app-dot"></span><span>{packageName || "Select package"}</span><ChevronDown/></button>
    {#if packagePickerOpen}<div class="menu package-menu"><label class="package-filter"><Search size={16}/><input aria-label="Search Android packages" value={packageSearch} oninput={(event) => setPackageSearch((event.target as HTMLInputElement).value)} placeholder="Search package name" onclick={(event) => event.stopPropagation()} /></label>{#each matchingApps as app}<button class:selected={packageName === app.package_name} onclick={() => choosePackage(app.package_name)}><span><b>{app.package_name}</b><small>{app.version_name ? `${app.version_name} · ` : ""}{app.debuggable ? "Debug build" : "Installed package"}</small></span>{#if packageName === app.package_name}<ShieldCheck size={15}/>{/if}</button>{/each}{#if packageSearch && !matchingApps.length}<button class="use-package" onclick={() => choosePackage(packageSearch)}><span><b>Use “{packageSearch}”</b><small>Enter this package directly</small></span></button>{:else if !packageSearch && apps.length > 8}<span class="menu-empty">Search {apps.length} packages.</span>{/if}</div>{/if}
  </div>
  {#if capturing}<button class="quiet" onclick={() => void connectCompanion()} disabled={busy}><Wifi/>{busy ? "Opening companion…" : "Open companion"}</button><button class="quiet" onclick={() => setPaused(!paused)}>{#if paused}<Play/>{:else}<Pause/>{/if}{paused ? "Resume" : "Pause"}</button><button class="stop" onclick={() => void stop()} disabled={busy}><Square/>Stop</button>{:else}<button class="primary" onclick={() => void start()} disabled={busy}><Play/>{busy ? "Preparing…" : selectedDevice?.connection_type === "usb" ? "Open companion" : "Start capture"}</button>{/if}
</header>
