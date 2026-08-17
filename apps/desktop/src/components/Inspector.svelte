<script lang="ts">
  import { ListTree, ShieldCheck } from "lucide-svelte";
  import {
    bodyTextPreview,
    byteSizeLabel,
    curlCommand,
    prettyJson,
    textPreview,
  } from "../lib";
  import { approveBaseline, getSelectedTransaction, ui } from "../stores.svelte";
  import type { Tab } from "../lib";

  const selectedTransaction = $derived(getSelectedTransaction());
  const tab = $derived(ui.tab);
  let tabDirection = $state<"forward" | "backward">("forward");

  function setTab(next: Tab) {
    if (next === tab) return;
    tabDirection = TABS.indexOf(next) > TABS.indexOf(tab) ? "forward" : "backward";
    ui.tab = next;
  }

  const TABS: Tab[] = ["Request", "Response", "Compare", "cURL", "Timeline"];
</script>

<svelte:boundary>
  <aside class="inspector">
    {#if selectedTransaction}
      {@const tx = selectedTransaction}
      {#key tx.id}
        <div class="inspector-heading">
          <div><span>{tx.request.method}</span><b>{tx.request.host}{tx.request.path}</b></div>
          <strong class:failed={(tx.response?.status ?? 0) >= 400}>{tx.response?.status ?? "Pending"}</strong>
        </div>
      {/key}
      {#if ui.detailLoading}<div class="inspector-progress" aria-label="Loading request details"><span></span></div>{/if}
      <div class="tabs" role="tablist" aria-label="Request details">
        {#each TABS as name (name)}
          <button role="tab" aria-selected={tab === name} class:active={tab === name} onclick={() => setTab(name)}>{name}</button>
        {/each}
      </div>
      {#key `${tx.id}:${tab}`}
      <div class:backward={tabDirection === "backward"} class="detail-panel inspector-tab-content" role="tabpanel">
        {#if tab === "Request" || tab === "Response"}
          {@const message = tab === "Request" ? tx.request : tx.response}
          {@const preview = bodyTextPreview(message?.body)}
          <h3>{tab} headers</h3>
          <div class="header-list">
            {#each message?.headers || [] as header, index (`${index}:${header.name}:${header.value}`)}
              <div><b>{header.name}</b><span>{header.value}</span></div>
            {/each}
          </div>
          <h3>Body</h3>
          <pre>{preview.truncated ? preview.text : prettyJson(preview.text || "No body")}</pre>
          {#if preview.truncated}
            <div class="content-truncated">
              Showing {byteSizeLabel(preview.shown)} of {byteSizeLabel(preview.total)}. The full body remains in the capture.
            </div>
          {/if}
        {:else if tab === "Compare"}
          <div class="compare-summary">
            <div>
              <span>JSON shape comparison</span>
              <b>{tx.comparison ? tx.comparison.compatibility.replaceAll("_", " ") : "Waiting for a comparable response"}</b>
              <small>Scalar values are ignored; only JSON keys, nesting, array item shapes, and types are compared.</small>
            </div>
            <button class="quiet" onclick={() => void approveBaseline(tx)}>Set baseline</button>
          </div>
          {#if tx.comparison?.differences.length}
            <div class="difference-list">
              {#each tx.comparison.differences as difference, index (`${index}:${difference.kind}:${difference.path}:${difference.explanation}`)}
                <article class:critical={difference.severity === "critical"} class:ignored={difference.ignored}>
                  <div><b>{difference.kind.replaceAll("_", " ")}</b><code>{difference.path || "Response"}</code></div>
                  <p>{difference.explanation}</p>
                  {#if difference.previous || difference.current}
                    <small><span>Before: {difference.previous || "—"}</span><span>After: {difference.current || "—"}</span></small>
                  {/if}
                </article>
              {/each}
            </div>
          {:else}
            <div class="compare-empty">
              <ShieldCheck />
              <b>{tx.comparison ? "No JSON-key changes" : "No comparison yet"}</b>
              <span>{tx.comparison ? "This response matches the observed JSON shape." : "Set a baseline or wait for another response to this endpoint."}</span>
            </div>
          {/if}
        {:else if tab === "cURL"}
          {@const preview = textPreview(curlCommand(tx) || "cURL will be generated once the request is complete.")}
          <pre>{preview.text}</pre>
          {#if preview.truncated}
            <div class="content-truncated">
              Showing {preview.shown.toLocaleString()} of {preview.total.toLocaleString()} characters.
            </div>
          {/if}
        {:else}
          <ol class="timeline">
            <li>Request started <time>{new Date(tx.timing.request_started_ms).toLocaleTimeString()}</time></li>
            {#if tx.timing.request_complete_ms}<li>Request sent</li>{/if}
            {#if tx.timing.response_started_ms}<li>Response headers received</li>{/if}
            {#if tx.timing.response_complete_ms}<li>Response complete</li>{/if}
          </ol>
        {/if}
      </div>
      {/key}
    {:else}
      <div class="empty-state inspector-empty">
        <ListTree size={30} />
        <b>Select a request</b>
        <span>Its headers, body and timing will appear here.</span>
      </div>
    {/if}
  </aside>
  {#snippet failed(_error, reset)}
    <aside class="inspector">
      <div class="empty-state inspector-empty">
        <ListTree size={30} />
        <b>Could not render this request</b>
        <span>The capture is still available. Retry the inspector or select another request.</span>
        <button class="quiet" onclick={reset}>Retry</button>
      </div>
    </aside>
  {/snippet}
</svelte:boundary>
