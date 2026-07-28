<!--
  Screen Share Layer

  The layer every watched share floats in. It never covers the app: only the
  windows themselves take pointer events, so chat, voice and everything else
  keep working while a share is on screen. Several shares open as several
  windows — place them side by side, shrink one into a corner, maximize
  another; the small bar only appears once there is more than one to arrange.
-->
<script lang="ts">
  import { browser } from '$app/environment';
  import ScreenShareWindow from './ScreenShareWindow.svelte';
  import { screenShareWindows } from '$lib/stores/screenShareWindows';
  import type { WatchedScreenShare } from '$lib/types';

  interface Props {
    /** The shares to show, in the order they were opened. */
    tiles: WatchedScreenShare[];
    /** Stop watching everything ("Close all"). */
    onCloseAll: () => void;
  }

  let { tiles, onCloseAll }: Props = $props();

  // The viewport the windows are clamped to. Bound to the window so a resized
  // (or maximized/restored) app pulls every window back into view instead of
  // leaving one stranded outside it. The initial read matters: the binding
  // only lands after the first effects have run, and a window placed for a
  // guessed viewport would visibly jump into place.
  let viewportWidth = $state(browser ? window.innerWidth : 1280);
  let viewportHeight = $state(browser ? window.innerHeight : 800);
  let viewport = $derived({ width: viewportWidth, height: viewportHeight });

  $effect(() => {
    // Open a window for every share being watched and drop the ones whose
    // share is gone. Our own capture starts as a corner preview — the sharer
    // wants to check it, not watch it.
    screenShareWindows.sync(
      tiles.map((tile) => ({
        key: tile.key,
        preferredMode: tile.isSelf ? ('pip' as const) : ('normal' as const)
      })),
      viewport
    );
  });

  $effect(() => {
    screenShareWindows.clampAll(viewport);
  });
</script>

<svelte:window bind:innerWidth={viewportWidth} bind:innerHeight={viewportHeight} />

<div class="layer">
  {#if tiles.length > 1}
    <div class="layer-bar">
      <span class="layer-count">Watching {tiles.length} screens</span>
      <button class="btn btn-ghost" onclick={() => screenShareWindows.tileAll(viewport)}>
        Tile
      </button>
      <button class="btn btn-ghost" onclick={onCloseAll}>Close all</button>
    </div>
  {/if}

  {#each tiles as tile (tile.key)}
    {@const geometry = $screenShareWindows[tile.key]}
    {#if geometry}
      <ScreenShareWindow
        windowKey={tile.key}
        userId={tile.userId}
        peer={tile.peer}
        isSelf={tile.isSelf}
        {geometry}
        {viewport}
        onClose={tile.onClose}
      />
    {/if}
  {/each}
</div>

<style>
  .layer {
    position: fixed;
    inset: 0;
    /* Nothing here swallows clicks — the app underneath stays usable; the
       windows and the bar switch pointer events back on for themselves. */
    pointer-events: none;
    z-index: var(--z-overlay);
  }

  .layer-bar {
    position: absolute;
    top: var(--space-2);
    left: 50%;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    background: var(--color-surface-elevated);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-pill);
    box-shadow: var(--shadow-md);
    pointer-events: auto;
    /* Above every window, so it cannot be buried by the shares it arranges.
       The layer is its own stacking context, so this only orders the bar
       against the windows in it — the app's overlays are unaffected. */
    z-index: var(--z-top);
  }

  .layer-count {
    padding-left: var(--space-2);
    color: var(--color-on-surface-variant);
    font-size: var(--text-xs);
    white-space: nowrap;
  }

  .layer-bar :global(.btn) {
    min-height: var(--control-height);
    padding: 0 var(--space-3);
    font-size: var(--text-xs);
  }
</style>
