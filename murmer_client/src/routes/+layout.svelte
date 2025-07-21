<script lang="ts">
  import { APP_VERSION } from '$lib/version';
</script>

<svelte:head>
  <link
    rel="stylesheet"
    href="https://fonts.googleapis.com/css2?family=Inter:wght@400;600&family=Roboto:wght@400;500&display=swap"
  />
</svelte:head>

<style>

  :global(:root) {
    --color-bg: #1e1e2e;
    --color-panel: #252537;
    --color-accent: #7c3aed;
    --color-accent-alt: #2563eb;
    --color-text: #e2e8f0;
  }

  :global(html),
  :global(body) {
    margin: 0;
    padding: 0;
    height: 100%;
    background: var(--color-bg);
    color: var(--color-text);
    font-family: 'Inter', 'Roboto', sans-serif;
  }

  :global(button),
  :global(input),
  :global(textarea) {
    font-family: inherit;
    border-radius: 4px;
  }

  :global(a) {
    color: var(--color-accent-alt);
  }

  /* Global scrollbar styling */
  :global(*) {
    scrollbar-width: thin;
    scrollbar-color: var(--color-accent) var(--color-panel);
  }

  :global(*::-webkit-scrollbar) {
    width: 8px;
    height: 8px;
  }

  :global(*::-webkit-scrollbar-track) {
    background: var(--color-panel);
  }

  :global(*::-webkit-scrollbar-thumb) {
    background: var(--color-accent);
    border-radius: 4px;
  }

  :global(*::-webkit-scrollbar-thumb:hover) {
    background: var(--color-accent-alt);
  }

  .version {
    position: fixed;
    bottom: 0.5rem;
    right: 0.5rem;
    color: #aaa;
    font-size: 0.8rem;
  }
</style>

<slot />

<div class="version">{APP_VERSION}</div>
