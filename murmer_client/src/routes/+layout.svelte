<!--
  Root layout for the Murmer desktop client. It initialises the theme store on
  mount and defines the design system every page builds on:

  - Color tokens (`--color-*`), dark by default with a light override.
  - A 4px spacing scale (`--space-*`) — all padding/margins/gaps use it.
  - A type scale (`--text-*`) on a 16px root; Inter for UI, JetBrains Mono
    for code, timestamps and addresses only.
  - Radii, shadows and z-index scales.
  - Shared component primitives: `.btn` (+ variants), `.icon-btn`, `.field`,
    `.menu-panel`, `.badge`, `.surface-card`.

  Components must use these tokens and primitives — no hardcoded colors,
  font sizes or one-off spacing values.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { APP_VERSION } from '$lib/version';
  import { theme } from '$lib/stores/theme';
  import DialogHost from '$lib/components/DialogHost.svelte';

  // Fonts are bundled locally so startup never blocks on a network fetch
  // and the desktop client works fully offline.
  import '@fontsource-variable/inter/index.css';
  import '@fontsource/jetbrains-mono/400.css';
  import '@fontsource/jetbrains-mono/500.css';
  import '@fontsource/jetbrains-mono/600.css';

  onMount(() => {
    theme.init();
  });
</script>

