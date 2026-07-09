<!--
  Compact emoji popover for message reactions. Opens at the given cursor
  coordinates (clamped to the viewport, like ContextMenu), offers a curated
  grid of common reactions plus a free-form input for anything else.
-->
<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import { fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';

  export let open = false;
  export let x = 0;
  export let y = 0;
  export let onPick: (emoji: string) => void;
  export let onClose: () => void;

  const EMOJI = [
    '👍', '👎', '❤️', '😂', '😮', '😢', '🎉', '🔥',
    '👀', '🤔', '✅', '❌', '🙏', '💯', '🚀', '🫡'
  ];

  let panel: HTMLDivElement | null = null;
  let custom = '';
  let adjustedX = 0;
  let adjustedY = 0;

  $: if (open) {
    adjustedX = x;
    adjustedY = y;
    custom = '';
    tick().then(() => {
      if (!panel) return;
      const rect = panel.getBoundingClientRect();
      if (rect.right > window.innerWidth) adjustedX = Math.max(8, x - rect.width);
      if (rect.bottom > window.innerHeight) adjustedY = Math.max(8, y - rect.height);
      panel.querySelector('button')?.focus();
    });
  }

  function pick(emoji: string) {
    const trimmed = emoji.trim();
    if (!trimmed) return;
    onPick(trimmed);
    onClose();
  }

  function handleClickOutside(event: MouseEvent) {
    if (open && panel && !panel.contains(event.target as Node)) onClose();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (open && event.key === 'Escape') {
      event.stopPropagation();
      onClose();
    }
  }

  onMount(() => {
    document.addEventListener('mousedown', handleClickOutside);
    document.addEventListener('keydown', handleKeydown, true);
  });

  onDestroy(() => {
    document.removeEventListener('mousedown', handleClickOutside);
    document.removeEventListener('keydown', handleKeydown, true);
  });
</script>

{#if open}
  <div
    bind:this={panel}
    class="picker menu-panel"
    style="top:{adjustedY}px;left:{adjustedX}px"
    role="dialog"
    aria-label="Add reaction"
    transition:fly={{ y: -6, duration: 140, easing: cubicOut }}
  >
    <div class="grid">
      {#each EMOJI as emoji (emoji)}
        <button type="button" class="emoji" on:click={() => pick(emoji)} title={`React with ${emoji}`}>
          {emoji}
        </button>
      {/each}
    </div>
    <form class="custom" on:submit|preventDefault={() => pick(custom)}>
      <input
        bind:value={custom}
        type="text"
        placeholder="Custom emoji…"
        maxlength="8"
        aria-label="Custom emoji"
      />
    </form>
  </div>
{/if}

<style>
  .picker {
    position: fixed;
    z-index: var(--z-modal);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-2);
    width: 232px;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(8, 1fr);
    gap: 2px;
  }

  .emoji {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.625rem;
    height: 1.625rem;
    padding: 0;
    border: none;
    border-radius: var(--radius-xs);
    background: transparent;
    font-size: var(--text-lg);
    line-height: 1;
  }

  .emoji:hover {
    background: var(--color-surface-raised);
  }

  .custom input {
    width: 100%;
    min-height: var(--control-height);
    font-size: var(--text-sm);
  }
</style>
