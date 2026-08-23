<!--
  Video stage: every camera on in the voice channel, as one grid above the
  chat. Unlike a screen share this is not a window to arrange — a camera is a
  face next to the conversation, so the tiles stay in the flow and the stage
  disappears entirely the moment the last camera goes off.
-->
<script lang="ts">
  import { displayNames } from '$lib/stores/profiles';
  import { speakingUsers } from '$lib/stores/voiceSpeaking';
  import { voiceMuteStates } from '$lib/stores/voiceMute';
  import { microphoneMuted } from '$lib/stores/settings';
  import { mirrorSelfView } from '$lib/stores/webcam';
  import type { WebcamTile } from '$lib/types';

  interface Props {
    /** The cameras to show, our own preview first. */
    tiles: WebcamTile[];
  }

  let { tiles }: Props = $props();

  // Collapsed the stage still says how many cameras are on, so nobody
  // wonders why a member is flagged as on camera with nothing to see.
  let collapsed = $state(false);

  /**
   * Point a video element at a stream, and only at a *different* one:
   * re-assigning `srcObject` restarts playback from black.
   */
  function attachStream(node: HTMLVideoElement, stream: MediaStream | null) {
    const apply = (next: MediaStream | null) => {
      if (node.srcObject !== next) node.srcObject = next ?? null;
    };
    apply(stream);
    return {
      update: apply,
      destroy: () => {
        node.srcObject = null;
      }
    };
  }

  /** Whether this member's microphone is muted, ours included. */
  function micMuted(tile: WebcamTile): boolean {
    if (tile.isSelf) return $microphoneMuted;
    return $voiceMuteStates[tile.userId]?.micMuted ?? false;
  }

  function toggleFullscreen(event: MouseEvent) {
    const tile = (event.currentTarget as HTMLElement).closest('.tile');
    const video = tile?.querySelector('video');
    if (!video) return;
    if (document.fullscreenElement === video) document.exitFullscreen?.();
    else video.requestFullscreen?.();
  }
</script>

<section class="stage" aria-label="Cameras">
  <div class="stage-bar">
    <button
      class="collapse"
      onclick={() => (collapsed = !collapsed)}
      aria-expanded={!collapsed}
      title={collapsed ? 'Show cameras' : 'Hide cameras'}
    >
      <svg
        width="14"
        height="14"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        stroke-width="1.8"
        stroke-linecap="round"
        stroke-linejoin="round"
        aria-hidden="true"
        class:rotated={collapsed}
      >
        <polyline points="6 9 12 15 18 9" />
      </svg>
      <span>{tiles.length} {tiles.length === 1 ? 'camera' : 'cameras'}</span>
    </button>
  </div>

  {#if !collapsed}
    <div class="grid">
      {#each tiles as tile (tile.key)}
        {@const talking = Boolean($speakingUsers[tile.userId]) && !micMuted(tile)}
        <div class="tile" class:talking class:mirrored={tile.isSelf && $mirrorSelfView}>
          {#if tile.stream}
            <!-- svelte-ignore a11y_media_has_caption -->
            <!-- Always muted: the voice arrives on the peer's audio element,
                 and a second sink for the same person would double it. -->
            <video autoplay playsinline muted use:attachStream={tile.stream}></video>
          {:else}
            <div class="pending" role="status">
              <span class="spinner" aria-hidden="true"></span>
              <span>Connecting…</span>
            </div>
          {/if}

          <button class="expand" onclick={toggleFullscreen} title="Fullscreen" aria-label="Fullscreen">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="15 3 21 3 21 9"/><polyline points="9 21 3 21 3 15"/><line x1="21" y1="3" x2="14" y2="10"/><line x1="3" y1="21" x2="10" y2="14"/></svg>
          </button>

          <div class="label">
            {#if micMuted(tile)}
              <span class="mute-icon" title="Microphone muted" aria-label="Microphone muted">
                <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><line x1="2" y1="2" x2="22" y2="22"/><path d="M18.89 13.23A7.12 7.12 0 0 0 19 12v-2"/><path d="M5 10v2a7 7 0 0 0 12 5"/><path d="M15 9.34V5a3 3 0 0 0-5.68-1.33"/><path d="M9 9v3a3 3 0 0 0 5.12 2.12"/><line x1="12" y1="19" x2="12" y2="22"/></svg>
              </span>
            {/if}
            <span class="name">{tile.isSelf ? 'You' : $displayNames(tile.userId)}</span>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</section>

<style>
  .stage {
    flex-shrink: 0;
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--color-surface-outline);
    background: var(--color-surface);
  }

  .stage-bar {
    display: flex;
    align-items: center;
    margin-bottom: var(--space-2);
  }

  .collapse {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    padding: 0 var(--space-1);
    background: transparent;
    border: none;
    color: var(--color-muted);
    font-size: var(--text-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    cursor: pointer;
  }

  .collapse:hover {
    color: var(--color-on-surface);
  }

  .collapse svg {
    transition: transform 0.15s ease;
  }

  .collapse svg.rotated {
    transform: rotate(-90deg);
  }

  .grid {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    /* Roughly two rows before it scrolls, so a channel full of cameras cannot
       push the conversation off screen. */
    max-height: 19rem;
    overflow-y: auto;
  }

  .tile {
    position: relative;
    flex: 0 0 auto;
    /* A fixed height with a 16:9 width, rather than a grid column that fills
       whatever is left: a stretched tile made one camera 500px tall in a wide
       window and left the chat a sliver. The stage now grows by whole rows. */
    height: clamp(6rem, 13vh, 9rem);
    aspect-ratio: 16 / 9;
    overflow: hidden;
    border: 2px solid transparent;
    border-radius: var(--radius-md);
    background: var(--color-bg);
  }

  .tile.talking {
    border-color: var(--color-primary);
  }

  .tile video {
    width: 100%;
    height: 100%;
    /* Cover rather than contain: a webcam is nearly always the shape of the
       tile, and letterboxing every face to avoid the rare crop looks worse. */
    object-fit: cover;
    display: block;
  }

  .tile.mirrored video {
    transform: scaleX(-1);
  }

  .pending {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    width: 100%;
    height: 100%;
    color: var(--color-muted);
    font-size: var(--text-xs);
  }

  .spinner {
    width: 1rem;
    height: 1rem;
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

  .label {
    position: absolute;
    left: var(--space-1);
    bottom: var(--space-1);
    display: flex;
    align-items: center;
    gap: var(--space-1);
    max-width: calc(100% - var(--space-2));
    padding: 0 var(--space-1);
    border-radius: var(--radius-sm);
    /* The one place a literal color is unavoidable: this sits on top of a
       video frame, not on a themed surface, and has to stay legible over a
       bright one. */
    background: rgb(0 0 0 / 0.55);
    color: #fff;
    font-size: var(--text-xs);
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .mute-icon {
    display: inline-flex;
    color: var(--color-error);
  }

  .expand {
    position: absolute;
    top: var(--space-1);
    right: var(--space-1);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-1);
    border: none;
    border-radius: var(--radius-sm);
    background: rgb(0 0 0 / 0.45);
    color: #fff;
    opacity: 0;
    cursor: pointer;
  }

  .tile:hover .expand,
  .expand:focus-visible {
    opacity: 1;
  }
</style>
