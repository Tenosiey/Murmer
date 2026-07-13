<script lang="ts">
  import { tick } from 'svelte';
  import { HELP_COMMANDS } from '$lib/chat/constants';
  import { hotkeys, HOTKEY_ACTIONS, formatCombo } from '$lib/stores/hotkeys';

  $: boundActions = HOTKEY_ACTIONS.filter((action) => $hotkeys[action.id]);

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
      {#if boundActions.length > 0}
        <div class="help-header">
          <h2>Hotkeys</h2>
          <p class="help-description">Customizable under Settings → Hotkeys.</p>
        </div>
        <ul class="help-command-list help-hotkey-list">
          {#each boundActions as action (action.id)}
            <li class="help-hotkey">
              <code class="help-command-usage">{formatCombo($hotkeys[action.id])}</code>
              <span class="help-command-description">{action.label}</span>
            </li>
          {/each}
        </ul>
      {/if}
      <button type="button" class="btn help-close" on:click={onClose} bind:this={closeButton}>
        Close
      </button>
    </div>
  </div>
{/if}

<style>
  .help-overlay {
    position: fixed;
    inset: 0;
    background: var(--color-overlay);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-5);
    z-index: var(--z-overlay);
  }

  .help-panel {
    width: min(640px, 92vw);
    max-height: min(70vh, 640px);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    background: var(--color-surface-elevated);
    border-radius: var(--radius-lg);
    border: 1px solid var(--color-surface-outline);
    box-shadow: var(--shadow-lg);
    padding: var(--space-5);
  }

  .help-header {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .help-header h2 {
    font-size: var(--text-lg);
  }

  .help-description {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-sm);
  }

  .help-command-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    overflow-y: auto;
  }

  .help-command {
    padding: var(--space-3) 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }

  .help-command + .help-command {
    border-top: 1px solid var(--color-surface-outline);
  }

  .help-command-heading {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: center;
  }

  .help-command-usage {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    background: var(--color-surface-raised);
    border-radius: var(--radius-xs);
    padding: 0.125rem var(--space-2);
  }

  .help-command-aliases {
    color: var(--color-muted);
    font-size: var(--text-xs);
  }

  .help-command-description {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-sm);
  }

  .help-hotkey-list {
    flex-shrink: 0;
  }

  .help-hotkey {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) 0;
  }

  .help-hotkey + .help-hotkey {
    border-top: 1px solid var(--color-surface-outline);
  }

  .help-close {
    align-self: flex-end;
  }
</style>
