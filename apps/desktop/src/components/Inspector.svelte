<script lang="ts">
  import { ListTree, ShieldCheck, X } from "lucide-svelte";
  import {
    bodyTextPreview,
    bodyImagePreviews,
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
      <div class="inspector-heading">
        <div><span>{tx.request.method}</span><b>{tx.request.host}{tx.request.path}</b></div>
        <strong class:failed={(tx.response?.status ?? 0) >= 400}>{tx.response?.status ?? "Pending"}</strong>
        <button class="inspector-close icon-button" aria-label="Close request inspector" title="Close inspector" onclick={() => { ui.selectedId = ""; ui.transactionDetail = null; }}><X/></button>
      </div>
      {#if ui.detailLoading}<div class="inspector-progress" aria-label="Loading request details"><span></span></div>{/if}
      <div class="tabs" role="tablist" aria-label="Request details">
        {#each TABS as name (name)}
          <button role="tab" aria-selected={tab === name} class:active={tab === name} onclick={() => setTab(name)}>{name}</button>
        {/each}
      </div>
      <div class:backward={tabDirection === "backward"} class="detail-panel inspector-tab-content" role="tabpanel">
        {#if tab === "Request" || tab === "Response"}
          {@const message = tab === "Request" ? tx.request : tx.response}
          {@const preview = bodyTextPreview(message?.body)}
          {@const images = bodyImagePreviews(message?.body, message?.content_type)}
          <h3>{tab} headers</h3>
          <div class="header-list">
            {#each message?.headers || [] as header, index (`${index}:${header.name}:${header.value}`)}
              <div><b>{header.name}</b><span>{header.value}</span></div>
            {/each}
          </div>
          <h3>Body</h3>
          {#if images.length}
            <div class="body-image-grid" aria-label="Image previews">
              {#each images as image, index (`${index}:${image.name}:${image.byteLength}`)}
                <figure class="body-image-preview">
                  <img src={image.dataUrl} alt={`Multipart upload preview: ${image.name}`} />
                  <figcaption><b>{image.name}</b><span>{image.mediaType} · {byteSizeLabel(image.byteLength)}</span></figcaption>
                </figure>
              {/each}
            </div>
          {/if}
          <pre>{preview.truncated ? preview.text : prettyJson(preview.text || "No body")}</pre>
          {#if preview.truncated}
            <div class="content-truncated">
              Showing {byteSizeLabel(preview.shown)} of {byteSizeLabel(preview.total)}. The full body remains in the capture.
            </div>
          {/if}
        {:else if tab === "Compare"}
          {#if tx.daily_changes?.count}
            <div class="daily-change-summary">
              <b>Changed {tx.daily_changes.count} {tx.daily_changes.count === 1 ? "time" : "times"} today</b>
              <span>The latest response replaced earlier snapshots while this daily history was preserved.</span>
            </div>
          {/if}
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

<style>
  .inspector-heading b { font-size: 12px; }
  .tabs { min-height: 44px; gap: 4px; padding: 4px 8px 0; }
  .tabs button { min-height: 39px; padding: 9px 11px; font-size: 12.5px; font-weight: 650; line-height: 1; }
  .tabs button::after {
    right: 11px;
    bottom: 0;
    left: 11px;
    height: 3px;
    opacity: 0;
    transform: scaleX(.08);
    transform-origin: center;
    will-change: transform,opacity;
    transition: transform 280ms cubic-bezier(.16,1,.3,1),opacity 160ms ease,box-shadow 240ms ease;
  }
  .tabs button:hover::after { opacity: .58; transform: scaleX(.42); }
  .tabs button.active::after {
    opacity: 1;
    transform: scaleX(1);
    box-shadow: 0 0 8px color-mix(in srgb,var(--shell-accent) 42%,transparent);
    animation: tab-indicator-lock 480ms linear(0,.28 15%,.76 42%,1.12 65%,.97 84%,1) both;
  }
  .detail-panel { padding: 14px; font-size: 12.5px; }
  .detail-panel h3 { margin: 16px 0 8px; font-size: 11px; font-weight: 750; }
  .detail-panel pre { font-size: 12.5px; line-height: 1.65; }
  .header-list > div { grid-template-columns: minmax(120px,.75fr) minmax(0,1.25fr); padding: 10px 12px; font-size: 12.5px; line-height: 1.45; }
  .header-list b { font-size: 12px; }
  .timeline { font-size: 12.5px; line-height: 1.5; }
  .daily-change-summary span,.compare-summary small { font-size: 12px; line-height: 1.5; }
  .compare-summary > div > span { font-size: 11px; }
  .compare-summary b { font-size: 13px; }
  .difference-list article { font-size: 12.5px; line-height: 1.5; }
  @keyframes tab-indicator-lock {
    0% { opacity: .58; transform: scaleX(.42); box-shadow: 0 0 0 transparent; }
    64% { opacity: 1; transform: scaleX(1.12); box-shadow: 0 0 14px color-mix(in srgb,var(--shell-accent) 58%,transparent); }
    84% { transform: scaleX(.97); }
    100% { opacity: 1; transform: scaleX(1); box-shadow: 0 0 8px color-mix(in srgb,var(--shell-accent) 42%,transparent); }
  }
</style>
