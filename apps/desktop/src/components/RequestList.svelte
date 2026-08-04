<script lang="ts">
  import { Activity } from "lucide-svelte";
  import { getRowStates, getSelectedTransaction, getVisibleTransactions, rowTime, ui } from "../stores.svelte";
  const visibleTransactions = $derived(getVisibleTransactions());
  const selectedTransaction = $derived(getSelectedTransaction());
  const rowStates = $derived(getRowStates());
  const capturing = $derived(ui.capturing);
  const selectedId = $derived(ui.selectedId);
  const setSelectedId = (id: string) => ui.selectedId = id;
  const setTab = (tab: "Overview") => ui.tab = tab;
</script>

<div class="request-list"><div class="list-heading"><span>LIVE REQUESTS</span><small>{visibleTransactions.length} visible</small></div><div class="request-scroll">{#each visibleTransactions as tx (tx.id)}<button class:selected={selectedTransaction?.id === tx.id} class:failed={rowStates.get(tx.id) === "Failed"} class:changed={rowStates.get(tx.id) === "Changed"} class="request-row" onclick={() => { setSelectedId(tx.id); setTab("Overview"); }}><time>{rowTime(tx)}</time><b class="method">{tx.request.method}</b><span class="request-target"><b>{tx.request.host}</b><small>{tx.request.path}</small></span><span>{tx.response?.status ?? "…"}</span><span class="request-state">{rowStates.get(tx.id)}</span></button>{/each}{#if !visibleTransactions.length}<div class="empty-state"><Activity size={28}/><b>No traffic yet</b><span>{capturing ? "Use the selected app and requests will stream in here." : "Start a capture when you are ready."}</span></div>{/if}</div></div>
