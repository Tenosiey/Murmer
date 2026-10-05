<!--
  Drag handle between the chat pane and one of its sidebars. The move and
  release are heard on the window, not the handle: the pointer leaves a 5px
  strip on the first fast drag, and a resize that only stopped once the
  pointer happened to cross back would keep following it.
-->
<script lang="ts">
  import type { Writable } from 'svelte/store';

  /** Narrowest a sidebar can be dragged to, so it never disappears. */
  const MIN_WIDTH = 80;

  interface Props {
    /** The persisted width of the sidebar this handle resizes. */
    width: Writable<number>;
    /** Which sidebar: dragging right widens the left one, narrows the right. */
    side: 'left' | 'right';
    label: string;
  }

  let { width, side, label }: Props = $props();

  let dragging = false;
  let startX = 0;

  function start(event: MouseEvent) {
    dragging = true;
    startX = event.clientX;
  }

  function move(event: MouseEvent) {
    if (!dragging) return;
    const diff = side === 'left' ? event.clientX - startX : startX - event.clientX;
    startX = event.clientX;
    width.update((w) => Math.max(MIN_WIDTH, w + diff));
  }
</script>

<svelte:window onmousemove={move} onmouseup={() => (dragging = false)} />

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div class="resizer" role="separator" aria-label={label} onmousedown={start}></div>

<style>
  .resizer {
    width: 5px;
    margin: 0 -2px;
    cursor: col-resize;
    position: relative;
    flex-shrink: 0;
    z-index: 5;
  }

  .resizer:hover {
    background: color-mix(in srgb, var(--color-primary) 35%, transparent);
  }

  /* The panes stack on narrow windows, leaving nothing side by side to resize. */
  @media (max-width: 1100px) {
    .resizer {
      display: none;
    }
  }
</style>
