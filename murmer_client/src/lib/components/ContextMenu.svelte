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
      <li class="entry" on:click={() => { item.action(); close(); }}>{item.label}</li>
    {/each}
  </ul>
{/if}

<style>
  .menu {
    position: fixed;
    background: var(--color-panel);
    border: 1px solid #4b5563;
    padding: 0.25rem 0;
    z-index: 1000;
  }

  .entry {
    padding: 0.25rem 1rem;
    cursor: pointer;
    white-space: nowrap;
  }
  .entry:hover {
    background: #374151;
  }
</style>
