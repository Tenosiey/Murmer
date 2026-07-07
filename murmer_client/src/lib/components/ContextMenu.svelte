<!--
  Modern floating context menu with smooth animations and micro-interactions.
  Accepts a list of menu items and exposes coordinates for positioning relative
  to the user's cursor. Features fade-in/scale animation and keyboard navigation.
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { fly, scale } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  
  export let items: { label: string; action: () => void; danger?: boolean; icon?: string }[] = [];
  export let x = 0;
  export let y = 0;
  export let open = false;

  let menuElement: HTMLUListElement;
  let mounted = false;

  function close() {
    open = false;
  }

  function handleClickOutside(event: MouseEvent) {
    if (menuElement && !(event.target as HTMLElement).closest('.menu')) {
      close();
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      close();
    }
  }

  onMount(() => {
    mounted = true;
    document.addEventListener('click', handleClickOutside);
    document.addEventListener('contextmenu', handleClickOutside);
    document.addEventListener('keydown', handleKeydown);
  });
  
  onDestroy(() => {
    document.removeEventListener('click', handleClickOutside);
    document.removeEventListener('contextmenu', handleClickOutside);
    document.removeEventListener('keydown', handleKeydown);
  });

  // Ensure menu doesn't go off screen
  $: adjustedX = x;
  $: adjustedY = y;
  $: if (menuElement && mounted) {
    const rect = menuElement.getBoundingClientRect();
    const viewportWidth = window.innerWidth;
    const viewportHeight = window.innerHeight;
    
    if (rect.right > viewportWidth) {
      adjustedX = x - (rect.right - viewportWidth) - 8;
    }
    if (rect.bottom > viewportHeight) {
      adjustedY = y - (rect.bottom - viewportHeight) - 8;
    }
  }
</script>

{#if open}
  <ul
    bind:this={menuElement}
    class="menu"
    style="top:{adjustedY}px;left:{adjustedX}px"
    transition:fly={{ y: -8, duration: 180, easing: cubicOut }}
    role="menu"
  >
    {#each items as item, index (index)}
      <li role="none">
        <button
          type="button"
          class="entry"
          class:entry-danger={item.danger}
          on:click={() => {
            item.action();
            close();
          }}
          role="menuitem"
          tabindex="0"
        >
          {#if item.icon}
            <span class="entry-icon" aria-hidden="true">{item.icon}</span>
          {/if}
          <span class="entry-label">{item.label}</span>
        </button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .menu {
    position: fixed;
    background: var(--color-surface-elevated);
    border: 1px solid var(--color-surface-outline);
    padding: var(--space-1);
    z-index: var(--z-modal);
    list-style: none;
    margin: 0;
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-md);
    min-width: 200px;
    max-height: 70vh;
    overflow-y: auto;
  }

  .entry {
    padding: var(--space-2) var(--space-3);
    cursor: pointer;
    white-space: nowrap;
    background: none;
    border: none;
    border-radius: var(--radius-xs);
    color: var(--color-on-surface);
    width: 100%;
    text-align: left;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-weight: 400;
    font-size: var(--text-md);
  }

  .entry:hover,
  .entry:focus-visible {
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
  }

  .entry-danger {
    color: var(--color-error);
  }

  .entry-danger:hover,
  .entry-danger:focus-visible {
    background: color-mix(in srgb, var(--color-error) 14%, transparent);
    color: var(--color-error);
  }

  .entry-icon {
    font-size: var(--text-md);
    flex-shrink: 0;
  }

  .entry-label {
    flex: 1;
  }

  .entry:focus-visible {
    outline: none;
  }
</style>
