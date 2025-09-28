<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  export let items: { label: string; action: () => void }[] = [];
  export let x = 0;
  export let y = 0;
  export let open = false;

  function close() {
    open = false;
  }

  function handleClickOutside(event: MouseEvent) {
    if (!(event.target as HTMLElement).closest('.menu')) {
      close();
    }
  }

  onMount(() => {
    document.addEventListener('click', handleClickOutside);
    document.addEventListener('contextmenu', handleClickOutside);
  });
  onDestroy(() => {
    document.removeEventListener('click', handleClickOutside);
    document.removeEventListener('contextmenu', handleClickOutside);
  });
</script>

{#if open}
  <ul class="menu" style="top:{y}px;left:{x}px">
    {#each items as item}
      <li>
        <button
          type="button"
          class="entry"
          on:click={() => {
            item.action();
            close();
          }}
        >
          {item.label}
        </button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .menu {
    position: fixed;
    background: color-mix(in srgb, var(--color-surface-elevated) 92%, transparent);
    border: 1px solid var(--color-surface-outline);
    padding: 0.35rem 0;
    z-index: 1000;
    list-style: none;
    margin: 0;
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-md);
    backdrop-filter: var(--blur-elevated);
    min-width: 200px;
  }

  .entry {
    padding: 0.55rem 1rem;
    cursor: pointer;
    white-space: nowrap;
    background: none;
    border: none;
    color: var(--color-on-surface);
    width: 100%;
    text-align: left;
    display: block;
    font-weight: 500;
    letter-spacing: 0.01em;
  }

  .entry:hover,
  .entry:focus-visible {
    background: color-mix(in srgb, var(--color-primary) 16%, transparent);
    color: var(--color-on-primary);
  }
</style>
