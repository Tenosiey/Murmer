<script lang="ts">
  import { VERSION } from '$lib/version';
</script>

<style>
  @import url('https://fonts.googleapis.com/css2?family=Inter:wght@400;600&family=Roboto:wght@400;500&display=swap');

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

  .version {
    position: fixed;
    bottom: 0.5rem;
    right: 0.5rem;
    color: #aaa;
    font-size: 0.8rem;
  }
</style>

<slot />

<div class="version">{VERSION}</div>
