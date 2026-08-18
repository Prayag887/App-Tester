<script lang="ts">
  import { onDestroy } from "svelte";
  import { Check, Copy, Download, FolderUp, Search, Send, Smartphone, Trash2 } from "lucide-svelte";
  let importInput: HTMLInputElement | undefined;
  let copyTimer: number | undefined;
  let copied = $state(false);
  import {
    copySelectedCurl,
    exportCapture,
    getCapturedTransactions,
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
  let requestListWidth = $state(readPanelSize("app-tester.traffic-list-width", 720));
  const changedCount = $derived(getChangedCount());
  const failedCount = $derived(getFailedCount());
  const requestCount = $derived(getCapturedTransactions().length);
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
  const showAll = () => {
    setChangedOnly(false);
    setErrorsOnly(false);
  };
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

<section class="hero"><div><span>Traffic lab <i>›</i></span><h1>See what your app is really doing.</h1><p>Capture scoped traffic from your selected package.</p></div><div class="hero-host"><small>Desktop route</small><b>{desktopHost}</b></div></section>
<section class="toolbar" aria-label="Traffic controls">
  <label class="search-field"><Search size={17}/><input value={query} oninput={(event) => setQuery((event.target as HTMLInputElement).value)} placeholder="Search requests, hosts, paths…" /></label>
  <div class="filter-group">
    <button class:active={!changedOnly && !errorsOnly} class="filter-button" aria-pressed={!changedOnly && !errorsOnly} onclick={showAll}><span class:visible={!changedOnly && !errorsOnly} class="filter-check" aria-hidden="true"><Check size={12}/></span><span>All</span> <b>{requestCount}</b></button>
    <button class:active={changedOnly} class="filter-button" aria-pressed={changedOnly} onclick={() => { setChangedOnly(!changedOnly); setErrorsOnly(false); }}><span class:visible={changedOnly} class="filter-check" aria-hidden="true"><Check size={12}/></span><span>Changed</span> <b>{changedCount}</b></button>
    <button class:active={errorsOnly} class="filter-button" aria-pressed={errorsOnly} onclick={() => { setErrorsOnly(!errorsOnly); setChangedOnly(false); }}><span class:visible={errorsOnly} class="filter-check" aria-hidden="true"><Check size={12}/></span><span>Errors</span> <b>{failedCount}</b></button>
  </div>
  <div class="toolbar-spacer"></div><button class:active={mirrorOpen} class="action-mirror" aria-pressed={mirrorOpen} title="Mirror the device screen" onclick={() => setMirrorOpen(!mirrorOpen)}><Smartphone/> Mirror</button><button class="action-send icon-button" title="Send the selected request in the composer" aria-label="Send the selected request in the composer" onclick={() => openInComposer()} disabled={!selectedTransaction}><Send/></button><button class:copied class="action-copy icon-button" title={copied ? "cURL copied" : "Copy selected cURL"} aria-label={copied ? "cURL copied" : "Copy selected cURL"} onclick={copyCurl} disabled={!selectedTransaction?.curl?.multiline && !selectedTransaction?.curl?.compact}>{#if copied}<Check/>{:else}<Copy/>{/if}</button><button class:confirming={confirmDeleteAll} class="action-delete icon-button destructive" title={confirmDeleteAll ? "Click again to delete all captured traffic and diagnostics" : "Delete all captured traffic and diagnostics"} aria-label={confirmDeleteAll ? "Confirm deletion of all captured traffic and diagnostics" : "Delete all captured traffic and diagnostics"} onclick={() => requestDeleteAll()} disabled={busy}><span class:hidden={confirmDeleteAll} class="delete-icon" aria-hidden="true"><Trash2/></span><b class:visible={confirmDeleteAll} class="delete-confirm" aria-hidden={!confirmDeleteAll}>Delete all?</b></button><button class="action-export icon-button" aria-label="Export redacted capture" title="Export redacted capture" onclick={() => void exportCapture()}><Download/></button><input class="hidden" bind:this={importInput} type="file" accept="application/json,.json" onchange={importCapture}/><button class="action-import icon-button" aria-label="Import capture" title="Import capture" onclick={() => importInput?.click()}><FolderUp/></button>
</section>

<style>
  .filter-button {
    display: flex;
    align-items: center;
    justify-content: center;
    transition: color 180ms ease, background-color 220ms ease, box-shadow 220ms ease;
  }
  .filter-check {
    display: inline-grid;
    width: 0;
    margin-right: 0;
    place-items: center;
    overflow: hidden;
    opacity: 0;
    transform: scale(.55) rotate(-12deg);
    transition: width 240ms var(--ease-out), margin-right 240ms var(--ease-out), opacity 160ms ease, transform 260ms var(--ease-out);
  }
  .filter-check.visible {
    width: 12px;
    margin-right: 2px;
    opacity: 1;
    transform: scale(1) rotate(0);
  }
  .filter-check :global(svg) { color: #9ee4c8; }

  .action-delete {
    position: relative;
    isolation: isolate;
    display: flex !important;
    width: 38px;
    min-width: 38px;
    align-items: center;
    justify-content: center;
    gap: 0;
    padding: 0 !important;
    overflow: hidden;
    white-space: nowrap;
    transform-origin: center;
    transition: width 500ms linear(0,-.04 12%,.14 28%,.78 58%,1.06 78%,.98 90%,1), min-width 500ms linear(0,-.04 12%,.14 28%,.78 58%,1.06 78%,.98 90%,1), color 180ms ease, border-color 180ms ease, background-color 180ms ease, box-shadow 320ms ease;
  }
  .action-delete::after {
    position: absolute;
    z-index: -1;
    inset: 0;
    content: "";
    background: linear-gradient(105deg, transparent 22%, color-mix(in srgb, currentColor 28%, transparent) 50%, transparent 78%);
    opacity: 0;
    transform: translateX(-135%);
    pointer-events: none;
  }
  .action-delete.confirming::after {
    animation: delete-sheen 560ms 90ms var(--ease-out) both;
  }
  .delete-icon {
    display: grid;
    width: 18px;
    place-items: center;
    opacity: 1;
    transform: scale(1) rotate(0);
    transition: width 260ms var(--ease-out), opacity 130ms ease, transform 260ms var(--ease-out);
  }
  .delete-confirm {
    display: block;
    max-width: 0;
    margin: 0;
    overflow: hidden;
    opacity: 0;
    transform: translateX(7px) scale(.96);
    transition: max-width 380ms cubic-bezier(.16,1,.3,1), opacity 190ms ease 150ms, transform 380ms cubic-bezier(.16,1,.3,1);
  }
  .action-delete.confirming {
    width: 108px;
    min-width: 108px;
    padding: 0 10px;
  }
  .delete-icon.hidden {
    width: 0;
    opacity: 0;
    transform: scale(.55) rotate(-12deg);
  }
  .delete-confirm.visible {
    max-width: 78px;
    opacity: 1;
    transform: translateX(0) scale(1);
  }
  @keyframes delete-sheen {
    0% { opacity: 0; transform: translateX(-135%); }
    28% { opacity: .65; }
    100% { opacity: 0; transform: translateX(135%); }
  }
</style>
<section class:inspector-open={Boolean(ui.selectedId)} class="workbench" class:with-mirror={mirrorOpen} bind:this={workbench} style:--traffic-list-width={`${requestListWidth}px`}>
  <RequestList />
  {#if ui.selectedId}
    <PanelResizeHandle label="Resize request list and inspector" onpointerdown={resizeTrafficPanels} onkeydown={resizeTrafficPanelsWithKeyboard} />
    <Inspector />
  {/if}
  {#if mirrorOpen}
    {#await import("./MirrorPanel.svelte") then module}
      {@const MirrorPanel = module.default}
      <MirrorPanel />
    {/await}
  {/if}
</section>
