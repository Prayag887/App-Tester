<script lang="ts">
  interface Props {
    label: string;
    onpointerdown: (event: PointerEvent) => void;
    onkeydown: (event: KeyboardEvent) => void;
    hidden?: boolean;
  }

  let { label, onpointerdown, onkeydown, hidden = false }: Props = $props();
</script>

<button
  type="button"
  class:hidden
  class="panel-resize-handle"
  aria-label={label}
  tabindex={hidden ? -1 : 0}
  {onpointerdown}
  {onkeydown}
>
  <span></span>
</button>
