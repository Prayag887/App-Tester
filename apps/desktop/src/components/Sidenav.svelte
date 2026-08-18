<script lang="ts">
  import { Activity, Circle, ListTree, Palette, Send, TerminalSquare } from "lucide-svelte";
  import { ui } from "../stores.svelte";
  import { applyTheme, readTheme, THEMES, type ThemeId } from "../theme";
  const capturing = $derived(ui.capturing);
  const incidents = $derived(ui.incidents);
  const packageName = $derived(ui.packageName);
  const screen = $derived(ui.screen);
  let theme = $state<ThemeId>(readTheme());
  const setScreen = (next: typeof screen) => ui.screen = next;
  const selectTheme = (next: ThemeId) => {
    theme = next;
    applyTheme(next);
  };
</script>

<aside class="sidenav">
  <div class="wordmark"><span class="wordmark-icon"><Activity size={18}/></span><b>App Tester</b></div>
  <span class="nav-eyebrow">Android diagnostics</span>
  <nav aria-label="Primary navigation">
    <button class:active={screen === "traffic"} onclick={() => setScreen("traffic")}><ListTree/><span><b>Traffic lab</b></span></button>
    <button class:active={screen === "composer"} onclick={() => setScreen("composer")}><Send/><span><b>Composer</b></span></button>
    <button class:active={screen === "logs"} onclick={() => setScreen("logs")}><TerminalSquare/><span><b>Log inspector</b></span>{#if incidents.length}<i>{incidents.length}</i>{/if}</button>
  </nav>
  <div class="sidenav-footer">
    <div class:live={capturing} class="capture-status"><Circle size={8}/><span><b>{capturing ? "Capture active" : "Ready"}</b><small>{packageName || "Select a package"}</small></span></div>
    <label class="theme-picker" title="Color theme">
      <Palette size={14}/>
      <span class="visually-hidden">Color theme</span>
      <select value={theme} onchange={(event) => selectTheme((event.currentTarget as HTMLSelectElement).value as ThemeId)} aria-label="Color theme">
        {#each THEMES as option (option.id)}
          <option value={option.id}>{option.label}</option>
        {/each}
      </select>
    </label>
  </div>
</aside>
