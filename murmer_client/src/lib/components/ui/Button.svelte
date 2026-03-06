<!--
  Modern button component with multiple variants following Material 3 design principles.
  Supports primary, secondary, ghost, and danger styles with consistent styling.
-->
<script lang="ts">
  export let variant: 'primary' | 'secondary' | 'ghost' | 'danger' = 'primary';
  export let size: 'sm' | 'md' | 'lg' = 'md';
  export let type: 'button' | 'submit' | 'reset' = 'button';
  export let disabled = false;
  export let fullWidth = false;
  export let icon = false;
  export let ariaLabel: string | undefined = undefined;
  
  const sizeClasses = {
    sm: 'btn-sm',
    md: 'btn-md',
    lg: 'btn-lg'
  };
</script>

<button
  {type}
  {disabled}
  on:click
  on:mouseenter
  on:mouseleave
  on:focus
  on:blur
  class="btn btn-{variant} {sizeClasses[size]}"
  class:btn-icon={icon}
  class:btn-full={fullWidth}
  aria-label={ariaLabel}
>
  <slot />
</button>

<style>
  .btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    border-radius: var(--radius-sm);
    font-weight: 600;
    letter-spacing: 0.01em;
    border: 1px solid transparent;
    cursor: pointer;
    transition: all var(--motion-duration-short) var(--motion-easing-standard);
    position: relative;
    overflow: hidden;
    font-family: inherit;
  }

  .btn::before {
    content: '';
    position: absolute;
    inset: 0;
    background: linear-gradient(135deg, rgba(255, 255, 255, 0.1), transparent);
    opacity: 0;
    transition: opacity var(--motion-duration-short) var(--motion-easing-standard);
  }

  .btn:hover::before {
    opacity: 1;
  }

  .btn:active {
    transform: scale(0.98);
  }

  .btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
    transform: none !important;
  }

  .btn:disabled::before {
    opacity: 0 !important;
  }

  /* Size variants */
  .btn-sm {
    padding: 0.5rem 0.9rem;
    font-size: 0.85rem;
  }

  .btn-md {
    padding: 0.75rem 1.15rem;
    font-size: 0.95rem;
  }

  .btn-lg {
    padding: 0.95rem 1.4rem;
    font-size: 1rem;
  }

  /* Style variants */
  .btn-primary {
    background: linear-gradient(135deg, var(--md-sys-color-primary), var(--md-sys-color-secondary));
    color: var(--md-sys-color-on-primary);
    box-shadow: 0 4px 16px color-mix(in srgb, var(--md-sys-color-primary) 25%, transparent);
  }

  .btn-primary:hover:not(:disabled) {
    transform: translateY(-2px);
    box-shadow: 0 8px 24px color-mix(in srgb, var(--md-sys-color-primary) 35%, transparent);
  }

  .btn-primary:active:not(:disabled) {
    transform: translateY(-1px);
    box-shadow: 0 4px 12px color-mix(in srgb, var(--md-sys-color-primary) 20%, transparent);
  }

  .btn-secondary {
    background: color-mix(in srgb, var(--md-sys-color-primary) 18%, var(--md-sys-color-surface-container) 82%);
    color: var(--md-sys-color-on-surface);
    border-color: color-mix(in srgb, var(--md-sys-color-primary) 28%, transparent);
  }

  .btn-secondary:hover:not(:disabled) {
    transform: translateY(-1px);
    box-shadow: var(--shadow-01);
    background: color-mix(in srgb, var(--md-sys-color-primary) 24%, var(--md-sys-color-surface-container) 76%);
  }

  .btn-ghost {
    background: color-mix(in srgb, var(--md-sys-color-surface-container-high) 88%, transparent);
    color: color-mix(in srgb, var(--md-sys-color-on-surface) 90%, var(--md-sys-color-muted) 10%);
    border-color: color-mix(in srgb, var(--md-sys-color-outline-variant) 80%, transparent);
  }

  .btn-ghost:hover:not(:disabled) {
    transform: translateY(-1px);
    box-shadow: var(--shadow-01);
    background: color-mix(in srgb, var(--md-sys-color-surface-container-high) 95%, transparent);
  }

  .btn-danger {
    background: color-mix(in srgb, var(--md-sys-color-error) 16%, transparent);
    color: var(--md-sys-color-error);
    border-color: color-mix(in srgb, var(--md-sys-color-error) 45%, transparent);
  }

  .btn-danger:hover:not(:disabled) {
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--md-sys-color-error) 35%, transparent);
    background: color-mix(in srgb, var(--md-sys-color-error) 22%, transparent);
  }

  /* Icon button */
  .btn-icon {
    padding: 0.65rem;
    border-radius: 0.95rem;
    aspect-ratio: 1;
  }

  .btn-icon.btn-sm {
    padding: 0.5rem;
    border-radius: 0.75rem;
  }

  .btn-icon.btn-lg {
    padding: 0.85rem;
    border-radius: 1.1rem;
  }

  /* Full width */
  .btn-full {
    width: 100%;
  }

  /* Focus visible */
  .btn:focus-visible {
    outline: 3px solid color-mix(in srgb, var(--md-sys-color-secondary) 55%, transparent);
    outline-offset: 2px;
  }
</style>

