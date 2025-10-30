<!--
  Colored dot reflecting current ping in milliseconds with pulse animation.
  Uses Material 3 semantic colors for status indication.
-->
<script lang="ts">
  export let ping: number = 0;
  export let size: 'sm' | 'md' | 'lg' = 'md';
  export let pulse = true;
  
  $: status = ping === 0 ? 'unknown'
    : ping < 50 ? 'excellent'
    : ping < 100 ? 'good'
    : ping < 200 ? 'fair'
    : ping < 400 ? 'poor'
    : 'critical';
  
  $: label = ping === 0 ? 'Checking connection'
    : ping < 50 ? `Excellent (${ping}ms)`
    : ping < 100 ? `Good (${ping}ms)`
    : ping < 200 ? `Fair (${ping}ms)`
    : ping < 400 ? `Poor (${ping}ms)`
    : `Critical (${ping}ms)`;
  
  const sizeMap = {
    sm: '0.5rem',
    md: '0.65rem',
    lg: '0.8rem'
  };
</script>

<span
  class="ping ping-{status} ping-{size}"
  class:ping-pulse={pulse && status !== 'unknown'}
  style="width: {sizeMap[size]}; height: {sizeMap[size]}"
  title={label}
  aria-label={label}
  role="status"
></span>

<style>
  .ping {
    border-radius: 50%;
    display: inline-block;
    margin-left: 0.25rem;
    position: relative;
    transition: all var(--motion-duration-short) var(--motion-easing-standard);
  }

  .ping-unknown {
    background: var(--md-sys-color-on-surface-variant);
    opacity: 0.5;
  }

  .ping-excellent {
    background: var(--md-sys-color-success);
    box-shadow: 0 0 4px color-mix(in srgb, var(--md-sys-color-success) 60%, transparent);
  }

  .ping-good {
    background: color-mix(in srgb, var(--md-sys-color-success) 70%, var(--md-sys-color-warning) 30%);
  }

  .ping-fair {
    background: var(--md-sys-color-warning);
  }

  .ping-poor {
    background: color-mix(in srgb, var(--md-sys-color-warning) 40%, var(--md-sys-color-error) 60%);
  }

  .ping-critical {
    background: var(--md-sys-color-error);
    box-shadow: 0 0 6px color-mix(in srgb, var(--md-sys-color-error) 70%, transparent);
  }

  .ping-pulse::after {
    content: '';
    position: absolute;
    inset: -2px;
    border-radius: 50%;
    background: inherit;
    opacity: 0.6;
    animation: ping-pulse 2s cubic-bezier(0.4, 0, 0.6, 1) infinite;
  }

  @keyframes ping-pulse {
    0%, 100% {
      opacity: 0.6;
      transform: scale(1);
    }
    50% {
      opacity: 0;
      transform: scale(1.8);
    }
  }
</style>
