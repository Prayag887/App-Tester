<script lang="ts">
  import { onMount } from "svelte";
  import { listen } from "@tauri-apps/api/event";
  import Sidenav from "./components/Sidenav.svelte";
  import Topbar from "./components/Topbar.svelte";
  import NoticeBar from "./components/NoticeBar.svelte";
  import TrafficView from "./components/TrafficView.svelte";
  import ComposerView from "./components/ComposerView.svelte";
  import LogsView from "./components/LogsView.svelte";
  import {
    closePickers,
    refreshDevices,
    refreshProxyStatus,
    refreshTransactions,
    ui,
    upsertIncident,
    upsertTransaction,
  } from "./stores.svelte";
  const screen = $derived(ui.screen);
  import type { HttpTransaction, LogIncident, ProxyStatus } from "./types";

  type InspectorEvent<T> = { kind: string; payload: T };

  onMount(() => {
    void refreshProxyStatus();
    void refreshDevices();
    // Traffic and proxy state arrive as live push events from the native
    // bridge; no polling is needed for them. Only device discovery is
    // inherently poll-based. The poll backs off while the window is
    // backgrounded (devices cannot change while the user is not looking,
    // and the focus handler below repairs any gap on return).
    const deviceTimer = window.setInterval(() => {
      if (!document.hidden) void refreshDevices();
    }, 2000);
    // The WebView can miss push events while suspended or restored; a single
    // reconcile on focus repairs any gap without constant polling.
    const onFocus = () => {
      void refreshProxyStatus();
      void refreshTransactions();
    };
    window.addEventListener("focus", onFocus);
    const unlisteners = [
      listen<InspectorEvent<ProxyStatus>>("proxy-status-changed", event => ui.proxy = event.payload.payload),
      listen<InspectorEvent<HttpTransaction>>("transaction-created", event => upsertTransaction(event.payload.payload)),
      listen<InspectorEvent<HttpTransaction>>("transaction-updated", event => upsertTransaction(event.payload.payload)),
      listen<InspectorEvent<HttpTransaction>>("transaction-completed", event => upsertTransaction(event.payload.payload)),
      listen<InspectorEvent<LogIncident>>("incident-created", event => upsertIncident(event.payload.payload))
    ];
    return () => {
      window.clearInterval(deviceTimer);
      window.removeEventListener("focus", onFocus);
      void Promise.all(unlisteners).then(items => items.forEach(stop => stop()));
    };
  });
</script>

<svelte:window onclick={closePickers} />
<main class="svelte-app">
  <Sidenav />
  <section class:logs-mode={screen === "logs"} class="svelte-main">
    <Topbar />
    <NoticeBar />
    {#if screen === "traffic"}
      <TrafficView />
    {:else if screen === "composer"}
      <ComposerView />
    {:else}
      <LogsView />
    {/if}
  </section>
</main>