<style>
  :global(:root) {
    color-scheme: dark;
    --font-sans: 'Inter Variable', 'Inter', system-ui, -apple-system, sans-serif;
    --font-mono: 'JetBrains Mono', ui-monospace, 'Menlo', monospace;

    /* Color system: dark blue-gray surfaces with a single cyan accent.
       Magenta/pink is reserved for errors and destructive actions. */
    --color-bg: #0b0e14;
    --color-surface: #11151d;
    --color-surface-elevated: #171c26;
    --color-surface-raised: #1f2531;
    --color-surface-outline: #262d3b;
    --color-outline-strong: #374052;
    --color-on-surface: #e8ecf4;
    --color-on-surface-variant: #b4bccc;
    --color-muted: #8a94a8;
    --color-primary: #27c0e8;
    --color-on-primary: #06222c;
    --color-primary-container: #0d3d4d;
    --color-secondary: var(--color-primary);
    --color-error: #f0517d;
    --color-tertiary: var(--color-error);
    --color-success: #3ecf8e;
    --color-warning: #e8a33d;
    --color-overlay: rgba(6, 9, 15, 0.66);

    /* Spacing scale (4px base). Use these for all padding, margin and gap. */
    --space-1: 0.25rem;
    --space-2: 0.5rem;
    --space-3: 0.75rem;
    --space-4: 1rem;
    --space-5: 1.5rem;
    --space-6: 2rem;
    --space-7: 3rem;
    --space-8: 4rem;

    /* Type scale (16px root). */
    --text-xs: 0.75rem;
    --text-sm: 0.8125rem;
    --text-md: 0.875rem;
    --text-lg: 1rem;
    --text-xl: 1.125rem;
    --text-2xl: 1.375rem;

    /* Radii: controls 6px, cards/inputs 8px, panels/modals 12px. */
    --radius-xs: 4px;
    --radius-sm: 6px;
    --radius-md: 8px;
    --radius-lg: 12px;
    --radius-pill: 999px;

    /* Elevation */
    --shadow-xs: 0 1px 2px rgba(0, 0, 0, 0.3);
    --shadow-sm: 0 2px 8px rgba(0, 0, 0, 0.35);
    --shadow-md: 0 8px 24px rgba(0, 0, 0, 0.45);
    --shadow-lg: 0 16px 48px rgba(0, 0, 0, 0.55);
    --blur-elevated: saturate(120%) blur(4px);

    /* Control sizing: consistent hit areas. */
    --control-height: 2.25rem;
    --control-height-lg: 2.75rem;

    /* Motion */
    --motion-duration-short: 100ms;
    --motion-duration-medium: 160ms;
    --motion-easing-standard: cubic-bezier(0.2, 0, 0, 1);
    --transition: var(--motion-duration-short) var(--motion-easing-standard);

    /* Stacking order */
    --z-dropdown: 60;
    --z-overlay: 100;
    --z-modal: 1200;
    --z-top: 10000;
  }

  :global(html[data-theme='light']) {
    color-scheme: light;
    --color-bg: #eef1f6;
    --color-surface: #f7f9fc;
    --color-surface-elevated: #ffffff;
    --color-surface-raised: #eaeef5;
    --color-surface-outline: #d7dde8;
    --color-outline-strong: #b9c2d2;
    --color-on-surface: #171c26;
    --color-on-surface-variant: #3d4658;
    --color-muted: #5c6678;
    --color-primary: #0b7ea3;
    --color-on-primary: #ffffff;
    --color-primary-container: #d0eef8;
    --color-error: #c22a5b;
    --color-success: #0d8f56;
    --color-warning: #a36b0a;
    --color-overlay: rgba(23, 28, 38, 0.4);
    --shadow-xs: 0 1px 2px rgba(23, 28, 38, 0.08);
    --shadow-sm: 0 2px 8px rgba(23, 28, 38, 0.1);
    --shadow-md: 0 8px 24px rgba(23, 28, 38, 0.14);
    --shadow-lg: 0 16px 48px rgba(23, 28, 38, 0.18);
  }

  :global(html),
  :global(body) {
    margin: 0;
    padding: 0;
    min-height: 100%;
    background: var(--color-bg);
    color: var(--color-on-surface);
    font-family: var(--font-sans);
    font-size: 16px;
    line-height: 1.5;
    -webkit-font-smoothing: antialiased;
  }

  :global(body) {
    font-size: var(--text-md);
  }

  :global(*),
  :global(*::before),
  :global(*::after) {
    box-sizing: border-box;
  }

  :global(button),
  :global(input),
  :global(textarea),
  :global(select) {
    font-family: inherit;
    font-size: var(--text-md);
    border-radius: var(--radius-sm);
    /* Only transition cheap paint/composite properties; `transition: all`
       also animates layout properties, which is expensive in WebKitGTK. */
    transition:
      background-color var(--transition),
      border-color var(--transition),
      color var(--transition),
      box-shadow var(--transition),
      opacity var(--transition);
  }

  :global(button) {
    cursor: pointer;
    border: none;
    font-weight: 500;
  }

  :global(button:disabled) {
    cursor: default;
    opacity: 0.5;
  }

  :global(button:focus-visible),
  :global(input:focus-visible),
  :global(textarea:focus-visible),
  :global(select:focus-visible),
  :global(a:focus-visible) {
    outline: 2px solid var(--color-primary);
    outline-offset: 2px;
  }

  :global(input),
  :global(textarea),
  :global(select) {
    border: 1px solid var(--color-surface-outline);
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
    padding: var(--space-2) var(--space-3);
    min-height: var(--control-height);
  }

  :global(input::placeholder),
  :global(textarea::placeholder) {
    color: var(--color-muted);
  }

  :global(input:focus),
  :global(textarea:focus),
  :global(select:focus) {
    outline: none;
    border-color: var(--color-primary);
    box-shadow: 0 0 0 1px var(--color-primary);
  }

  :global(textarea) {
    resize: vertical;
    min-height: var(--control-height-lg);
    line-height: 1.5;
  }

  :global(a) {
    color: var(--color-primary);
    text-decoration: none;
    font-weight: 500;
  }

  :global(a:hover) {
    text-decoration: underline;
  }

  :global(h1),
  :global(h2),
  :global(h3),
  :global(h4) {
    margin: 0;
    font-weight: 600;
    letter-spacing: -0.01em;
    line-height: 1.3;
  }

  :global(*) {
    scrollbar-width: thin;
    scrollbar-color: var(--color-outline-strong) transparent;
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
    background: var(--color-outline-strong);
    border-radius: var(--radius-pill);
  }

  :global(*::-webkit-scrollbar-thumb:hover) {
    background: color-mix(in srgb, var(--color-outline-strong) 70%, var(--color-muted));
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

  /* ---- Shared primitives ------------------------------------------------ */

  /* Buttons: one base, three variants. Default is a quiet outlined button. */
  :global(.btn) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    min-height: var(--control-height);
    padding: 0 var(--space-4);
    border-radius: var(--radius-sm);
    border: 1px solid var(--color-surface-outline);
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
    font-weight: 500;
    font-size: var(--text-md);
    white-space: nowrap;
  }

  :global(.btn:hover:not(:disabled)) {
    border-color: var(--color-outline-strong);
    background: color-mix(in srgb, var(--color-surface-raised) 80%, var(--color-outline-strong));
  }

  :global(.btn-primary) {
    background: var(--color-primary);
    border-color: transparent;
    color: var(--color-on-primary);
    font-weight: 600;
  }

  :global(.btn-primary:hover:not(:disabled)) {
    background: color-mix(in srgb, var(--color-primary) 88%, var(--color-on-surface));
    border-color: transparent;
  }

  :global(.btn-ghost) {
    background: transparent;
    border-color: transparent;
    color: var(--color-on-surface-variant);
  }

  :global(.btn-ghost:hover:not(:disabled)) {
    background: var(--color-surface-raised);
    border-color: transparent;
    color: var(--color-on-surface);
  }

  :global(.btn-danger) {
    background: color-mix(in srgb, var(--color-error) 12%, transparent);
    border-color: color-mix(in srgb, var(--color-error) 35%, transparent);
    color: var(--color-error);
  }

  :global(.btn-danger:hover:not(:disabled)) {
    background: color-mix(in srgb, var(--color-error) 20%, transparent);
    border-color: color-mix(in srgb, var(--color-error) 50%, transparent);
  }

  /* Square icon-only button; pair with an .sr-only label or title. */
  :global(.icon-btn) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--control-height);
    height: var(--control-height);
    padding: 0;
    flex-shrink: 0;
    border-radius: var(--radius-sm);
    border: 1px solid transparent;
    background: transparent;
    color: var(--color-on-surface-variant);
  }

  :global(.icon-btn:hover:not(:disabled)) {
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
  }

  :global(.icon-btn.danger) {
    color: var(--color-error);
  }

  :global(.icon-btn.danger:hover:not(:disabled)) {
    background: color-mix(in srgb, var(--color-error) 14%, transparent);
  }

  :global(.icon-btn svg) {
    width: 1.125rem;
    height: 1.125rem;
  }

  /* Labelled form field. */
  :global(.field) {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  :global(.field > span) {
    font-size: var(--text-sm);
    font-weight: 500;
    color: var(--color-on-surface-variant);
  }

  /* Floating panel for dropdowns and context menus. */
  :global(.menu-panel) {
    background: var(--color-surface-elevated);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-md);
    padding: var(--space-1);
  }

  /* Small count/status badge. */
  :global(.badge) {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 1.25rem;
    padding: 0 var(--space-2);
    border-radius: var(--radius-pill);
    font-size: var(--text-xs);
    font-weight: 600;
    line-height: 1.25rem;
    background: var(--color-surface-raised);
    color: var(--color-on-surface-variant);
  }

  :global(.surface-card) {
    background: var(--color-surface-elevated);
    border-radius: var(--radius-lg);
    border: 1px solid var(--color-surface-outline);
  }

  :global(.page-container) {
    width: min(1040px, 100%);
    margin: 0 auto;
    padding: var(--space-7) var(--space-5) var(--space-8);
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }

  :global(.eyebrow) {
    text-transform: uppercase;
    letter-spacing: 0.1em;
    font-size: var(--text-xs);
    color: var(--color-primary);
    font-weight: 600;
  }

  :global(.body-muted) {
    color: var(--color-muted);
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

  .version {
    position: fixed;
    bottom: var(--space-3);
    right: var(--space-3);
    color: var(--color-muted);
    font-size: var(--text-xs);
    font-family: var(--font-mono);
    background: var(--color-surface-elevated);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-xs);
    border: 1px solid var(--color-surface-outline);
    pointer-events: none;
  }
</style>

<slot />

<DialogHost />

<div class="version">{APP_VERSION}</div>
