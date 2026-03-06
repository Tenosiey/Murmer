<script lang="ts">
  import { tick } from 'svelte';
  import { HELP_COMMANDS } from '$lib/chat/constants';

  export let open: boolean;
  export let onClose: () => void;

  let panel: HTMLDivElement | null = null;
  let closeButton: HTMLButtonElement | null = null;

  export function focusPanel() {
    tick().then(() => {
      if (panel) panel.focus();
      else if (closeButton) closeButton.focus();
    });
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      event.preventDefault();
      onClose();
    }
  }

  function handleOverlayKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' || event.key === ' ') {
      event.preventDefault();
      onClose();
    }
  }
</script>

{#if open}
  <div
    class="help-overlay"
    role="button"
    tabindex="0"
    aria-label="Close command reference"
    on:click={onClose}
    on:keydown={handleOverlayKeydown}
  >
    <div
      class="help-panel"
      role="dialog"
      aria-modal="true"
      aria-labelledby="help-title"
      tabindex="-1"
      bind:this={panel}
      on:click|stopPropagation
      on:keydown={handleKeydown}
    >
      <div class="help-header">
        <h2 id="help-title">Slash commands</h2>
        <p class="help-description">Type a forward slash to run these quick actions.</p>
      </div>
      <ul class="help-command-list">
        {#each HELP_COMMANDS as command (command.usage)}
          <li class="help-command">
            <div class="help-command-heading">
              <code class="help-command-usage">{command.usage}</code>
              {#if command.aliases?.length}
                <span class="help-command-aliases">Also: {command.aliases.join(', ')}</span>
              {/if}
            </div>
            <p class="help-command-description">{command.description}</p>
          </li>
        {/each}
      </ul>
      <button type="button" class="help-close" on:click={onClose} bind:this={closeButton}>
        Close
      </button>
    </div>
  </div>
{/if}

<style>
  .help-overlay {
    position: fixed;
    inset: 0;
    background: color-mix(in srgb, var(--color-overlay) 85%, transparent);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: clamp(1.5rem, 4vw, 3rem);
    z-index: 60;
  }

  .help-panel {
    width: min(720px, 92vw);
    max-height: min(70vh, 640px);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
    background: color-mix(in srgb, var(--color-surface-elevated) 96%, transparent);
    border-radius: var(--radius-lg);
    border: 1px solid var(--color-surface-outline);
    box-shadow: var(--shadow-lg);
    padding: clamp(1rem, 3vw, 1.75rem);
  }

  .help-header {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .help-description {
    color: var(--color-muted);
    font-size: 0.95rem;
  }

  .help-command-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    overflow-y: auto;
  }

  .help-command {
    border-radius: var(--radius-md);
    border: 1px solid color-mix(in srgb, var(--color-primary) 14%, transparent);
    background: color-mix(in srgb, var(--color-surface-elevated) 92%, transparent);
    padding: 0.75rem 0.9rem;
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
  }

  .help-command-heading {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    align-items: center;
  }

  .help-command-usage {
    font-family: var(--font-mono);
    background: color-mix(in srgb, var(--color-primary) 12%, transparent);
    border-radius: var(--radius-sm);
    padding: 0.2rem 0.4rem;
  }

  .help-command-aliases {
    color: var(--color-muted);
    font-size: 0.85rem;
  }

  .help-command-description {
    margin: 0;
    color: var(--color-muted);
    font-size: 0.95rem;
  }

  .help-close {
    align-self: flex-end;
    padding: 0.55rem 0.9rem;
    border-radius: var(--radius-md);
    border: 1px solid color-mix(in srgb, var(--color-muted) 24%, transparent);
    background: color-mix(in srgb, var(--color-muted) 18%, transparent);
    color: var(--color-on-surface);
    font-weight: 600;
    transition: transform var(--transition);
  }

  .help-close:hover {
    transform: translateY(-1px);
  }
</style>
