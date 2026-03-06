<!--
  Reusable card component following Material 3 surface design.
  Provides consistent elevation, borders, and backdrop effects.
-->
<script lang="ts">
  export let variant: 'default' | 'elevated' | 'outlined' | 'tonal' = 'default';
  export let padding: 'none' | 'sm' | 'md' | 'lg' = 'md';
  export let interactive = false;
  export let glowColor: string | null = null;
</script>

{#if interactive}
  <div
    class="card card-{variant} card-padding-{padding} card-interactive"
    style={glowColor ? `--glow-color: ${glowColor}` : ''}
    on:click
    on:mouseenter
    on:mouseleave
    on:keydown={(e) => (e.key === 'Enter' || e.key === ' ') && e.currentTarget.click()}
    role="button"
    tabindex="0"
  >
    <slot />
  </div>
{:else}
  <div
    class="card card-{variant} card-padding-{padding}"
    style={glowColor ? `--glow-color: ${glowColor}` : ''}
  >
    <slot />
  </div>
{/if}

<style>
  .card {
    position: relative;
    border-radius: var(--radius-lg);
    transition: all var(--motion-duration-medium) var(--motion-easing-standard);
  }

  /* Padding variants */
  .card-padding-none {
    padding: 0;
  }

  .card-padding-sm {
    padding: 1rem;
  }

  .card-padding-md {
    padding: clamp(1.25rem, 3vw, 1.6rem);
  }

  .card-padding-lg {
    padding: clamp(1.6rem, 4vw, 2.4rem);
  }

  /* Style variants */
  .card-default {
    background: var(--md-sys-color-surface-container-high);
    border: 1px solid var(--md-sys-color-outline);
    box-shadow: var(--shadow-02);
    backdrop-filter: var(--blur-elevated);
  }

  .card-elevated {
    background: var(--md-sys-color-surface-container-high);
    border: 1px solid var(--md-sys-color-outline);
    box-shadow: var(--shadow-02);
    backdrop-filter: var(--blur-elevated);
  }

  .card-elevated::before {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
    border: 1px solid color-mix(in srgb, var(--md-sys-color-primary) 24%, transparent);
    opacity: 0.35;
    pointer-events: none;
  }

  .card-outlined {
    background: color-mix(in srgb, var(--md-sys-color-surface-container) 85%, transparent);
    border: 1px solid var(--md-sys-color-outline-variant);
  }

  .card-tonal {
    background: color-mix(in srgb, var(--md-sys-color-primary) 16%, var(--md-sys-color-surface-container));
    border: 1px solid color-mix(in srgb, var(--md-sys-color-outline) 60%, transparent);
  }

  /* Interactive cards */
  .card-interactive {
    cursor: pointer;
    outline: none;
  }

  .card-interactive:hover {
    transform: translateY(-2px);
    box-shadow: var(--shadow-03);
  }

  .card-interactive:active {
    transform: translateY(-1px);
  }

  .card-interactive:focus-visible {
    outline: 3px solid color-mix(in srgb, var(--md-sys-color-secondary) 55%, transparent);
    outline-offset: 2px;
  }

  /* Glow effect */
  .card[style*='--glow-color']::after {
    content: '';
    position: absolute;
    inset: -40% -30% 60% auto;
    width: 280px;
    height: 280px;
    background: radial-gradient(circle, var(--glow-color, rgba(137, 112, 255, 0.3)), transparent 65%);
    pointer-events: none;
    opacity: 0.7;
  }
</style>

