<!--
  Root layout for the Murmer desktop client. It initialises the theme store on
  mount and injects shared typography and colour tokens for every page.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { APP_VERSION } from '$lib/version';
  import { theme } from '$lib/stores/theme';

  onMount(() => {
    theme.init();
  });
</script>

<svelte:head>
  <link
    rel="stylesheet"
    href="https://fonts.googleapis.com/css2?family=Plus+Jakarta+Sans:wght@400;500;600;700&display=swap"
  />
</svelte:head>

<style>
:global(:root) {
    color-scheme: dark;
    --font-sans: 'Plus Jakarta Sans', 'Inter', 'Segoe UI', sans-serif;
    --color-surface: #11131a;
    --color-surface-elevated: #161923;
    --color-surface-raised: #1f2330;
    --color-surface-outline: rgba(255, 255, 255, 0.08);
    --color-outline-strong: rgba(255, 255, 255, 0.14);
    --color-primary: #b9a2ff;
    --color-on-primary: #20104d;
    --color-primary-container: #2d235f;
    --color-secondary: #a6b2ff;
    --color-tertiary: #ffb4d0;
    --color-on-surface: #e8ecff;
    --color-on-surface-variant: #bac2d6;
    --color-muted: rgba(232, 236, 255, 0.72);
    --color-success: #4ade80;
    --color-warning: #facc15;
    --color-error: #fb7185;
    --color-overlay: rgba(6, 10, 24, 0.55);
    --shadow-xs: 0 1px 2px rgba(2, 6, 23, 0.25);
    --shadow-sm: 0 8px 16px rgba(11, 15, 30, 0.28);
    --shadow-md: 0 18px 30px rgba(7, 11, 28, 0.32);
    --radius-sm: 12px;
    --radius-md: 16px;
    --radius-lg: 22px;
    --transition: 180ms cubic-bezier(0.2, 0, 0, 1);
    --blur-elevated: saturate(140%) blur(24px);
  }

:global(html[data-theme='light']) {
    color-scheme: light;
    --color-surface: #f6f7ff;
    --color-surface-elevated: #ffffff;
    --color-surface-raised: #eef0ff;
    --color-surface-outline: rgba(15, 23, 42, 0.06);
    --color-outline-strong: rgba(15, 23, 42, 0.12);
    --color-primary: #6151d3;
    --color-on-primary: #ffffff;
    --color-primary-container: #dedbff;
    --color-secondary: #4d6bff;
    --color-tertiary: #d83d73;
    --color-on-surface: #0f172a;
    --color-on-surface-variant: #4b5566;
    --color-muted: rgba(15, 23, 42, 0.64);
    --color-success: #16a34a;
    --color-warning: #f59e0b;
    --color-error: #dc2626;
    --color-overlay: rgba(15, 23, 42, 0.25);
    --shadow-xs: 0 1px 2px rgba(15, 23, 42, 0.06);
    --shadow-sm: 0 10px 24px rgba(15, 23, 42, 0.08);
    --shadow-md: 0 24px 38px rgba(15, 23, 42, 0.12);
    --radius-sm: 12px;
    --radius-md: 16px;
    --radius-lg: 22px;
    --transition: 180ms cubic-bezier(0.2, 0, 0, 1);
    --blur-elevated: saturate(120%) blur(18px);
  }

:global(html),
:global(body) {
    margin: 0;
    padding: 0;
    min-height: 100%;
    background: radial-gradient(circle at 0% 0%, rgba(110, 84, 255, 0.18), transparent 45%),
      radial-gradient(circle at 100% 0%, rgba(233, 94, 166, 0.16), transparent 52%),
      var(--color-surface);
    color: var(--color-on-surface);
    font-family: var(--font-sans);
    font-feature-settings: 'ss01' 1, 'ss03' 1;
    -webkit-font-smoothing: antialiased;
  }

:global(body)::before {
    content: '';
    position: fixed;
    inset: 0;
    pointer-events: none;
    background: linear-gradient(135deg, rgba(133, 119, 255, 0.12), rgba(87, 200, 255, 0.08));
    opacity: 0.7;
    z-index: -1;
  }

:global(button),
:global(input),
:global(textarea),
:global(select) {
    font-family: inherit;
    border-radius: var(--radius-sm);
    transition: var(--transition);
  }

:global(button) {
    cursor: pointer;
    border: none;
    outline: none;
    font-weight: 600;
    letter-spacing: 0.01em;
  }

:global(button:focus-visible),
:global(input:focus-visible),
:global(textarea:focus-visible),
:global(select:focus-visible) {
    outline: 2px solid var(--color-secondary);
    outline-offset: 2px;
  }

:global(input),
:global(textarea) {
    outline: none;
    border: 1px solid var(--color-surface-outline);
    background: color-mix(in srgb, var(--color-surface-elevated) 94%, transparent);
    color: var(--color-on-surface);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.02);
    padding: 0.85rem 1rem;
  }

:global(input:focus),
:global(textarea:focus) {
    border-color: var(--color-primary);
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--color-primary) 18%, transparent);
  }

:global(textarea) {
    resize: vertical;
    min-height: 3rem;
  }

:global(a) {
    color: var(--color-secondary);
    text-decoration: none;
  }

:global(a:hover) {
    text-decoration: underline;
  }

:global(*),
:global(*::before),
:global(*::after) {
    box-sizing: border-box;
  }

  /* Global scrollbar styling */
:global(*) {
    scrollbar-width: thin;
    scrollbar-color: var(--color-primary) transparent;
  }

:global(*::-webkit-scrollbar) {
    width: 0.5rem;
    height: 0.5rem;
  }

:global(*::-webkit-scrollbar-track) {
    background: transparent;
  }

:global(*::-webkit-scrollbar-thumb) {
    background: color-mix(in srgb, var(--color-primary) 60%, transparent);
    border-radius: 999px;
  }

:global(*::-webkit-scrollbar-thumb:hover) {
    background: color-mix(in srgb, var(--color-primary) 80%, transparent);
  }

@media (prefers-reduced-motion: reduce) {
    :global(*) {
      animation-duration: 0.001ms !important;
      animation-iteration-count: 1 !important;
      transition-duration: 0.001ms !important;
      scroll-behavior: auto !important;
    }
  }

  .version {
    position: fixed;
    bottom: 1rem;
    right: 1rem;
    color: var(--color-muted);
    font-size: 0.75rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    backdrop-filter: var(--blur-elevated);
    background: color-mix(in srgb, var(--color-surface-raised) 85%, transparent);
    padding: 0.35rem 0.8rem;
    border-radius: 999px;
    box-shadow: var(--shadow-xs);
    border: 1px solid var(--color-surface-outline);
  }
</style>

<slot />

<div class="version">{APP_VERSION}</div>
