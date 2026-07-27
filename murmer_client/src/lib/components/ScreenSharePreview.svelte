<!--
  Screen Share Self-Preview

  A small floating window showing the sharer their own capture, so they can
  verify what the channel actually receives. It only exists while sharing and
  can be hidden entirely (the video element is destroyed, not just visually
  hidden) because re-rendering the captured frames costs CPU/GPU.
-->
<script lang="ts">
  interface Props {
    stream: MediaStream;
    onExpand: () => void;
    onHide: () => void;
  }

  let { stream, onExpand, onHide }: Props = $props();

  let videoElement: HTMLVideoElement | undefined = $state();

  $effect(() => {
    if (videoElement) videoElement.srcObject = stream;
  });
</script>

<div class="preview" role="complementary" aria-label="Your screen share preview">
  <div class="preview-header">
    <span class="preview-dot" aria-hidden="true"></span>
    <span class="preview-title">Your screen</span>
    <div class="preview-actions">
      <button
        class="icon-btn"
        onclick={onExpand}
        title="Expand preview"
        aria-label="Expand screen share preview"
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="15 3 21 3 21 9"/><polyline points="9 21 3 21 3 15"/><line x1="21" y1="3" x2="14" y2="10"/><line x1="3" y1="21" x2="10" y2="14"/></svg>
      </button>
      <button
        class="icon-btn"
        onclick={onHide}
        title="Hide preview (saves CPU)"
        aria-label="Hide screen share preview"
      >
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M17.94 17.94A10.07 10.07 0 0 1 12 20c-7 0-11-8-11-8a18.45 18.45 0 0 1 5.06-5.94M9.9 4.24A9.12 9.12 0 0 1 12 4c7 0 11 8 11 8a18.5 18.5 0 0 1-2.16 3.19m-6.72-1.07a3 3 0 1 1-4.24-4.24"/><line x1="1" y1="1" x2="23" y2="23"/></svg>
      </button>
    </div>
  </div>
  <!-- svelte-ignore a11y_media_has_caption -->
  <video bind:this={videoElement} autoplay playsinline muted class="preview-video"></video>
</div>

<style>
  .preview {
    position: fixed;
    right: var(--space-4);
    bottom: var(--space-4);
    width: 17.5rem;
    max-width: calc(100vw - var(--space-8));
    display: flex;
    flex-direction: column;
    background: var(--color-surface-elevated);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-lg);
    overflow: hidden;
    /* App chrome, not a modal: menus and dialogs must stay on top of it. */
    z-index: var(--z-overlay);
  }

  .preview-header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-1) var(--space-1) var(--space-3);
    background: var(--color-surface-raised);
    border-bottom: 1px solid var(--color-surface-outline);
  }

  .preview-dot {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: var(--color-error);
    flex-shrink: 0;
  }

  .preview-title {
    flex: 1;
    min-width: 0;
    color: var(--color-on-surface-variant);
    font-size: var(--text-xs);
    font-weight: 600;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .preview-actions {
    display: flex;
    gap: var(--space-1);
  }

  /* Compact variant of the shared icon button: the full control height would
     dominate the header of this small floating window. */
  .preview-actions :global(.icon-btn) {
    width: 1.75rem;
    height: 1.75rem;
  }

  .preview-video {
    display: block;
    width: 100%;
    aspect-ratio: 16 / 9;
    background: #000;
    object-fit: contain;
  }
</style>
