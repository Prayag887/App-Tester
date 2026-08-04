<script lang="ts">
  import { Activity } from "lucide-svelte";
  import { getRowStates, getSelectedTransaction, getVisibleTransactions, rowTime, ui } from "../stores.svelte";
  // Rendering every visible row keeps the DOM (and the WebView's memory)
  // proportional to the whole capture. The list is newest-first, so capping
  // the rendered rows keeps the live capture view intact while bounding the
  // cost of very long sessions.
  const MAX_RENDERED_ROWS = 1000;
  const visibleTransactions = $derived(getVisibleTransactions());
  const renderedTransactions = $derived(visibleTransactions.slice(0, MAX_RENDERED_ROWS));
  const selectedTransaction = $derived(getSelectedTransaction());
  const rowStates = $derived(getRowStates());
  const capturing = $derived(ui.capturing);
  const selectedId = $derived(ui.selectedId);
  const setSelectedId = (id: string) => ui.selectedId = id;
  const setTab = (tab: "Overview") => ui.tab = tab;
  const countLabel = $derived(
    visibleTransactions.length > MAX_RENDERED_ROWS
      ? `${MAX_RENDERED_ROWS} of ${visibleTransactions.length} shown`
      : `${visibleTransactions.length} visible`,
  );
</script>

<div class="request-list"><div class="list-heading"><span>LIVE REQUESTS</span><small>{countLabel}</small></div><div class="request-scroll">{#each renderedTransactions as tx (tx.id)}<button class:selected={selectedTransaction?.id === tx.id} class:failed={rowStates.get(tx.id) === "Failed"} class:changed={rowStates.get(tx.id) === "Changed"} class="request-row" onclick={() => { setSelectedId(tx.id); setTab("Overview"); }}><time>{rowTime(tx)}</time><b class="method">{tx.request.method}</b><span class="request-target"><b>{tx.request.host}</b><small>{tx.request.path}</small></span><span>{tx.response?.status ?? "…"}</span><span class="request-state">{rowStates.get(tx.id)}</span></button>{/each}{#if !visibleTransactions.length}<div class="empty-state"><Activity size={28}/><b>No traffic yet</b><span>{capturing ? "Use the selected app and requests will stream in here." : "Start a capture when you are ready."}</span></div>{/if}</div></div>
