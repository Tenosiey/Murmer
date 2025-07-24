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
    --color-bg: #0f0f23;
    --color-bg-elevated: #1a1a3a;
    --color-panel: #252540;
    --color-panel-elevated: #2a2a50;
    --color-border: #3a3a5a;
    --color-border-subtle: #2a2a45;
    --color-accent: #7c3aed;
    --color-accent-alt: #2563eb;
    --color-accent-hover: #8b5cf6;
    --color-text: #e2e8f0;
    --color-text-muted: #94a3b8;
    --color-text-subtle: #64748b;
    --color-success: #10b981;
    --color-warning: #f59e0b;
    --color-error: #ef4444;
    --color-overlay: rgba(0, 0, 0, 0.5);
    --shadow-sm: 0 1px 2px 0 rgba(0, 0, 0, 0.3);
    --shadow-md: 0 4px 6px -1px rgba(0, 0, 0, 0.4);
    --shadow-lg: 0 10px 15px -3px rgba(0, 0, 0, 0.5);
    --radius-sm: 6px;
    --radius-md: 8px;
    --radius-lg: 12px;
    --transition: all 0.2s cubic-bezier(0.4, 0, 0.2, 1);
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
    border-radius: var(--radius-sm);
    transition: var(--transition);
  }

  :global(button) {
    cursor: pointer;
    border: none;
    outline: none;
    font-weight: 500;
  }

  :global(button:focus-visible) {
    outline: 2px solid var(--color-accent);
    outline-offset: 2px;
  }

  :global(input),
  :global(textarea) {
    outline: none;
    border: 1px solid var(--color-border-subtle);
    background: var(--color-panel);
    color: var(--color-text);
  }

  :global(input:focus),
  :global(textarea:focus) {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 3px rgba(124, 58, 237, 0.1);
  }

  :global(a) {
    color: var(--color-accent-alt);
  }

  /* Global scrollbar styling */
  :global(*) {
    scrollbar-width: thin;
    scrollbar-color: var(--color-border) var(--color-panel);
  }

  :global(*::-webkit-scrollbar) {
    width: 6px;
    height: 6px;
  }

  :global(*::-webkit-scrollbar-track) {
    background: var(--color-panel);
    border-radius: var(--radius-sm);
  }

  :global(*::-webkit-scrollbar-thumb) {
    background: var(--color-border);
    border-radius: var(--radius-sm);
    transition: var(--transition);
  }

  :global(*::-webkit-scrollbar-thumb:hover) {
    background: var(--color-accent);
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
