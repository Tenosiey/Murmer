<script lang="ts">
  const version = '2025.7.9-alpha.1';
</script>

<style global>
  @import url('https://fonts.googleapis.com/css2?family=Inter:wght@400;600&family=Roboto:wght@400;500&display=swap');

  :root {
    --color-bg: #1e1e2e;
    --color-panel: #252537;
    --color-accent: #7c3aed;
    --color-accent-alt: #2563eb;
    --color-text: #e2e8f0;
  }

  html, body {
    margin: 0;
    padding: 0;
    height: 100%;
    background: var(--color-bg);
    color: var(--color-text);
    font-family: 'Inter', 'Roboto', sans-serif;
  }

  button, input, textarea {
    font-family: inherit;
    border-radius: 4px;
  }

  a {
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

<div class="version">{version}</div>
