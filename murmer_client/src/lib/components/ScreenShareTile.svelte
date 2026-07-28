<!--
  Screen Share Tile

  One watched screen share on the viewer stage: header, video and a resize
  grip. Several of these sit side by side, so everything the tile offers —
  audio, size, fullscreen, closing — acts on this share alone.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import type { ScreenSharePeer } from '$lib/types';
  import { setShareAudio, shareAudio } from '$lib/stores/screenShare';
  import { displayNames } from '$lib/stores/profiles';
  import { outputMuted, outputDeviceId } from '$lib/stores/settings';

  interface Props {
    /** Account name of the sharer — the key for everything, display or not. */
    userId: string;
    /** The incoming stream, or null while the connection is still coming up. */
    peer: ScreenSharePeer | null;
    /** Viewing our own capture: there is no peer connection to tear down. */
    isSelf?: boolean;
    /** Width in pixels once the user has resized it; null = shared layout. */
    width: number | null;
    onResize: (width: number | null) => void;
    onClose: () => void;
  }

  let { userId, peer, isSelf = false, width, onResize, onClose }: Props = $props();

  /** Below this a share is unreadable; the grip stops there. */
  const MIN_WIDTH = 320;
  /** Keyboard resize step. */
  const WIDTH_STEP = 64;

  let videoElement: HTMLVideoElement | undefined = $state();
  let tileElement: HTMLDivElement | undefined = $state();
  let isFullscreen = $state(false);
  // The stream's own aspect ratio, so a resized tile stays the shape of the
  // shared screen instead of letterboxing into a 16:9 box.
  let aspect = $state(16 / 9);

  let audio = $derived($shareAudio(userId));
  /** Volume and mute are only worth showing when there is audio to control. */
  let showAudioControls = $derived(peer?.hasAudio === true && !isSelf);

  $effect(() => {
    // The stream object is stable for the lifetime of the share, so this only
    // re-attaches when the tile is actually pointed at a different one —
    // re-assigning `srcObject` restarts playback from black.
    if (videoElement && peer?.stream && videoElement.srcObject !== peer.stream) {
      videoElement.srcObject = peer.stream;
    }
  });

  $effect(() => {
    if (!videoElement) return;
    // The sharer's own preview stays silent: it would play the audio the
    // machine is already producing a second time.
    // Deafening covers this too — it means "nothing from this channel".
    videoElement.muted = isSelf || $outputMuted || audio.muted;
    videoElement.volume = audio.volume;
  });

  // Like the peer voice elements, the share has to be routed to the chosen
  // output device individually; without this it plays on the system default
  // while everything else goes to the headset.
  $effect(() => {
    const element = videoElement as
      | (HTMLVideoElement & { setSinkId?: (id: string) => Promise<void> })
      | undefined;
    element?.setSinkId?.($outputDeviceId || '').catch((error: unknown) => {
      console.warn('Failed to route the screen share to the selected output:', error);
    });
  });

  function updateAspect() {
    if (!videoElement?.videoWidth || !videoElement.videoHeight) return;
    aspect = videoElement.videoWidth / videoElement.videoHeight;
  }

  function setVolume(volume: number) {
    setShareAudio(userId, { volume, muted: audio.muted });
  }

  function toggleMute() {
    setShareAudio(userId, { volume: audio.volume, muted: !audio.muted });
  }

  /** Keep the tile between the readable minimum and the window width. */
  function clampWidth(next: number): number {
    const max = Math.max(MIN_WIDTH, window.innerWidth - 32);
    return Math.round(Math.min(Math.max(next, MIN_WIDTH), max));
  }

  function currentWidth(): number {
    return width ?? tileElement?.getBoundingClientRect().width ?? MIN_WIDTH;
  }

  /**
   * Drag the corner grip. Only the width is stored — the height follows the
   * stream's aspect ratio — but the vertical distance is folded in through
   * that ratio so dragging the corner diagonally behaves the way it looks.
   */
  function startResize(event: PointerEvent) {
    if (event.button !== 0) return;
    const grip = event.currentTarget as HTMLElement;
    const startX = event.clientX;
    const startY = event.clientY;
    const startWidth = currentWidth();

    event.preventDefault();
    grip.setPointerCapture(event.pointerId);

    const move = (moveEvent: PointerEvent) => {
      const delta = moveEvent.clientX - startX + (moveEvent.clientY - startY) * aspect;
      onResize(clampWidth(startWidth + delta));
    };
    const end = () => {
      grip.releasePointerCapture(event.pointerId);
      grip.removeEventListener('pointermove', move);
      grip.removeEventListener('pointerup', end);
      grip.removeEventListener('pointercancel', end);
    };
    grip.addEventListener('pointermove', move);
    grip.addEventListener('pointerup', end);
    grip.addEventListener('pointercancel', end);
  }

  function resizeByKey(event: KeyboardEvent) {
    if (event.key === 'ArrowRight') {
      onResize(clampWidth(currentWidth() + WIDTH_STEP));
    } else if (event.key === 'ArrowLeft') {
      onResize(clampWidth(currentWidth() - WIDTH_STEP));
    } else if (event.key === 'Home' || event.key === 'Enter' || event.key === ' ') {
      // Back to the automatic layout, where the tile shares the row evenly
      // with the other shares.
      onResize(null);
    } else {
      return;
    }
    event.preventDefault();
  }

  function toggleFullscreen() {
    if (!videoElement) return;
    if (!isFullscreen) {
      videoElement.requestFullscreen?.();
    } else {
      document.exitFullscreen?.();
    }
  }

  function handleFullscreenChange() {
    isFullscreen = document.fullscreenElement === videoElement;
  }

  onMount(() => {
    document.addEventListener('fullscreenchange', handleFullscreenChange);
    return () => {
      document.removeEventListener('fullscreenchange', handleFullscreenChange);
    };
  });
