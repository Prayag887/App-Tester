<script lang="ts">
  import { Download, RefreshCw, X } from "lucide-svelte";
  import { dismissUpdate, installUpdate, updater } from "../updates.svelte";

  const visible = $derived(
    updater.status === "available" || updater.status === "downloading",
  );
</script>

{#if visible}
  <section class="update-banner" aria-live="polite">
    <div>
      <Download size={18} />
      <span>
        <b>App Tester {updater.version} is available</b>
        <small>{updater.message || updater.notes || "A signed desktop update is ready."}</small>
      </span>
    </div>
    {#if updater.status === "downloading"}
      <span class="update-progress">
        <i style:width={`${updater.progress}%`}></i>
      </span>
      <button class="quiet" disabled><RefreshCw class="spin" />{updater.progress ? `${updater.progress}%` : "Downloading"}</button>
    {:else}
      <button class="primary" onclick={() => void installUpdate()}><Download />Update and restart</button>
      <button class="icon-button" aria-label="Remind me later" title="Remind me later" onclick={dismissUpdate}><X /></button>
    {/if}
  </section>
{/if}
