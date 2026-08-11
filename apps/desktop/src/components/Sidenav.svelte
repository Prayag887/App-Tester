<script lang="ts">
  import { Activity, Circle, ListTree, Send, TerminalSquare } from "lucide-svelte";
  import { ui } from "../stores.svelte";
  const capturing = $derived(ui.capturing);
  const incidents = $derived(ui.incidents);
  const packageName = $derived(ui.packageName);
  const screen = $derived(ui.screen);
  const setScreen = (next: typeof screen) => ui.screen = next;
</script>

<aside class="sidenav">
  <div class="wordmark"><Activity size={22}/><span><small>Android diagnostics</small></span></div>
  <nav aria-label="Primary navigation">
    <button class:active={screen === "traffic"} onclick={() => setScreen("traffic")}><ListTree/><span><b>Traffic lab</b><small>Requests & schema changes</small></span></button>
    <button class:active={screen === "composer"} onclick={() => setScreen("composer")}><Send/><span><b>Composer</b><small>Build & send requests</small></span></button>
    <button class:active={screen === "logs"} onclick={() => setScreen("logs")}><TerminalSquare/><span><b>Log inspector</b><small>Errors & warnings</small></span>{#if incidents.length}<i>{incidents.length}</i>{/if}</button>
  </nav>
  <div class:live={capturing} class="capture-status"><Circle size={10}/><span><b>{capturing ? "Capture active" : "Ready"}</b><small>{packageName || "Select a package"}</small></span></div>
</aside>
