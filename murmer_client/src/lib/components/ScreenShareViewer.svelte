<!--
  Screen Share Viewer Component

  Displays a screen share in a modal window with controls — either a remote
  user's stream or, with `isSelf`, the sharer's own capture as a preview.
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import type { ScreenSharePeer } from '$lib/types';
  import { stopViewingScreenShare } from '$lib/stores/screenShare';
  import {
    screenShareVolume,
    screenShareMuted,
    outputMuted,
    outputDeviceId
  } from '$lib/stores/settings';

  interface Props {
    peer: ScreenSharePeer;
    onClose: () => void;
    /** Viewing our own capture: there is no peer connection to tear down. */
    isSelf?: boolean;
  }

  let { peer, onClose, isSelf = false }: Props = $props();

  let videoElement: HTMLVideoElement | undefined = $state();
  let isFullscreen = $state(false);
  // Measured so the video can subtract the header from its height budget; the
  // container itself is capped at 95vh and clips overflow.
  let headerHeight = $state(0);

  /** Volume and mute are only worth showing when there is audio to control. */
  let showAudioControls = $derived(peer.hasAudio && !isSelf);

  $effect(() => {
    // The stream object is stable for the lifetime of the share, so this only
    // re-attaches when the viewer is actually pointed at a different one —
    // re-assigning `srcObject` restarts playback from black.
    if (videoElement && peer.stream && videoElement.srcObject !== peer.stream) {
      videoElement.srcObject = peer.stream;
    }
  });

  $effect(() => {
    if (!videoElement) return;
    // The sharer's own preview stays silent: it would play the audio the
    // machine is already producing a second time.
    // Deafening covers this too — it means "nothing from this channel".
    videoElement.muted = isSelf || $outputMuted || $screenShareMuted;
    videoElement.volume = $screenShareVolume;
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

  onDestroy(() => {
    if (!isSelf) stopViewingScreenShare(peer.userId);
  });

  function close() {
    if (!isSelf) stopViewingScreenShare(peer.userId);
    onClose();
  }

  function toggleFullscreen() {
    if (!videoElement) return;

    if (!isFullscreen) {
      if (videoElement.requestFullscreen) {
        videoElement.requestFullscreen();
      }
    } else {
      if (document.exitFullscreen) {
        document.exitFullscreen();
      }
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

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape' && !isFullscreen) {
      close();
    } else if (event.key === 'f' || event.key === 'F') {
      toggleFullscreen();
    }
  }
</script>

<svelte:window onkeydown={handleKeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="screenshare-overlay" onclick={close}>
  <div
    class="screenshare-container"
    style="--screenshare-header-height: {headerHeight}px"
    onclick={(e) => e.stopPropagation()}
    role="dialog"
    aria-label="Screen share viewer"
    tabindex="-1"
  >
    <div class="screenshare-header" bind:clientHeight={headerHeight}>
      <div class="screenshare-title">
        <h3>{isSelf ? 'Your screen (preview)' : `${peer.userId}'s Screen`}</h3>
        {#if isSelf}
          <!-- Sharing audio is a checkbox in the OS picker that is easy to
               miss, so the sharer is told which way it went. -->
          <span class="audio-note">
            {peer.hasAudio ? 'Sharing system audio' : 'No system audio'}
          </span>
        {/if}
      </div>
      <div class="screenshare-controls">
        {#if showAudioControls}
          <button
            onclick={() => screenShareMuted.update((muted) => !muted)}
            title={$screenShareMuted ? 'Unmute the shared audio' : 'Mute the shared audio'}
            aria-label={$screenShareMuted ? 'Unmute the shared audio' : 'Mute the shared audio'}
            aria-pressed={$screenShareMuted}
          >
            {#if $screenShareMuted || $screenShareVolume === 0}
              <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" d="M17.25 9.75L21 13.5m0-3.75l-3.75 3.75M11.25 4.5L6.75 8.25H3.75a.75.75 0 00-.75.75v6c0 .414.336.75.75.75h3l4.5 3.75a.75.75 0 001.25-.575V5.075a.75.75 0 00-1.25-.575z" />
              </svg>
            {:else}
              <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
                <path stroke-linecap="round" stroke-linejoin="round" d="M11.25 4.5L6.75 8.25H3.75a.75.75 0 00-.75.75v6c0 .414.336.75.75.75h3l4.5 3.75a.75.75 0 001.25-.575V5.075a.75.75 0 00-1.25-.575zM16.5 8.25a5.25 5.25 0 010 7.5M19.5 5.25a9 9 0 010 13.5" />
              </svg>
            {/if}
          </button>
          <input
            class="volume-slider"
            type="range"
            min="0"
            max="1"
            step="0.01"
            bind:value={$screenShareVolume}
            disabled={$screenShareMuted}
            title="Shared audio volume"
            aria-label="Shared audio volume"
          />
        {/if}
        <button onclick={toggleFullscreen} title="Toggle fullscreen (F)" aria-label="Toggle fullscreen">
          {#if isFullscreen}
            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" d="M9 9V4.5M9 9H4.5M9 9L3.75 3.75M9 15v4.5M9 15H4.5M9 15l-5.25 5.25M15 9h4.5M15 9V4.5M15 9l5.25-5.25M15 15h4.5M15 15v4.5m0-4.5l5.25 5.25" />
            </svg>
          {:else}
            <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
              <path stroke-linecap="round" stroke-linejoin="round" d="M3.75 3.75v4.5m0-4.5h4.5m-4.5 0L9 9M3.75 20.25v-4.5m0 4.5h4.5m-4.5 0L9 15M20.25 3.75h-4.5m4.5 0v4.5m0-4.5L15 9m5.25 11.25h-4.5m4.5 0v-4.5m0 4.5L15 15" />
            </svg>
          {/if}
        </button>
        <button onclick={close} title="Close (Esc)" aria-label="Close">
          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
    </div>
    <div class="screenshare-video-container">
      <!-- svelte-ignore a11y_media_has_caption -->
      <video
        bind:this={videoElement}
        autoplay
        playsinline
        muted={isSelf}
        class="screenshare-video"
      ></video>
    </div>
  </div>
</div>

<style>
  .screenshare-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.85);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: var(--z-top);
  }

  .screenshare-container {
    background: var(--color-surface-elevated);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-lg);
    max-width: 95vw;
    max-height: 95vh;
    display: flex;
    flex-direction: column;
    box-shadow: var(--shadow-lg);
    overflow: hidden;
  }

  .screenshare-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--space-3) var(--space-4);
    background: var(--color-surface-raised);
    border-bottom: 1px solid var(--color-surface-outline);
  }

  .screenshare-title {
    display: flex;
    align-items: baseline;
    gap: var(--space-3);
    min-width: 0;
  }

  .screenshare-header h3 {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: 600;
    color: var(--color-on-surface);
  }

  .audio-note {
    font-size: var(--text-xs);
    color: var(--color-muted);
    white-space: nowrap;
  }

  .screenshare-controls {
    display: flex;
    align-items: center;
    gap: var(--space-1);
  }

  .volume-slider {
    width: 6rem;
    min-height: 0;
    accent-color: var(--color-primary);
    cursor: pointer;
  }

  .volume-slider:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .screenshare-controls button {
    background: transparent;
    border: none;
    color: var(--color-muted);
    cursor: pointer;
    padding: var(--space-2);
    border-radius: var(--radius-sm);
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all var(--transition);
  }

  .screenshare-controls button:hover {
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
  }

  .screenshare-controls button svg {
    width: 1.25rem;
    height: 1.25rem;
  }

  .screenshare-video-container {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    background: #000;
    /* Placeholder size until the stream's dimensions are known, but never so
       tall that it overflows the container's 95vh cap (which clips). */
    min-height: min(400px, 50vh);
  }

  .screenshare-video {
    display: block;
    max-width: 95vw;
    /* The container's 95vh budget minus its border and the header, so a tall
       stream letterboxes inside the modal instead of being clipped. */
    max-height: calc(95vh - var(--screenshare-header-height, 3.5rem) - 2px);
    width: auto;
    height: auto;
    object-fit: contain;
  }

  .screenshare-video:fullscreen {
    width: 100vw;
    height: 100vh;
    max-width: none;
    max-height: none;
    object-fit: contain;
  }
</style>
