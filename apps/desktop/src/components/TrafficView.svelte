<script lang="ts">
  import { onDestroy } from "svelte";
  import { Check, Copy, Download, FolderUp, Search, Send, Smartphone, Trash2 } from "lucide-svelte";
  let importInput: HTMLInputElement | undefined;
  let copyTimer: number | undefined;
  let copied = $state(false);
  import {
    copySelectedCurl,
    exportCapture,
    getChangedCount,
    getFailedCount,
    getSelectedTransaction,
    importCapture,
    requestDeleteAll,
    setMirrorOpen,
    ui,
  } from "../stores.svelte";
  import { manualRequestFromTransaction } from "../lib";
  import {
    beginHorizontalResize,
    clampPanelSize,
    readPanelSize,
    storePanelSize,
  } from "../panel-resize";
  let workbench: HTMLElement;
  let requestListWidth = $state(readPanelSize("app-tester.traffic-list-width", 640));
  const changedCount = $derived(getChangedCount());
  const failedCount = $derived(getFailedCount());
  const selectedTransaction = $derived(getSelectedTransaction());

  /// Hands the selected captured request to the composer for editing or
  /// re-sending — the round trip from capture to manual request.
  function openInComposer() {
    const transaction = selectedTransaction;
    if (!transaction) return;
    ui.composerDraft = manualRequestFromTransaction(transaction);
    ui.screen = "composer";
    ui.notice = "Opened in the composer — review, then send.";
  }
  const mirrorOpen = $derived(ui.mirrorOpen);
  const confirmDeleteAll = $derived(ui.confirmDeleteAll);
  const busy = $derived(ui.busy);
  const changedOnly = $derived(ui.changedOnly);
  const desktopHost = $derived(ui.desktopHost);
  const errorsOnly = $derived(ui.errorsOnly);
  const packageName = $derived(ui.packageName);
  const query = $derived(ui.query);
  const setChangedOnly = (next: boolean) => ui.changedOnly = next;
  const setErrorsOnly = (next: boolean) => ui.errorsOnly = next;
  const setQuery = (next: string) => ui.query = next;
  const copyCurl = () => {
    copySelectedCurl();
    copied = false;
    window.requestAnimationFrame(() => copied = true);
    if (copyTimer !== undefined) window.clearTimeout(copyTimer);
    copyTimer = window.setTimeout(() => copied = false, 1200);
  };
  onDestroy(() => {
    if (copyTimer !== undefined) window.clearTimeout(copyTimer);
  });
  import RequestList from "./RequestList.svelte";
  import Inspector from "./Inspector.svelte";
  import MirrorPanel from "./MirrorPanel.svelte";
  import PanelResizeHandle from "./PanelResizeHandle.svelte";

  function resizeTrafficPanels(event: PointerEvent) {
    const bounds = workbench.getBoundingClientRect();
    const inspectorMinimum = 350;
    beginHorizontalResize(
      event,
      clientX => clampPanelSize(clientX - bounds.left, 450, Math.max(450, bounds.width - inspectorMinimum)),
      value => requestListWidth = value,
      value => storePanelSize("app-tester.traffic-list-width", value),
    );
  }

  function resizeTrafficPanelsWithKeyboard(event: KeyboardEvent) {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
    event.preventDefault();
    requestListWidth = clampPanelSize(requestListWidth + (event.key === "ArrowRight" ? 16 : -16), 450, 820);
    storePanelSize("app-tester.traffic-list-width", requestListWidth);
  }
</script>

<section class="hero"><div><span>Traffic lab</span><h1>See what your app is really doing.</h1><p>Capture scoped traffic from <b>{packageName || "your selected package"}</b>.</p></div><div class="hero-host"><small>DESKTOP HOST</small><b>{desktopHost}</b></div></section>
<section class="toolbar" aria-label="Traffic controls">
  <label class="search-field"><Search size={17}/><input value={query} oninput={(event) => setQuery((event.target as HTMLInputElement).value)} placeholder="Search requests, hosts, paths…" /></label>
  <button class:active={changedOnly} class="filter-button" aria-pressed={changedOnly} onclick={() => setChangedOnly(!changedOnly)}>{#if changedOnly}<Check size={12}/>{/if}Changed <b>{changedCount}</b></button>
  <button class:active={errorsOnly} class="filter-button" aria-pressed={errorsOnly} onclick={() => setErrorsOnly(!errorsOnly)}>{#if errorsOnly}<Check size={12}/>{/if}Errors <b>{failedCount}</b></button>
  <div class="toolbar-spacer"></div><button class:active={mirrorOpen} class="action-mirror" aria-pressed={mirrorOpen} title="Mirror the device screen" onclick={() => setMirrorOpen(!mirrorOpen)}><Smartphone/> Mirror</button><button class="action-send icon-button" title="Send the selected request in the composer" aria-label="Send the selected request in the composer" onclick={() => openInComposer()} disabled={!selectedTransaction}><Send/></button><button class:copied class="action-copy icon-button" title={copied ? "cURL copied" : "Copy selected cURL"} aria-label={copied ? "cURL copied" : "Copy selected cURL"} onclick={copyCurl} disabled={!selectedTransaction?.curl?.multiline && !selectedTransaction?.curl?.compact}>{#if copied}<Check/>{:else}<Copy/>{/if}</button><button class:confirming={confirmDeleteAll} class="action-delete icon-button destructive" title="Delete all captured traffic and diagnostics" aria-label="Delete all captured traffic and diagnostics" onclick={() => requestDeleteAll()} disabled={busy}>{#if confirmDeleteAll}<b>Confirm?</b>{:else}<Trash2/>{/if}</button><button class="action-export icon-button" aria-label="Export redacted capture" title="Export redacted capture" onclick={() => void exportCapture()}><Download/></button><input class="hidden" bind:this={importInput} type="file" accept="application/json,.json" onchange={importCapture}/><button class="action-import icon-button" aria-label="Import capture" title="Import capture" onclick={() => importInput?.click()}><FolderUp/></button>
</section>
<section class="workbench" class:with-mirror={mirrorOpen} bind:this={workbench} style:--traffic-list-width={`${requestListWidth}px`}>
  <RequestList />
  <PanelResizeHandle label="Resize request list and inspector" onpointerdown={resizeTrafficPanels} onkeydown={resizeTrafficPanelsWithKeyboard} />
  <Inspector />
  {#if mirrorOpen}<MirrorPanel />{/if}
</section>
