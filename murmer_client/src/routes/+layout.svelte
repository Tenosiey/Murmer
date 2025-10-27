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
    --font-mono: 'JetBrains Mono', 'Fira Code', 'Menlo', monospace;
    --md-sys-color-primary: #c4b5ff;
    --md-sys-color-on-primary: #20104d;
    --md-sys-color-primary-container: #3a2c70;
    --md-sys-color-on-primary-container: #ede8ff;
    --md-sys-color-secondary: #8bd3ff;
    --md-sys-color-on-secondary: #002639;
    --md-sys-color-tertiary: #ffb3c8;
    --md-sys-color-on-tertiary: #3c001d;
    --md-sys-color-error: #ffb4ab;
    --md-sys-color-on-error: #370001;
    --md-sys-color-surface: #0f1118;
    --md-sys-color-surface-dim: #0a0c12;
    --md-sys-color-surface-bright: #1f2230;
    --md-sys-color-surface-container-lowest: #07080d;
    --md-sys-color-surface-container-low: #14171f;
    --md-sys-color-surface-container: #1b1e28;
    --md-sys-color-surface-container-high: #232633;
    --md-sys-color-surface-container-highest: #2b2f3d;
    --md-sys-color-outline: rgba(218, 225, 255, 0.12);
    --md-sys-color-outline-variant: rgba(218, 225, 255, 0.2);
    --md-sys-color-shadow: rgba(7, 9, 18, 0.65);
    --md-sys-color-on-surface: #e8ecff;
    --md-sys-color-on-surface-variant: #bbc4d8;
    --md-sys-color-muted: rgba(227, 233, 255, 0.72);
    --md-sys-color-success: #4ade80;
    --md-sys-color-warning: #facc15;
    --motion-duration-short: 140ms;
    --motion-duration-medium: 220ms;
    --motion-easing-standard: cubic-bezier(0.2, 0, 0, 1);
    --radius-xs: 8px;
    --radius-sm: 12px;
    --radius-md: 16px;
    --radius-lg: 22px;
    --shadow-01: 0 1px 2px rgba(10, 13, 24, 0.25);
    --shadow-02: 0 12px 32px rgba(11, 15, 30, 0.28);
    --shadow-03: 0 28px 48px rgba(7, 11, 28, 0.32);
    --blur-elevated: saturate(140%) blur(26px);

    /* Legacy aliases for existing components */
    --color-surface: var(--md-sys-color-surface);
    --color-surface-elevated: var(--md-sys-color-surface-container);
    --color-surface-raised: var(--md-sys-color-surface-container-high);
    --color-surface-outline: var(--md-sys-color-outline);
    --color-outline-strong: var(--md-sys-color-outline-variant);
    --color-primary: var(--md-sys-color-primary);
    --color-on-primary: var(--md-sys-color-on-primary);
    --color-primary-container: var(--md-sys-color-primary-container);
    --color-secondary: var(--md-sys-color-secondary);
    --color-tertiary: var(--md-sys-color-tertiary);
    --color-on-surface: var(--md-sys-color-on-surface);
    --color-on-surface-variant: var(--md-sys-color-on-surface-variant);
    --color-muted: var(--md-sys-color-muted);
    --color-success: var(--md-sys-color-success);
    --color-warning: var(--md-sys-color-warning);
    --color-error: var(--md-sys-color-error);
    --color-overlay: rgba(12, 17, 30, 0.58);
    --shadow-xs: var(--shadow-01);
    --shadow-sm: var(--shadow-02);
    --shadow-md: var(--shadow-03);
    --transition: var(--motion-duration-short) var(--motion-easing-standard);
  }

  :global(html[data-theme='light']) {
    color-scheme: light;
    --md-sys-color-primary: #6550db;
    --md-sys-color-on-primary: #ffffff;
    --md-sys-color-primary-container: #e6deff;
    --md-sys-color-on-primary-container: #21005d;
    --md-sys-color-secondary: #3864ff;
    --md-sys-color-on-secondary: #ffffff;
    --md-sys-color-tertiary: #b32672;
    --md-sys-color-on-tertiary: #ffffff;
    --md-sys-color-error: #ba1a1a;
    --md-sys-color-on-error: #ffffff;
    --md-sys-color-surface: #f6f6ff;
    --md-sys-color-surface-dim: #ecebfa;
    --md-sys-color-surface-bright: #ffffff;
    --md-sys-color-surface-container-lowest: #ffffff;
    --md-sys-color-surface-container-low: #f0efff;
    --md-sys-color-surface-container: #f7f5ff;
    --md-sys-color-surface-container-high: #ffffff;
    --md-sys-color-surface-container-highest: #ffffff;
    --md-sys-color-outline: rgba(37, 51, 92, 0.12);
    --md-sys-color-outline-variant: rgba(37, 51, 92, 0.18);
    --md-sys-color-shadow: rgba(21, 30, 59, 0.12);
    --md-sys-color-on-surface: #0f172a;
    --md-sys-color-on-surface-variant: #4c566d;
    --md-sys-color-muted: rgba(15, 23, 42, 0.65);
    --md-sys-color-success: #16a34a;
    --md-sys-color-warning: #f59e0b;
    --blur-elevated: saturate(120%) blur(18px);
  }

  :global(html),
  :global(body) {
    margin: 0;
    padding: 0;
    min-height: 100%;
    background: radial-gradient(circle at 20% -10%, rgba(137, 112, 255, 0.32), transparent 55%),
      radial-gradient(circle at 90% -10%, rgba(71, 203, 255, 0.28), transparent 60%),
      var(--md-sys-color-surface);
    color: var(--md-sys-color-on-surface);
    font-family: var(--font-sans);
    font-feature-settings: 'ss01' 1, 'ss03' 1;
    -webkit-font-smoothing: antialiased;
  }

  :global(body)::before {
    content: '';
    position: fixed;
    inset: 0;
    pointer-events: none;
    background: linear-gradient(130deg, rgba(151, 125, 255, 0.18), rgba(255, 161, 209, 0.12));
    mix-blend-mode: screen;
    opacity: 0.7;
    z-index: -1;
  }

  :global(button),
  :global(input),
  :global(textarea),
  :global(select) {
    font-family: inherit;
    border-radius: var(--radius-sm);
    transition: all var(--transition);
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
    outline: 3px solid color-mix(in srgb, var(--md-sys-color-secondary) 55%, transparent);
    outline-offset: 2px;
  }

  :global(input),
  :global(textarea) {
    outline: none;
    border: 1px solid var(--md-sys-color-outline);
    background: color-mix(in srgb, var(--md-sys-color-surface-container-high) 92%, transparent);
    color: var(--md-sys-color-on-surface);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.02);
    padding: 0.85rem 1rem;
  }

  :global(input:focus),
  :global(textarea:focus) {
    border-color: color-mix(in srgb, var(--md-sys-color-primary) 55%, transparent);
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--md-sys-color-primary) 18%, transparent);
  }

  :global(textarea) {
    resize: vertical;
    min-height: 3rem;
  }

  :global(a) {
    color: var(--md-sys-color-secondary);
    text-decoration: none;
    font-weight: 600;
  }

  :global(a:hover) {
    text-decoration: underline;
  }

  :global(*),
  :global(*::before),
  :global(*::after) {
    box-sizing: border-box;
  }

  :global(*) {
    scrollbar-width: thin;
    scrollbar-color: color-mix(in srgb, var(--md-sys-color-primary) 70%, transparent) transparent;
  }

  :global(*::-webkit-scrollbar) {
    width: 0.5rem;
    height: 0.5rem;
  }

  :global(*::-webkit-scrollbar-button) {
    display: none;
  }

  :global(*::-webkit-scrollbar-track) {
    background: transparent;
  }

  :global(*::-webkit-scrollbar-thumb) {
    background: color-mix(in srgb, var(--md-sys-color-primary) 60%, transparent);
    border-radius: 999px;
  }

  :global(*::-webkit-scrollbar-thumb:hover) {
    background: color-mix(in srgb, var(--md-sys-color-primary) 80%, transparent);
  }

  @media (prefers-reduced-motion: reduce) {
    :global(*) {
      animation-duration: 0.001ms !important;
      animation-iteration-count: 1 !important;
      transition-duration: 0.001ms !important;
      scroll-behavior: auto !important;
    }
  }

  :global(.surface-card) {
    background: var(--md-sys-color-surface-container-high);
    border-radius: var(--radius-lg);
    border: 1px solid var(--md-sys-color-outline);
    box-shadow: var(--shadow-02);
    backdrop-filter: var(--blur-elevated);
  }

  :global(.surface-tonal) {
    background: color-mix(in srgb, var(--md-sys-color-primary) 16%, var(--md-sys-color-surface-container));
    color: var(--md-sys-color-on-surface);
  }

  :global(.surface-outline) {
    border: 1px solid color-mix(in srgb, var(--md-sys-color-outline) 60%, transparent);
  }

  :global(.page-container) {
    width: min(1100px, 100%);
    margin: 0 auto;
    padding: clamp(2rem, 5vw, 4rem) clamp(1.5rem, 4vw, 3.5rem) clamp(3rem, 6vw, 4.5rem);
    display: flex;
    flex-direction: column;
    gap: clamp(2rem, 5vw, 3.5rem);
  }

  :global(.eyebrow) {
    text-transform: uppercase;
    letter-spacing: 0.14em;
    font-size: 0.72rem;
    color: color-mix(in srgb, var(--md-sys-color-primary) 65%, var(--md-sys-color-muted) 35%);
    font-weight: 700;
  }

  :global(.headline-hero) {
    margin: 0;
    font-size: clamp(2.3rem, 5vw, 3.2rem);
    letter-spacing: -0.02em;
  }

  :global(.body-muted) {
    color: var(--md-sys-color-muted);
    line-height: 1.6;
  }

  :global(.button-primary) {
    border-radius: var(--radius-sm);
    background: linear-gradient(135deg, var(--md-sys-color-primary), var(--md-sys-color-secondary));
    color: var(--md-sys-color-on-primary);
    box-shadow: 0 20px 40px color-mix(in srgb, var(--md-sys-color-primary) 35%, transparent);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    padding: 0.95rem 1.4rem;
    font-size: 1rem;
  }

  :global(.button-primary:hover) {
    transform: translateY(-1px);
    box-shadow: 0 24px 44px color-mix(in srgb, var(--md-sys-color-primary) 42%, transparent);
  }

  :global(.button-primary:active) {
    transform: translateY(0);
    box-shadow: 0 12px 30px color-mix(in srgb, var(--md-sys-color-primary) 28%, transparent);
  }

  .version {
    position: fixed;
    bottom: 1rem;
    right: 1rem;
    color: var(--md-sys-color-muted);
    font-size: 0.72rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    backdrop-filter: var(--blur-elevated);
    background: color-mix(in srgb, var(--md-sys-color-surface-container-high) 80%, transparent);
    padding: 0.35rem 0.85rem;
    border-radius: 999px;
    box-shadow: var(--shadow-xs);
    border: 1px solid var(--md-sys-color-outline);
  }
</style>

<slot />

<div class="version">{APP_VERSION}</div>
