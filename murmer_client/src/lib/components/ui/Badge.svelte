<!--
  Badge component for displaying status indicators, counts, or labels.
  Follows Material 3 design principles with multiple variants.
-->
<script lang="ts">
  export let variant: 'primary' | 'secondary' | 'success' | 'warning' | 'error' | 'neutral' = 'primary';
  export let size: 'sm' | 'md' | 'lg' = 'md';
  export let dot = false;
  export let pulse = false;
</script>

<span
  class="badge badge-{variant} badge-{size}"
  class:badge-dot={dot}
  class:badge-pulse={pulse}
>
  <slot />
</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border-radius: 999px;
    font-weight: 600;
    letter-spacing: 0.02em;
    transition: all var(--motion-duration-short) var(--motion-easing-standard);
    position: relative;
  }

  /* Size variants */
  .badge-sm {
    padding: 0.15rem 0.5rem;
    font-size: 0.7rem;
  }

  .badge-md {
    padding: 0.25rem 0.65rem;
    font-size: 0.8rem;
  }

  .badge-lg {
    padding: 0.35rem 0.85rem;
    font-size: 0.9rem;
  }

  /* Dot variant */
  .badge-dot {
    width: 0.6rem;
    height: 0.6rem;
    padding: 0;
    border-radius: 50%;
  }

  .badge-dot.badge-md {
    width: 0.75rem;
    height: 0.75rem;
  }

  .badge-dot.badge-lg {
    width: 0.9rem;
    height: 0.9rem;
  }

  /* Color variants */
  .badge-primary {
    background: linear-gradient(135deg, var(--md-sys-color-primary), var(--md-sys-color-secondary));
    color: var(--md-sys-color-on-primary);
    box-shadow: 0 2px 8px color-mix(in srgb, var(--md-sys-color-primary) 35%, transparent);
  }

  .badge-secondary {
    background: color-mix(in srgb, var(--md-sys-color-secondary) 85%, transparent);
    color: var(--md-sys-color-on-surface);
    border: 1px solid color-mix(in srgb, var(--md-sys-color-secondary) 45%, transparent);
  }

  .badge-success {
    background: color-mix(in srgb, var(--md-sys-color-success) 20%, transparent);
    color: var(--md-sys-color-success);
    border: 1px solid color-mix(in srgb, var(--md-sys-color-success) 40%, transparent);
  }

  .badge-warning {
    background: color-mix(in srgb, var(--md-sys-color-warning) 20%, transparent);
    color: var(--md-sys-color-warning);
    border: 1px solid color-mix(in srgb, var(--md-sys-color-warning) 40%, transparent);
  }

  .badge-error {
    background: color-mix(in srgb, var(--md-sys-color-error) 20%, transparent);
    color: var(--md-sys-color-error);
    border: 1px solid color-mix(in srgb, var(--md-sys-color-error) 40%, transparent);
  }

  .badge-neutral {
    background: var(--md-sys-color-surface-container-high);
    color: var(--md-sys-color-on-surface-variant);
    border: 1px solid var(--md-sys-color-outline);
  }

  /* Dot color variants */
  .badge-dot.badge-primary {
    background: var(--md-sys-color-primary);
  }

  .badge-dot.badge-secondary {
    background: var(--md-sys-color-secondary);
  }

  .badge-dot.badge-success {
    background: var(--md-sys-color-success);
  }

  .badge-dot.badge-warning {
    background: var(--md-sys-color-warning);
  }

  .badge-dot.badge-error {
    background: var(--md-sys-color-error);
  }

  .badge-dot.badge-neutral {
    background: var(--md-sys-color-on-surface-variant);
  }

  /* Pulse animation */
  .badge-pulse::after {
    content: '';
    position: absolute;
    inset: -2px;
    border-radius: inherit;
    background: inherit;
    opacity: 0.4;
    animation: pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite;
  }

  @keyframes pulse {
    0%, 100% {
      opacity: 0.4;
      transform: scale(1);
    }
    50% {
      opacity: 0;
      transform: scale(1.5);
    }
  }
</style>

