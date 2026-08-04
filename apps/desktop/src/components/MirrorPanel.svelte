<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { Smartphone, X } from "lucide-svelte";
  import { captureScreen, setMirrorOpen, ui } from "../stores.svelte";

  const CAPTURE_INTERVAL_MS = 1000;
  let timer: number | undefined;

  const captureTick = () => {
    // Never burn ADB bandwidth while the window is backgrounded.
    if (document.hidden) return;
    void captureScreen();
  };

  onMount(() => {
    void captureScreen();
    timer = window.setInterval(captureTick, CAPTURE_INTERVAL_MS);
  });

  onDestroy(() => {
    if (timer !== undefined) window.clearInterval(timer);
  });
</script>

<aside class="mirror-panel">
  <div class="mirror-heading">
    <span>DEVICE SCREEN</span>
    <button class="icon-button" title="Close mirror" aria-label="Close device mirror" onclick={() => setMirrorOpen(false)}><X size={15}/></button>
  </div>
  <div class="mirror-stage">
    {#if ui.mirrorData}
      <img src={ui.mirrorData} alt="Live Android screen mirror" />
    {:else if ui.mirrorError}
      <div class="mirror-hint"><Smartphone size={22}/><span>{ui.mirrorError}</span></div>
    {:else}
      <div class="mirror-hint"><Smartphone size={22}/><span>Capturing screen…</span></div>
    {/if}
  </div>
</aside>
