<script lang="ts">
  import { Download, FolderUp, Play, Search, Smartphone, Trash2 } from "lucide-svelte";
  let importInput: HTMLInputElement | undefined;
  import {
    exportCapture,
    getChangedCount,
    getFailedCount,
    getSelectedTransaction,
    importCapture,
    requestDeleteAll,
    setMirrorOpen,
    start,
    ui,
  } from "../stores.svelte";
  const changedCount = $derived(getChangedCount());
  const failedCount = $derived(getFailedCount());
  const selectedTransaction = $derived(getSelectedTransaction());
  const mirrorOpen = $derived(ui.mirrorOpen);
  const confirmDeleteAll = $derived(ui.confirmDeleteAll);
  const busy = $derived(ui.busy);
  const capturing = $derived(ui.capturing);
  const changedOnly = $derived(ui.changedOnly);
  const desktopHost = $derived(ui.desktopHost);
  const errorsOnly = $derived(ui.errorsOnly);
  const packageName = $derived(ui.packageName);
  const query = $derived(ui.query);
  const setChangedOnly = (next: boolean) => ui.changedOnly = next;
  const setErrorsOnly = (next: boolean) => ui.errorsOnly = next;
  const setQuery = (next: string) => ui.query = next;
  import RequestList from "./RequestList.svelte";
  import Inspector from "./Inspector.svelte";
  import MirrorPanel from "./MirrorPanel.svelte";
</script>

<section class="hero"><div><span>Traffic lab</span><h1>See what your app is really doing.</h1><p>Capture scoped traffic from <b>{packageName || "your selected package"}</b>.</p></div><div class="hero-host"><small>DESKTOP HOST</small><b>{desktopHost}</b></div></section>
<section class="toolbar">
  <label class="search-field"><Search size={17}/><input value={query} oninput={(event) => setQuery((event.target as HTMLInputElement).value)} placeholder="Search requests, hosts, paths…" /></label>
  <button class:active={changedOnly} onclick={() => setChangedOnly(!changedOnly)}>Changed <b>{changedCount}</b></button>
  <button class:active={errorsOnly} onclick={() => setErrorsOnly(!errorsOnly)}>Errors <b>{failedCount}</b></button>
  <div class="toolbar-spacer"></div><button class:active={mirrorOpen} title="Mirror the device screen" onclick={() => setMirrorOpen(!mirrorOpen)}><Smartphone/> Mirror</button><button class:confirming={confirmDeleteAll} class="icon-button destructive" title="Delete all captured traffic and diagnostics" aria-label="Delete all captured traffic and diagnostics" onclick={() => requestDeleteAll()} disabled={busy}>{#if confirmDeleteAll}<b>Confirm?</b>{:else}<Trash2/>{/if}</button><button class="icon-button" title="Export redacted capture" onclick={() => void exportCapture()}><Download/></button><input class="hidden" bind:this={importInput} type="file" accept="application/json,.json" onchange={importCapture}/><button class="icon-button" title="Import capture" onclick={() => importInput?.click()}><FolderUp/></button>
</section>
<section class="workbench" class:with-mirror={mirrorOpen}>
  <RequestList />
  <Inspector />
  {#if mirrorOpen}<MirrorPanel />{/if}
</section>
