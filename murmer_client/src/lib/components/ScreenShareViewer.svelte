<!--
  Screen Share Viewer Component
  
  Displays a remote user's screen share in a modal window with controls.
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import type { ScreenSharePeer } from '$lib/types';
  import { stopViewingScreenShare } from '$lib/stores/screenShare';

  export let peer: ScreenSharePeer;
  export let onClose: () => void;

  let videoElement: HTMLVideoElement;
  let isFullscreen = false;

  onMount(() => {
    if (videoElement && peer.stream) {
      videoElement.srcObject = peer.stream;
    }
  });

  onDestroy(() => {
    stopViewingScreenShare(peer.userId);
  });

  function close() {
    stopViewingScreenShare(peer.userId);
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

<svelte:window on:keydown={handleKeydown} />

<!-- svelte-ignore a11y-click-events-have-key-events -->
<!-- svelte-ignore a11y-no-static-element-interactions -->
<div class="screenshare-overlay" on:click={close}>
  <div class="screenshare-container" on:click={(e) => e.stopPropagation()} role="dialog" aria-label="Screen share viewer" tabindex="-1">
    <div class="screenshare-header">
      <h3>{peer.userId}'s Screen</h3>
      <div class="screenshare-controls">
        <button on:click={toggleFullscreen} title="Toggle fullscreen (F)" aria-label="Toggle fullscreen">
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
        <button on:click={close} title="Close (Esc)" aria-label="Close">
          <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
            <path stroke-linecap="round" stroke-linejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      </div>
    </div>
    <div class="screenshare-video-container">
      <!-- svelte-ignore a11y-media-has-caption -->
      <video
        bind:this={videoElement}
        autoplay
        playsinline
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
    z-index: 10000;
    backdrop-filter: blur(4px);
  }

  .screenshare-container {
    background: var(--bg-primary, #1e1e1e);
    border-radius: 12px;
    max-width: 95vw;
    max-height: 95vh;
    display: flex;
    flex-direction: column;
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.5);
    overflow: hidden;
  }

  .screenshare-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1rem 1.5rem;
    background: var(--bg-secondary, #2a2a2a);
    border-bottom: 1px solid var(--border-color, #404040);
  }

  .screenshare-header h3 {
    margin: 0;
    font-size: 1.1rem;
    font-weight: 600;
    color: var(--text-primary, #ffffff);
  }

  .screenshare-controls {
    display: flex;
    gap: 0.5rem;
  }

  .screenshare-controls button {
    background: transparent;
    border: none;
    color: var(--text-secondary, #b0b0b0);
    cursor: pointer;
    padding: 0.5rem;
    border-radius: 6px;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: all 0.2s;
  }

  .screenshare-controls button:hover {
    background: var(--bg-hover, #3a3a3a);
    color: var(--text-primary, #ffffff);
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
    min-height: 400px;
  }

  .screenshare-video {
    width: 100%;
    height: 100%;
    object-fit: contain;
  }

  .screenshare-video:fullscreen {
    object-fit: contain;
  }
</style>
