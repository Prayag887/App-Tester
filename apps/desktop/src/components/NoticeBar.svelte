<script lang="ts">
  import { AlertCircle, X } from "lucide-svelte";
  import { ui } from "../stores.svelte";
  const notice = $derived(ui.notice);
  const setNotice = (message: string) => ui.notice = message;
</script>

{#if notice}<div class="notice-bar"><AlertCircle size={16}/><span>{notice}</span><button aria-label="Dismiss notice" onclick={() => setNotice("")}><X size={15}/></button></div>{/if}
