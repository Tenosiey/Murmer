<!--
  Icon button component for actions with icon-only interface.
  Optimized for touch targets and accessibility.
-->
<script lang="ts">
  export let variant: 'default' | 'primary' | 'danger' = 'default';
  export let size: 'sm' | 'md' | 'lg' = 'md';
  export let ariaLabel: string;
  export let type: 'button' | 'submit' | 'reset' = 'button';
  export let disabled = false;
  
  const sizeMap = {
    sm: { padding: '0.45rem', radius: '0.7rem' },
    md: { padding: '0.65rem', radius: '0.95rem' },
    lg: { padding: '0.85rem', radius: '1.1rem' }
  };
</script>

<button
  {type}
  {disabled}
  on:click
  on:mouseenter
  on:mouseleave
  class="icon-btn icon-btn-{variant}"
  style="padding: {sizeMap[size].padding}; border-radius: {sizeMap[size].radius}"
  aria-label={ariaLabel}
>
  <slot />
</button>

<style>
  .icon-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    border: 1px solid transparent;
    cursor: pointer;
    transition: all var(--motion-duration-short) var(--motion-easing-standard);
    position: relative;
    overflow: hidden;
    font-family: inherit;
    aspect-ratio: 1;
  }

  .icon-btn::before {
    content: '';
    position: absolute;
    inset: 0;
    background: linear-gradient(135deg, rgba(255, 255, 255, 0.08), transparent);
    opacity: 0;
    transition: opacity var(--motion-duration-short) var(--motion-easing-standard);
  }

  .icon-btn:hover:not(:disabled)::before {
    opacity: 1;
  }

  .icon-btn:active:not(:disabled) {
    transform: scale(0.95);
  }

  .icon-btn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }

  .icon-btn-default {
    background: color-mix(in srgb, var(--md-sys-color-surface-container-high) 88%, transparent);
    color: color-mix(in srgb, var(--md-sys-color-on-surface) 90%, var(--md-sys-color-muted) 10%);
    border-color: color-mix(in srgb, var(--md-sys-color-outline-variant) 80%, transparent);
  }

  .icon-btn-default:hover:not(:disabled) {
    transform: translateY(-1px);
    box-shadow: var(--shadow-01);
    background: color-mix(in srgb, var(--md-sys-color-surface-container-high) 95%, transparent);
  }

  .icon-btn-primary {
    background: linear-gradient(135deg, var(--md-sys-color-primary), var(--md-sys-color-secondary));
    color: var(--md-sys-color-on-primary);
    box-shadow: 0 4px 12px color-mix(in srgb, var(--md-sys-color-primary) 30%, transparent);
  }

  .icon-btn-primary:hover:not(:disabled) {
    transform: translateY(-2px);
    box-shadow: 0 6px 16px color-mix(in srgb, var(--md-sys-color-primary) 40%, transparent);
  }

  .icon-btn-danger {
    background: color-mix(in srgb, var(--md-sys-color-error) 16%, transparent);
    color: var(--md-sys-color-error);
    border-color: color-mix(in srgb, var(--md-sys-color-error) 45%, transparent);
  }

  .icon-btn-danger:hover:not(:disabled) {
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--md-sys-color-error) 35%, transparent);
    background: color-mix(in srgb, var(--md-sys-color-error) 22%, transparent);
  }

  .icon-btn:focus-visible {
    outline: 3px solid color-mix(in srgb, var(--md-sys-color-secondary) 55%, transparent);
    outline-offset: 2px;
  }
</style>

