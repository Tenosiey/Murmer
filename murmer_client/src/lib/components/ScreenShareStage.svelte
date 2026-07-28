<!--
  Screen Share Stage

  Holds every screen share the user is currently watching. Several people can
  share in one voice channel at the same time, so the shares are laid out side
  by side and wrap onto further rows; each tile can be resized and closed on
  its own, and the ones left open keep playing.
-->
<script lang="ts">
  import ScreenShareTile from './ScreenShareTile.svelte';
  import type { WatchedScreenShare } from '$lib/types';

  interface Props {
    /** The shares to show, in the order they were opened. */
    tiles: WatchedScreenShare[];
    /** Stop watching everything (backdrop click, Escape, "Close all"). */
    onCloseAll: () => void;
  }

  let { tiles, onCloseAll }: Props = $props();

  /**
   * Widths the user has dragged a tile to, keyed like the tiles themselves.
   * Missing (or explicitly null) means the tile takes part in the automatic
   * layout, where the open shares split the row evenly.
   */
  let widths = $state<Record<string, number | null>>({});

  function setWidth(key: string, width: number | null) {
    widths = { ...widths, [key]: width };
  }

  function closeAllFromBackdrop(event: MouseEvent) {
    // Only a click on the backdrop itself counts; anything that bubbled out
    // of a tile happened inside a share the user is watching.
    if (event.target === event.currentTarget) onCloseAll();
  }

  function handleKeydown(event: KeyboardEvent) {
    // While a share is fullscreen, Escape belongs to the browser: it leaves
    // fullscreen rather than closing what the user is watching.
    if (event.key === 'Escape' && !document.fullscreenElement) {
      onCloseAll();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="stage" onclick={closeAllFromBackdrop}>
  {#if tiles.length > 1}
    <div class="stage-bar">
      <span class="stage-count">Watching {tiles.length} screens</span>
      <button class="btn btn-ghost" onclick={onCloseAll}>Close all</button>
    </div>
  {/if}
  <div class="stage-tiles" onclick={closeAllFromBackdrop}>
    {#each tiles as tile (tile.key)}
      <ScreenShareTile
        userId={tile.userId}
        peer={tile.peer}
        isSelf={tile.isSelf}
        width={widths[tile.key] ?? null}
        onResize={(width) => setWidth(tile.key, width)}
        onClose={tile.onClose}
      />
    {/each}
  </div>
</div>

<style>
  .stage {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
    background: rgba(0, 0, 0, 0.85);
    z-index: var(--z-top);
  }

  .stage-bar {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-3);
    padding: var(--space-2) var(--space-4);
    flex-shrink: 0;
  }

  .stage-count {
    color: var(--color-on-surface-variant);
    font-size: var(--text-sm);
  }

  .stage-tiles {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-wrap: wrap;
    align-content: center;
    justify-content: center;
    gap: var(--space-3);
    padding: var(--space-4);
    overflow-y: auto;
  }
</style>
