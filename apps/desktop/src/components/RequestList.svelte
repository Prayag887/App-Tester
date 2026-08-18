<script lang="ts">
  import { onDestroy } from "svelte";
  import { Activity, WandSparkles } from "lucide-svelte";
  import { durationMs, elapsedLabel, type TransactionState } from "../lib";
  import {
    getRowStates,
    getCapturedTransactions,
    getSelectedTransaction,
    getVisibleTransactions,
    loadDemoCapture,
    rowTime,
    selectTransaction,
    ui,
  } from "../stores.svelte";
  import type { HttpTransaction } from "../types";

  const ROW_HEIGHT = 58;
  const OVERSCAN_ROWS = 6;
  let scrollTop = $state(0);
  let viewportHeight = $state(580);
  let scrollFrame: number | undefined;
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

  function changeLabel(transaction: HttpTransaction, state: TransactionState | undefined) {
    if (transaction.daily_changes?.count) {
      return `Changed ×${transaction.daily_changes.count}`;
    }
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

  function updateScrollTop(element: HTMLDivElement) {
    if (scrollFrame !== undefined) return;
    scrollFrame = window.requestAnimationFrame(() => {
      scrollTop = element.scrollTop;
      scrollFrame = undefined;
    });
  }

  onDestroy(() => {
    if (scrollFrame !== undefined) window.cancelAnimationFrame(scrollFrame);
  });
</script>

<div class="request-list">
  <div class="request-columns" aria-hidden="true">
    <span>Time</span><span>Method</span><span>Endpoint</span><span>Status</span><span>Duration</span><span>Changes</span>
  </div>
  <div
    class="request-scroll"
    use:observeViewport
    onscroll={(event) => updateScrollTop(event.currentTarget)}
  >
    {#if topSpacerHeight}<div class="request-spacer" style:height={`${topSpacerHeight}px`}></div>{/if}
    {#each renderedTransactions as tx (tx.id)}
      {@const selected = selectedTransaction?.id === tx.id}
      <button
        class:selected
        class:failed={rowStates.get(tx.id) === "Failed"}
        class:changed={rowStates.get(tx.id) === "Changed"}
        class:success={(tx.response?.status ?? 0) >= 200 && (tx.response?.status ?? 0) < 300}
        class="request-row"
        data-method={tx.request.method.toUpperCase()}
        aria-current={selected ? "true" : undefined}
        onclick={() => void selectTransaction(tx.id)}
      >
        <time>{rowTime(tx)}</time>
        <b class="method">{tx.request.method}</b>
        <span class="request-target"><b>{tx.request.path}</b><small>{tx.request.host}</small></span>
        <span class="request-status">{tx.response?.status ?? "…"}</span>
        <span class="request-duration">{durationLabel(tx)}</span>
        <span class="request-state">{changeLabel(tx, rowStates.get(tx.id))}</span>
      </button>
    {/each}
    {#if bottomSpacerHeight}<div class="request-spacer" style:height={`${bottomSpacerHeight}px`}></div>{/if}
    {#if !visibleTransactions.length}
      <div class:filtered={hasCapturedTransactions} class="empty-state">
        <Activity size={28} />
        <b>{hasCapturedTransactions ? "No matching requests" : "No traffic yet"}</b>
        <span>{hasCapturedTransactions ? "Adjust the search or active filters." : capturing ? "Use the selected app and requests will stream in here." : "Connect Android later, or explore a realistic capture now."}</span>
        {#if !hasCapturedTransactions && !capturing}
          <button class="demo-capture" onclick={loadDemoCapture}><WandSparkles size={15}/> Load demo traffic</button>
        {/if}
      </div>
    {/if}
  </div>
</div>

<style>
  .request-columns { font-size: 10px; }
  .request-row { font-size: 12px; }
  .request-target small { font-size: 10.5px; }
  .request-columns span:nth-child(5),.request-columns span:nth-child(6),.request-duration,.request-state { text-align: center; }
  .demo-capture {
    height: 36px;
    margin-top: 5px;
    padding: 0 13px;
    color: var(--shell-text);
    border-color: var(--shell-accent);
    background: color-mix(in srgb, var(--shell-accent) 14%, var(--shell-panel-raised));
    box-shadow: 0 8px 22px color-mix(in srgb, var(--shell-accent) 16%, transparent);
  }
  .demo-capture :global(svg) { color: var(--shell-accent); }
</style>
