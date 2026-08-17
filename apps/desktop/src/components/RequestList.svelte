<script lang="ts">
  import { Activity } from "lucide-svelte";
  import { durationMs, elapsedLabel, type TransactionState } from "../lib";
  import {
    getRowStates,
    getCapturedTransactions,
    getSelectedTransaction,
    getVisibleTransactions,
    rowTime,
    selectTransaction,
    ui,
  } from "../stores.svelte";
  import type { HttpTransaction } from "../types";

  const ROW_HEIGHT = 58;
  const OVERSCAN_ROWS = 6;
  let scrollTop = $state(0);
  let viewportHeight = $state(580);
  const visibleTransactions = $derived(getVisibleTransactions());
  const startIndex = $derived(
    Math.min(
      Math.max(0, visibleTransactions.length - 1),
      Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN_ROWS),
    ),
  );
  const endIndex = $derived(
    Math.min(
      visibleTransactions.length,
      startIndex + Math.ceil(viewportHeight / ROW_HEIGHT) + OVERSCAN_ROWS * 2,
    ),
  );
  const renderedTransactions = $derived(visibleTransactions.slice(startIndex, endIndex));
  const topSpacerHeight = $derived(startIndex * ROW_HEIGHT);
  const bottomSpacerHeight = $derived(
    Math.max(0, (visibleTransactions.length - endIndex) * ROW_HEIGHT),
  );
  const selectedTransaction = $derived(getSelectedTransaction());
  const rowStates = $derived(getRowStates());
  const capturing = $derived(ui.capturing);
  const hasCapturedTransactions = $derived(getCapturedTransactions().length > 0);

  function durationLabel(transaction: HttpTransaction) {
    const duration = durationMs(transaction);
    return duration == null ? "—" : elapsedLabel(duration);
  }

  function changeLabel(state: TransactionState | undefined) {
    if (state === "Changed") return "Changed";
    if (state === "Failed") return "Error";
    return "—";
  }

  function observeViewport(scrollElement: HTMLDivElement) {
    const updateViewport = () =>
      (viewportHeight = scrollElement.clientHeight || viewportHeight);
    updateViewport();
    if (typeof ResizeObserver === "undefined") return {};
    const observer = new ResizeObserver(updateViewport);
    observer.observe(scrollElement);
    return { destroy: () => observer.disconnect() };
  }
</script>

<div class="request-list">
  <div class="request-columns" aria-hidden="true">
    <span>Time</span><span>Method</span><span>Endpoint</span><span>Status</span><span>Duration</span><span>Changes</span>
  </div>
  <div
    class="request-scroll"
    use:observeViewport
    onscroll={(event) => (scrollTop = event.currentTarget.scrollTop)}
  >
    {#if topSpacerHeight}<div class="request-spacer" style:height={`${topSpacerHeight}px`}></div>{/if}
    {#each renderedTransactions as tx (tx.id)}
      {@const selected = selectedTransaction?.id === tx.id}
      <button
        class:selected
        class:failed={rowStates.get(tx.id) === "Failed"}
        class:changed={rowStates.get(tx.id) === "Changed"}
        class="request-row"
        aria-current={selected ? "true" : undefined}
        onclick={() => void selectTransaction(tx.id)}
      >
        <time>{rowTime(tx)}</time>
        <b class="method">{tx.request.method}</b>
        <span class="request-target"><b>{tx.request.path}</b><small>{tx.request.host}</small></span>
        <span class="request-status">{tx.response?.status ?? "…"}</span>
        <span class="request-duration">{durationLabel(tx)}</span>
        <span class="request-state">{changeLabel(rowStates.get(tx.id))}</span>
      </button>
    {/each}
    {#if bottomSpacerHeight}<div class="request-spacer" style:height={`${bottomSpacerHeight}px`}></div>{/if}
    {#if !visibleTransactions.length}
      <div class:filtered={hasCapturedTransactions} class="empty-state">
        <Activity size={28} />
        <b>{hasCapturedTransactions ? "No matching requests" : "No traffic yet"}</b>
        <span>{hasCapturedTransactions ? "Adjust the search or active filters." : capturing ? "Use the selected app and requests will stream in here." : "Start a capture when you are ready."}</span>
      </div>
    {/if}
  </div>
</div>