</script>

<div
  class="tile"
  class:fixed={width !== null}
  style={width !== null ? `width: ${width}px` : ''}
  bind:this={tileElement}
  role="group"
  aria-label={isSelf ? 'Your screen share preview' : `${$displayNames(userId)}'s screen share`}
>
  <div class="tile-header">
    <div class="tile-title">
      <h3>{isSelf ? 'Your screen (preview)' : $displayNames(userId)}</h3>
      {#if isSelf}
        <!-- Sharing audio is a checkbox in the OS picker that is easy to
             miss, so the sharer is told which way it went. -->
        <span class="tile-note">
          {peer?.hasAudio ? 'Sharing system audio' : 'No system audio'}
        </span>
      {/if}
    </div>
    <div class="tile-controls">
      {#if showAudioControls}
        <button
          class="icon-btn"
          onclick={toggleMute}
          title={audio.muted ? 'Unmute this share' : 'Mute this share'}
          aria-label={audio.muted ? 'Unmute this share' : 'Mute this share'}
          aria-pressed={audio.muted}
        >
          {#if audio.muted || audio.volume === 0}
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><line x1="22" y1="9" x2="16" y2="15"/><line x1="16" y1="9" x2="22" y2="15"/></svg>
          {:else}
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5"/><path d="M15.54 8.46a5 5 0 0 1 0 7.07"/><path d="M18.36 5.64a9 9 0 0 1 0 12.72"/></svg>
          {/if}
        </button>
        <input
          class="volume-slider"
          type="range"
          min="0"
          max="1"
          step="0.01"
          value={audio.volume}
          oninput={(e) => setVolume(e.currentTarget.valueAsNumber)}
          disabled={audio.muted}
          title="Volume of this share"
          aria-label="Volume of this share"
        />
      {/if}
      <button
        class="icon-btn"
        onclick={toggleFullscreen}
        title="Toggle fullscreen"
        aria-label="Toggle fullscreen"
      >
        {#if isFullscreen}
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="4 14 10 14 10 20"/><polyline points="20 10 14 10 14 4"/><line x1="14" y1="10" x2="21" y2="3"/><line x1="3" y1="21" x2="10" y2="14"/></svg>
        {:else}
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="15 3 21 3 21 9"/><polyline points="9 21 3 21 3 15"/><line x1="21" y1="3" x2="14" y2="10"/><line x1="3" y1="21" x2="10" y2="14"/></svg>
        {/if}
      </button>
      <button
        class="icon-btn"
        onclick={onClose}
        title={isSelf ? 'Collapse the preview' : 'Stop watching this share'}
        aria-label={isSelf ? 'Collapse the preview' : 'Stop watching this share'}
      >
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
      </button>
    </div>
  </div>

  <div class="tile-stage" style="--tile-aspect: {aspect}">
    {#if peer}
      <!-- svelte-ignore a11y_media_has_caption -->
      <video
        bind:this={videoElement}
        autoplay
        playsinline
        muted={isSelf}
        class="tile-video"
        onloadedmetadata={updateAspect}
        onresize={updateAspect}
      ></video>
    {:else}
      <div class="tile-placeholder">
        <span class="spinner" aria-hidden="true"></span>
        <span>Connecting to {$displayNames(userId)}'s screen…</span>
      </div>
    {/if}
  </div>

  <button
    class="resize-grip"
    onpointerdown={startResize}
    onkeydown={resizeByKey}
    ondblclick={() => onResize(null)}
    title="Drag to resize · double-click to fit"
    aria-label="Resize this share"
  >
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" aria-hidden="true"><line x1="21" y1="9" x2="9" y2="21"/><line x1="21" y1="16" x2="16" y2="21"/></svg>
  </button>
</div>

<style>
  .tile {
    position: relative;
    /* Auto layout: tiles share the row and wrap once they no longer fit. A
       resized tile switches to its explicit width below. */
    flex: 1 1 32rem;
    min-width: 20rem;
    max-width: 100%;
    display: flex;
    flex-direction: column;
    background: var(--color-surface-elevated);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    overflow: hidden;
  }

  .tile.fixed {
    flex: 0 0 auto;
  }

  .tile-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    background: var(--color-surface-raised);
    border-bottom: 1px solid var(--color-surface-outline);
  }

  .tile-title {
    display: flex;
    align-items: baseline;
    gap: var(--space-3);
    min-width: 0;
  }

  .tile-header h3 {
    margin: 0;
    font-size: var(--text-base);
    font-weight: 600;
    color: var(--color-on-surface);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .tile-note {
    font-size: var(--text-xs);
    color: var(--color-muted);
    white-space: nowrap;
  }

  .tile-controls {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    flex-shrink: 0;
  }

  /* Compact variant of the shared icon button: the full control height would
     dominate a tile header that several shares have to fit next to. */
  .tile-controls :global(.icon-btn) {
    width: 1.875rem;
    height: 1.875rem;
  }

  .volume-slider {
    width: 5rem;
    min-height: 0;
    accent-color: var(--color-primary);
    cursor: pointer;
  }

  .volume-slider:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .tile-stage {
    display: flex;
    align-items: center;
    justify-content: center;
    background: #000;
  }

  .tile-video {
    display: block;
    width: 100%;
    aspect-ratio: var(--tile-aspect, 1.7777);
    /* Leave room for the stage padding, the tile header and the toolbar, so a
       tall stream letterboxes instead of pushing the tile off screen. */
    max-height: calc(100vh - 12rem);
    object-fit: contain;
  }

  .tile-video:fullscreen {
    width: 100vw;
    height: 100vh;
    max-height: none;
    aspect-ratio: auto;
    object-fit: contain;
  }

  .tile-placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-3);
    width: 100%;
    aspect-ratio: 16 / 9;
    max-height: calc(100vh - 12rem);
    color: var(--color-muted);
    font-size: var(--text-sm);
  }

  .spinner {
    width: 1.5rem;
    height: 1.5rem;
    border: 2px solid var(--color-surface-outline);
    border-top-color: var(--color-primary);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation: none;
    }
  }

  .resize-grip {
    position: absolute;
    right: 0;
    bottom: 0;
    width: 1.5rem;
    height: 1.5rem;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    min-height: 0;
    background: transparent;
    border: none;
    color: var(--color-muted);
    cursor: nwse-resize;
    touch-action: none;
  }

  .resize-grip:hover,
  .resize-grip:focus-visible {
    color: var(--color-on-surface);
  }
</style>
