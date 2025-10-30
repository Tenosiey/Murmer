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
    background: color-mix(in srgb, var(--md-sys-color-surface-container-high) 96%, transparent);
    border: 1px solid var(--md-sys-color-outline);
    padding: 0.45rem;
    z-index: 1200;
    list-style: none;
    margin: 0;
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-03);
    backdrop-filter: var(--blur-elevated);
    min-width: 220px;
  }

  .entry {
    padding: 0.7rem 1rem;
    cursor: pointer;
    white-space: nowrap;
    background: none;
    border: none;
    border-radius: var(--radius-sm);
    color: var(--md-sys-color-on-surface);
    width: 100%;
    text-align: left;
    display: flex;
    align-items: center;
    gap: 0.75rem;
    font-weight: 500;
    letter-spacing: 0.01em;
    transition: all var(--motion-duration-short) var(--motion-easing-standard);
    position: relative;
    overflow: hidden;
  }

  .entry::before {
    content: '';
    position: absolute;
    left: 0;
    top: 50%;
    transform: translateY(-50%);
    width: 3px;
    height: 0;
    background: linear-gradient(180deg, var(--md-sys-color-primary), var(--md-sys-color-secondary));
    border-radius: 0 999px 999px 0;
    transition: height var(--motion-duration-short) var(--motion-easing-standard);
  }

  .entry:hover::before,
  .entry:focus-visible::before {
    height: 70%;
  }

  .entry:hover,
  .entry:focus-visible {
    background: color-mix(in srgb, var(--md-sys-color-primary) 14%, transparent);
    color: var(--md-sys-color-on-surface);
    padding-left: 1.2rem;
  }

  .entry:active {
    transform: scale(0.98);
  }

  .entry-danger {
    color: var(--md-sys-color-error);
  }

  .entry-danger::before {
    background: var(--md-sys-color-error);
  }

  .entry-danger:hover,
  .entry-danger:focus-visible {
    background: color-mix(in srgb, var(--md-sys-color-error) 12%, transparent);
    color: var(--md-sys-color-error);
  }

  .entry-icon {
    font-size: 1.1rem;
    flex-shrink: 0;
  }

  .entry-label {
    flex: 1;
  }

  .entry:focus-visible {
    outline: 2px solid color-mix(in srgb, var(--md-sys-color-secondary) 45%, transparent);
    outline-offset: -2px;
  }
</style>
