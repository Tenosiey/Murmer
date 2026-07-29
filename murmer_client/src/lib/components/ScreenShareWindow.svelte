<!--
  Screen Share Window

  One watched share, floating above the app: drag it by the header, resize it
  from any corner, shrink it into a corner as picture-in-picture or blow it up
  to fill the viewport — none of which blocks the chat underneath. Everything
  the window offers (audio, size, fullscreen, closing) acts on this share
  alone, so a second share opened next to it is untouched.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import type { ScreenSharePeer } from '$lib/types';
  import { setShareAudio, shareAudio } from '$lib/stores/screenShare';
  import {
    screenShareWindows,
    WINDOW_HEADER_HEIGHT,
    type ScreenShareWindowState,
    type Viewport
  } from '$lib/stores/screenShareWindows';
  import { displayNames } from '$lib/stores/profiles';
  import { outputMuted, outputDeviceId } from '$lib/stores/settings';

  interface Props {
    /** Identifies this window in the geometry store. */
    windowKey: string;
    /** Account name of the sharer — the key for everything, display or not. */
    userId: string;
    /** The incoming stream, or null while the connection is still coming up. */
    peer: ScreenSharePeer | null;
    /** Viewing our own capture: there is no peer connection to tear down. */
    isSelf?: boolean;
    /** Position, size and mode, owned by `screenShareWindows`. */
    geometry: ScreenShareWindowState;
    /** The area windows may occupy. */
    viewport: Viewport;
    onClose: () => void;
  }

  let { windowKey, userId, peer, isSelf = false, geometry, viewport, onClose }: Props = $props();

  /** Below this a window is unusable; the resize handles stop there. */
  const MIN_WIDTH = 280;
  const MIN_HEIGHT = 160;
  /** Keyboard move/resize step. */
  const STEP = 32;

  let videoElement: HTMLVideoElement | undefined = $state();
  let isFullscreen = $state(false);
  // The stream's own aspect ratio, so "fit" and the picture-in-picture box end
  // up the shape of the shared screen instead of a hardcoded 16:9.
  let aspect = $state(16 / 9);

  let audio = $derived($shareAudio(userId));
  /** Volume and mute are only worth showing when there is audio to control. */
  let showAudioControls = $derived(peer?.hasAudio === true && !isSelf);
  /** A picture-in-picture window has room for the essentials only. */
  let compact = $derived(geometry.mode === 'pip');
  let title = $derived(isSelf ? 'Your screen' : $displayNames(userId));

  $effect(() => {
    // The stream object is stable for the lifetime of the share, so this only
    // re-attaches when the window is actually pointed at a different one —
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

  function raise() {
    screenShareWindows.raise(windowKey);
  }

  function togglePip() {
    screenShareWindows.toggleMode(windowKey, 'pip', viewport, aspect);
  }

  function toggleMaximized() {
    screenShareWindows.toggleMode(windowKey, 'maximized', viewport, aspect);
  }

  /**
   * Follow the pointer with the whole window. A maximized window has nowhere
   * to go, so dragging it is ignored rather than silently resized.
   */
  function startDrag(event: PointerEvent) {
    if (event.button !== 0 || geometry.mode === 'maximized') return;
    const handle = event.currentTarget as HTMLElement;
    const startX = event.clientX;
    const startY = event.clientY;
    const originX = geometry.x;
    const originY = geometry.y;

    event.preventDefault();
    raise();
    handle.setPointerCapture(event.pointerId);

    const move = (moveEvent: PointerEvent) => {
      screenShareWindows.move(
        windowKey,
        originX + moveEvent.clientX - startX,
        originY + moveEvent.clientY - startY,
        viewport
      );
    };
    const end = () => {
      handle.releasePointerCapture(event.pointerId);
      handle.removeEventListener('pointermove', move);
      handle.removeEventListener('pointerup', end);
      handle.removeEventListener('pointercancel', end);
    };
    handle.addEventListener('pointermove', move);
    handle.addEventListener('pointerup', end);
    handle.addEventListener('pointercancel', end);
  }

  function moveByKey(event: KeyboardEvent) {
    const dx = event.key === 'ArrowLeft' ? -STEP : event.key === 'ArrowRight' ? STEP : 0;
    const dy = event.key === 'ArrowUp' ? -STEP : event.key === 'ArrowDown' ? STEP : 0;
    if (dx === 0 && dy === 0) return;
    if (geometry.mode === 'maximized') return;
    screenShareWindows.move(windowKey, geometry.x + dx, geometry.y + dy, viewport);
    event.preventDefault();
  }

  /**
   * Drag one of the four corners. The corners that hold an edge in place move
   * the window as they resize it, which is what makes dragging the top-left
   * corner grow the window towards the top left instead of away from it.
   */
  function startResize(event: PointerEvent, corner: 'nw' | 'ne' | 'sw' | 'se') {
    if (event.button !== 0 || geometry.mode === 'maximized') return;
    const handle = event.currentTarget as HTMLElement;
    const startX = event.clientX;
    const startY = event.clientY;
    const origin = { x: geometry.x, y: geometry.y, width: geometry.width, height: geometry.height };
    const west = corner === 'nw' || corner === 'sw';
    const north = corner === 'nw' || corner === 'ne';

    event.preventDefault();
    raise();
    handle.setPointerCapture(event.pointerId);

    const move = (moveEvent: PointerEvent) => {
      const dx = moveEvent.clientX - startX;
      const dy = moveEvent.clientY - startY;
      const width = Math.max(MIN_WIDTH, origin.width + (west ? -dx : dx));
      const height = Math.max(MIN_HEIGHT, origin.height + (north ? -dy : dy));
      screenShareWindows.resize(
        windowKey,
        {
          width,
          height,
          // The anchored edge stays put: its coordinate follows the size that
          // actually applied, so hitting the minimum stops the drag dead
          // instead of walking the window across the screen.
          x: west ? origin.x + origin.width - width : origin.x,
          y: north ? origin.y + origin.height - height : origin.y
        },
        viewport
      );
    };
    const end = () => {
      handle.releasePointerCapture(event.pointerId);
      handle.removeEventListener('pointermove', move);
      handle.removeEventListener('pointerup', end);
      handle.removeEventListener('pointercancel', end);
    };
    handle.addEventListener('pointermove', move);
    handle.addEventListener('pointerup', end);
    handle.addEventListener('pointercancel', end);
  }

  function resizeByKey(event: KeyboardEvent) {
    if (event.key === 'ArrowRight' || event.key === 'ArrowLeft') {
      const width = geometry.width + (event.key === 'ArrowRight' ? STEP : -STEP);
      screenShareWindows.resize(windowKey, { ...geometry, width }, viewport);
    } else if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
      const height = geometry.height + (event.key === 'ArrowDown' ? STEP : -STEP);
      screenShareWindows.resize(windowKey, { ...geometry, height }, viewport);
    } else if (event.key === 'Home' || event.key === 'Enter' || event.key === ' ') {
      screenShareWindows.fitAspect(windowKey, aspect, viewport);
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

  /** Escape closes the window the user is working in — never all of them. */
  function handleKeydown(event: KeyboardEvent) {
    if (event.key !== 'Escape' || document.fullscreenElement) return;
    event.stopPropagation();
    onClose();
  }

  onMount(() => {
    document.addEventListener('fullscreenchange', handleFullscreenChange);
    return () => {
      document.removeEventListener('fullscreenchange', handleFullscreenChange);
    };
  });
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<section
  class="window"
  class:compact
  style="left: {geometry.x}px; top: {geometry.y}px; width: {geometry.width}px; height: {geometry.height}px; z-index: {geometry.z};"
  aria-label={isSelf ? 'Your screen share preview' : `${$displayNames(userId)}'s screen share`}
  onpointerdown={raise}
  onkeydown={handleKeydown}
>
  <div class="window-header" style="height: {WINDOW_HEADER_HEIGHT}px">
    <!-- The drag handle is a button so the window can also be moved from the
         keyboard; double-clicking it maximizes, the way a title bar does. -->
    <button
      class="drag-handle"
      onpointerdown={startDrag}
      onkeydown={moveByKey}
      ondblclick={toggleMaximized}
      title="Drag to move · double-click to maximize"
      aria-label={`Move ${title}'s window`}
    >
      {#if isSelf}
        <span class="live-dot" aria-hidden="true"></span>
      {/if}
      <span class="window-title">{title}</span>
      {#if isSelf && !compact}
        <!-- Sharing audio is a checkbox in the OS picker that is easy to
             miss, so the sharer is told which way it went. -->
        <span class="window-note">
          {peer?.hasAudio ? 'Sharing system audio' : 'No system audio'}
        </span>
      {/if}
    </button>
    <div class="window-controls">
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
        {#if !compact}
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
      {/if}
      <button
        class="icon-btn"
        onclick={togglePip}
        title={compact ? 'Back to a normal window' : 'Shrink into the corner'}
        aria-label={compact ? 'Back to a normal window' : 'Shrink into the corner'}
        aria-pressed={compact}
      >
        {#if compact}
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="2"/><polyline points="9 15 9 11 13 11"/><line x1="15" y1="9" x2="9" y2="15"/></svg>
        {:else}
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="4" width="18" height="16" rx="2"/><rect x="12" y="12" width="7" height="5" rx="1"/></svg>
        {/if}
      </button>
      {#if !compact}
        <button
          class="icon-btn"
          onclick={toggleMaximized}
          title={geometry.mode === 'maximized' ? 'Restore this window' : 'Fill the app window'}
          aria-label={geometry.mode === 'maximized' ? 'Restore this window' : 'Fill the app window'}
          aria-pressed={geometry.mode === 'maximized'}
        >
          {#if geometry.mode === 'maximized'}
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="4 14 10 14 10 20"/><polyline points="20 10 14 10 14 4"/><line x1="14" y1="10" x2="21" y2="3"/><line x1="3" y1="21" x2="10" y2="14"/></svg>
          {:else}
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="15 3 21 3 21 9"/><polyline points="9 21 3 21 3 15"/><line x1="21" y1="3" x2="14" y2="10"/><line x1="3" y1="21" x2="10" y2="14"/></svg>
          {/if}
        </button>
        <button
          class="icon-btn"
          onclick={toggleFullscreen}
          title="Toggle fullscreen"
          aria-label="Toggle fullscreen"
        >
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M8 3H5a2 2 0 0 0-2 2v3"/><path d="M16 3h3a2 2 0 0 1 2 2v3"/><path d="M8 21H5a2 2 0 0 1-2-2v-3"/><path d="M16 21h3a2 2 0 0 0 2-2v-3"/></svg>
        </button>
      {/if}
      <button
        class="icon-btn"
        onclick={onClose}
        title={isSelf ? 'Hide your preview' : 'Stop watching this share'}
        aria-label={isSelf ? 'Hide your preview' : 'Stop watching this share'}
      >
        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>
      </button>
    </div>
  </div>

  <div class="window-body">
    {#if peer}
      <!-- svelte-ignore a11y_media_has_caption -->
      <video
        bind:this={videoElement}
        autoplay
        playsinline
        muted={isSelf}
        class="window-video"
        onloadedmetadata={updateAspect}
        onresize={updateAspect}
        ondblclick={toggleMaximized}
      ></video>
      {#if peer.reconnecting}
        <!-- The last frame stays up underneath: the share has not ended, and
             replacing a frozen picture with a spinner hides which share this
             window even is. -->
        <div class="window-reconnecting" role="status">
          <span class="spinner" aria-hidden="true"></span>
          <span>Reconnecting…</span>
        </div>
      {/if}
    {:else}
      <div class="window-placeholder">
        <span class="spinner" aria-hidden="true"></span>
        <span>Connecting to {$displayNames(userId)}'s screen…</span>
      </div>
    {/if}
  </div>

  {#if geometry.mode !== 'maximized'}
    {#each [['nw', 'Top left'], ['ne', 'Top right'], ['sw', 'Bottom left'], ['se', 'Bottom right']] as [corner, label] (corner)}
      <button
        class="resize-handle {corner}"
        onpointerdown={(event) => startResize(event, corner as 'nw' | 'ne' | 'sw' | 'se')}
        onkeydown={resizeByKey}
        ondblclick={() => screenShareWindows.fitAspect(windowKey, aspect, viewport)}
        title="Drag to resize · double-click to fit the stream"
        aria-label={`Resize ${title}'s window from the ${label.toLowerCase()} corner`}
      ></button>
    {/each}
  {/if}
</section>

<style>
  .window {
    position: fixed;
    display: flex;
    flex-direction: column;
    /* Free-floating chrome, not a modal: the app underneath keeps working and
       menus/dialogs still open on top of it. Windows only order among
       themselves, through the z the store hands out. */
    background: var(--color-surface-elevated);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    overflow: hidden;
    pointer-events: auto;
  }

  .window-header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: 0 var(--space-2) 0 var(--space-1);
    background: var(--color-surface-raised);
    border-bottom: 1px solid var(--color-surface-outline);
    flex-shrink: 0;
  }

  .drag-handle {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    height: 100%;
    min-height: 0;
    padding: 0 var(--space-2);
    background: transparent;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--color-on-surface);
    text-align: left;
    cursor: grab;
    touch-action: none;
  }

  .drag-handle:active {
    cursor: grabbing;
  }

  .live-dot {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: var(--color-error);
    flex-shrink: 0;
    align-self: center;
  }

  .window-title {
    font-size: var(--text-sm);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .window-note {
    font-size: var(--text-xs);
    color: var(--color-muted);
    white-space: nowrap;
  }

  .window-controls {
    display: flex;
    align-items: center;
    gap: var(--space-1);
    flex-shrink: 0;
  }

  /* Compact variant of the shared icon button: the full control height would
     dominate a header that several windows have to fit next to. */
  .window-controls :global(.icon-btn) {
    width: 1.875rem;
    height: 1.875rem;
    min-height: 0;
  }

  .compact .window-controls :global(.icon-btn) {
    width: 1.625rem;
    height: 1.625rem;
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

  .window-body {
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: #000;
    position: relative;
  }

  /* Sits over the frozen last frame rather than replacing it. */
  .window-reconnecting {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-3);
    padding: var(--space-3);
    background: color-mix(in srgb, var(--color-bg) 65%, transparent);
    color: var(--color-on-surface);
    font-size: var(--text-sm);
    text-align: center;
  }

  /* The window is sized by the user, so the stream is letterboxed into
     whatever box it ended up as; the resize handles offer "fit the stream"
     for anyone who wants the shape back. */
  .window-video {
    display: block;
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .window-video:fullscreen {
    width: 100vw;
    height: 100vh;
  }

  .window-placeholder {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-3);
    width: 100%;
    height: 100%;
    padding: var(--space-3);
    color: var(--color-muted);
    font-size: var(--text-sm);
    text-align: center;
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

  .resize-handle {
    position: absolute;
    width: 1.125rem;
    height: 1.125rem;
    padding: 0;
    min-height: 0;
    background: transparent;
    border: none;
    touch-action: none;
  }

  .resize-handle:focus-visible {
    outline: 2px solid var(--color-primary);
    outline-offset: -2px;
    border-radius: var(--radius-xs);
  }

  .resize-handle.nw {
    left: 0;
    top: 0;
    cursor: nwse-resize;
  }

  .resize-handle.ne {
    right: 0;
    top: 0;
    cursor: nesw-resize;
  }

  .resize-handle.sw {
    left: 0;
    bottom: 0;
    cursor: nesw-resize;
  }

  .resize-handle.se {
    right: 0;
    bottom: 0;
    cursor: nwse-resize;
  }
</style>
