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
  <link rel="preconnect" href="https://fonts.googleapis.com" />
  <link rel="preconnect" href="https://fonts.gstatic.com" crossorigin="anonymous" />
  <link
    rel="stylesheet"
    href="https://fonts.googleapis.com/css2?family=IBM+Plex+Mono:wght@400;500;600;700&family=JetBrains+Mono:wght@400;500;600;700&display=swap"
  />
</svelte:head>

<style>
  :global(:root) {
    color-scheme: dark;
    --font-sans: 'IBM Plex Mono', 'JetBrains Mono', 'Menlo', ui-monospace, monospace;
    --font-mono: 'JetBrains Mono', 'IBM Plex Mono', 'Fira Code', 'Menlo', monospace;
    /* Murmer design system: dual-tone cyberpunk on dark blue-gray.
       Primary = electric cyan, tertiary = hot magenta. */
    --md-sys-color-primary: #00d9ff;
    --md-sys-color-on-primary: #001a20;
    --md-sys-color-primary-container: #003848;
    --md-sys-color-on-primary-container: #c5f6ff;
    --md-sys-color-secondary: #00d9ff;
    --md-sys-color-on-secondary: #001a20;
    --md-sys-color-tertiary: #ff3d7f;
    --md-sys-color-on-tertiary: #360015;
    --md-sys-color-error: #ff3d7f;
    --md-sys-color-on-error: #360015;
    --md-sys-color-surface: #0f1419;
    --md-sys-color-surface-dim: #0a0e12;
    --md-sys-color-surface-bright: #2d3548;
    --md-sys-color-surface-container-lowest: #080b0f;
    --md-sys-color-surface-container-low: #1a1f2e;
    --md-sys-color-surface-container: #1e2433;
    --md-sys-color-surface-container-high: #252c3d;
    --md-sys-color-surface-container-highest: #2d3548;
    --md-sys-color-outline: rgba(61, 68, 88, 0.9);
    --md-sys-color-outline-variant: rgba(61, 68, 88, 0.6);
    --md-sys-color-shadow: rgba(0, 0, 0, 0.75);
    --md-sys-color-on-surface: #f0f3f7;
    --md-sys-color-on-surface-variant: #b8c1d3;
    /* Muted is kept at >= 5:1 contrast on the darkest surfaces (a11y). */
    --md-sys-color-muted: #8a94a9;
    --md-sys-color-success: #00ff88;
    --md-sys-color-warning: #ffaa00;
    --motion-duration-short: 100ms;
    --motion-duration-medium: 160ms;
    --motion-easing-standard: cubic-bezier(0.2, 0, 0, 1);
    /* Brutalist: zero radius on panels, 3px on controls, pills for chips. */
    --radius-xs: 0px;
    --radius-sm: 3px;
    --radius-md: 3px;
    --radius-lg: 0px;
    --radius-pill: 999px;
    /* Elevation scale */
    --shadow-xs: 0 1px 0 rgba(0, 217, 255, 0.08);
    --shadow-sm: 0 8px 24px rgba(0, 0, 0, 0.45);
    --shadow-md: 0 20px 60px rgba(0, 0, 0, 0.9);
    --shadow-lg: 0 24px 72px rgba(0, 0, 0, 0.92);
    --blur-elevated: saturate(120%) blur(4px);
    /* Type scale (root font-size is 13px) */
    --text-xs: 0.7rem;
    --text-sm: 0.8rem;
    --text-md: 0.9rem;
    --text-lg: 1.1rem;
    --text-xl: 1.35rem;
    --text-2xl: 1.7rem;
    /* Stacking order */
    --z-dropdown: 60;
    --z-overlay: 100;
    --z-modal: 1200;
    --z-top: 10000;

    /* Component-facing aliases — components use these, never --md-sys-* directly */
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
    --transition: var(--motion-duration-short) var(--motion-easing-standard);
  }

  :global(html[data-theme='light']) {
    color-scheme: light;
    --md-sys-color-primary: #007a99;
    --md-sys-color-on-primary: #ffffff;
    --md-sys-color-primary-container: #c5f6ff;
    --md-sys-color-on-primary-container: #001a20;
    --md-sys-color-secondary: #007a99;
    --md-sys-color-on-secondary: #ffffff;
    --md-sys-color-tertiary: #c2185b;
    --md-sys-color-on-tertiary: #ffffff;
    --md-sys-color-error: #c2185b;
    --md-sys-color-on-error: #ffffff;
    --md-sys-color-surface: #f4f6fa;
    --md-sys-color-surface-dim: #e6ebf1;
    --md-sys-color-surface-bright: #ffffff;
    --md-sys-color-surface-container-lowest: #ffffff;
    --md-sys-color-surface-container-low: #eef1f6;
    --md-sys-color-surface-container: #e6ebf1;
    --md-sys-color-surface-container-high: #dde3ec;
    --md-sys-color-surface-container-highest: #d3dae5;
    --md-sys-color-outline: rgba(15, 20, 25, 0.16);
    --md-sys-color-outline-variant: rgba(15, 20, 25, 0.24);
    --md-sys-color-shadow: rgba(15, 20, 25, 0.12);
    --md-sys-color-on-surface: #0f1419;
    --md-sys-color-on-surface-variant: #3d4458;
    --md-sys-color-muted: #5a647a;
    --md-sys-color-success: #0a8f47;
    --md-sys-color-warning: #b76e00;
    --blur-elevated: saturate(110%) blur(4px);
  }

  :global(html),
  :global(body) {
    margin: 0;
    padding: 0;
    min-height: 100%;
    background: var(--md-sys-color-surface);
    color: var(--md-sys-color-on-surface);
    font-family: var(--font-sans);
    font-size: 13px;
    line-height: 1.5;
    -webkit-font-smoothing: antialiased;
  }

  :global(body)::before {
    content: '';
    position: fixed;
    inset: 0;
    pointer-events: none;
    background:
      radial-gradient(circle at 15% 0%, rgba(0, 217, 255, 0.06), transparent 45%),
      radial-gradient(circle at 90% 100%, rgba(255, 61, 127, 0.05), transparent 50%);
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
    border-color: var(--md-sys-color-primary);
    box-shadow: 0 0 0 1px var(--md-sys-color-primary),
      0 0 12px color-mix(in srgb, var(--md-sys-color-primary) 30%, transparent);
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
    border-radius: var(--radius-pill);
  }

  :global(*::-webkit-scrollbar-thumb:hover) {
    background: color-mix(in srgb, var(--md-sys-color-primary) 80%, transparent);
  }

  /* Smooth scroll behavior */
  :global(html) {
    scroll-behavior: smooth;
  }

  /* Additional animation utilities */
  :global(.fade-in) {
    animation: fadeIn 0.4s var(--motion-easing-standard);
  }

  :global(.slide-up) {
    animation: slideUp 0.5s var(--motion-easing-standard);
  }

  :global(.scale-in) {
    animation: scaleIn 0.3s var(--motion-easing-standard);
  }

  @keyframes fadeIn {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @keyframes slideUp {
    from {
      opacity: 0;
      transform: translateY(20px);
    }
    to {
      opacity: 1;
      transform: translateY(0);
    }
  }

  @keyframes scaleIn {
    from {
      opacity: 0;
      transform: scale(0.95);
    }
    to {
      opacity: 1;
      transform: scale(1);
    }
  }

  /* Shimmer loading effect */
  :global(.shimmer) {
    background: linear-gradient(
      90deg,
      var(--md-sys-color-surface-container) 0%,
      var(--md-sys-color-surface-container-high) 50%,
      var(--md-sys-color-surface-container) 100%
    );
    background-size: 200% 100%;
    animation: shimmer 1.5s ease-in-out infinite;
  }

  @keyframes shimmer {
    0% {
      background-position: -200% 0;
    }
    100% {
      background-position: 200% 0;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    :global(*),
    :global(html) {
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
    box-shadow: var(--shadow-sm);
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
    font-size: var(--text-xs);
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

  /* Visually hidden but readable by screen readers */
  :global(.sr-only) {
    border: 0;
    clip: rect(0 0 0 0);
    height: 1px;
    margin: -1px;
    overflow: hidden;
    padding: 0;
    position: absolute;
    width: 1px;
  }

  :global(.button-primary) {
    border-radius: var(--radius-sm);
    background: var(--md-sys-color-primary);
    color: var(--md-sys-color-on-primary);
    box-shadow: 0 0 12px color-mix(in srgb, var(--md-sys-color-primary) 35%, transparent);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    padding: 0.85rem 1.25rem;
    font-size: var(--text-md);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    font-weight: 700;
  }

  :global(.button-primary:hover) {
    box-shadow: 0 0 20px color-mix(in srgb, var(--md-sys-color-primary) 55%, transparent);
  }

  :global(.button-primary:active) {
    box-shadow: 0 0 6px color-mix(in srgb, var(--md-sys-color-primary) 25%, transparent);
  }

  .version {
    position: fixed;
    bottom: 0.75rem;
    right: 0.75rem;
    color: var(--md-sys-color-muted);
    font-size: var(--text-xs);
    letter-spacing: 0.12em;
    text-transform: uppercase;
    font-family: var(--font-mono);
    background: var(--md-sys-color-surface-container);
    padding: 0.3rem 0.6rem;
    border-radius: var(--radius-sm);
    border: 1px solid var(--md-sys-color-outline);
  }
</style>

<slot />

<div class="version">{APP_VERSION}</div>
