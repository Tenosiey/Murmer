<!--
  Tooltip component for displaying helpful information on hover.
  Follows Material 3 design with smooth animations.
-->
<script lang="ts">
  import { fade, fly } from 'svelte/transition';
  import { cubicOut } from 'svelte/easing';
  
  export let text: string;
  export let position: 'top' | 'bottom' | 'left' | 'right' = 'top';
  export let delay = 600;
  
  let showTooltip = false;
  let timeout: ReturnType<typeof setTimeout> | null = null;
  
  function handleMouseEnter() {
    timeout = setTimeout(() => {
      showTooltip = true;
    }, delay);
  }
  
  function handleMouseLeave() {
    if (timeout) {
      clearTimeout(timeout);
      timeout = null;
    }
    showTooltip = false;
  }
  
  const positionClasses = {
    top: 'tooltip-top',
    bottom: 'tooltip-bottom',
    left: 'tooltip-left',
    right: 'tooltip-right'
  };
  
  const flyProps = {
    top: { y: 8, duration: 150, easing: cubicOut },
    bottom: { y: -8, duration: 150, easing: cubicOut },
    left: { x: 8, duration: 150, easing: cubicOut },
    right: { x: -8, duration: 150, easing: cubicOut }
  };
</script>

<div
  class="tooltip-wrapper"
  role="presentation"
  on:mouseenter={handleMouseEnter}
  on:mouseleave={handleMouseLeave}
  on:focus={handleMouseEnter}
  on:blur={handleMouseLeave}
>
  <slot />
  {#if showTooltip}
    <div
      class="tooltip {positionClasses[position]}"
      transition:fly={flyProps[position]}
      role="tooltip"
    >
      {text}
    </div>
  {/if}
</div>

<style>
  .tooltip-wrapper {
    position: relative;
    display: inline-flex;
  }

  .tooltip {
    position: absolute;
    z-index: 9999;
    padding: 0.5rem 0.75rem;
    background: color-mix(in srgb, var(--md-sys-color-surface-container-highest) 98%, transparent);
    color: var(--md-sys-color-on-surface);
    border: 1px solid var(--md-sys-color-outline);
    border-radius: var(--radius-sm);
    font-size: 0.8rem;
    font-weight: 500;
    white-space: nowrap;
    box-shadow: var(--shadow-02);
    backdrop-filter: var(--blur-elevated);
    pointer-events: none;
  }

  .tooltip-top {
    bottom: calc(100% + 8px);
    left: 50%;
    transform: translateX(-50%);
  }

  .tooltip-bottom {
    top: calc(100% + 8px);
    left: 50%;
    transform: translateX(-50%);
  }

  .tooltip-left {
    right: calc(100% + 8px);
    top: 50%;
    transform: translateY(-50%);
  }

  .tooltip-right {
    left: calc(100% + 8px);
    top: 50%;
    transform: translateY(-50%);
  }
</style>

