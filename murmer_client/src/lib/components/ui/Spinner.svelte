<!--
  Loading spinner component with customizable size and color.
  Uses Material 3 design tokens for theming.
-->
<script lang="ts">
  export let size: 'sm' | 'md' | 'lg' | 'xl' = 'md';
  export let color: 'primary' | 'secondary' | 'on-surface' = 'primary';
  export let ariaLabel = 'Loading';
  
  const sizeMap = {
    sm: '1rem',
    md: '1.5rem',
    lg: '2rem',
    xl: '3rem'
  };
  
  const colorMap = {
    primary: 'var(--md-sys-color-primary)',
    secondary: 'var(--md-sys-color-secondary)',
    'on-surface': 'var(--md-sys-color-on-surface-variant)'
  };
</script>

<div
  class="spinner"
  style="--spinner-size: {sizeMap[size]}; --spinner-color: {colorMap[color]}"
  role="status"
  aria-label={ariaLabel}
>
  <svg class="spinner-svg" viewBox="0 0 50 50">
    <circle
      class="spinner-circle"
      cx="25"
      cy="25"
      r="20"
      fill="none"
      stroke-width="5"
    />
  </svg>
  <span class="sr-only">{ariaLabel}</span>
</div>

<style>
  .spinner {
    display: inline-block;
    width: var(--spinner-size);
    height: var(--spinner-size);
  }

  .spinner-svg {
    animation: spinner-rotate 2s linear infinite;
    width: 100%;
    height: 100%;
  }

  .spinner-circle {
    stroke: var(--spinner-color);
    stroke-linecap: round;
    stroke-dasharray: 1, 150;
    stroke-dashoffset: 0;
    animation: spinner-dash 1.5s ease-in-out infinite;
  }

  @keyframes spinner-rotate {
    100% {
      transform: rotate(360deg);
    }
  }

  @keyframes spinner-dash {
    0% {
      stroke-dasharray: 1, 150;
      stroke-dashoffset: 0;
    }
    50% {
      stroke-dasharray: 90, 150;
      stroke-dashoffset: -35;
    }
    100% {
      stroke-dasharray: 90, 150;
      stroke-dashoffset: -124;
    }
  }

  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border-width: 0;
  }
</style>

