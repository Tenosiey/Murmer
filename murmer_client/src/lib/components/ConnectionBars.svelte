<!--
  Displays connection quality as five bars with smooth animations.
  Higher bars indicate better strength, similar to WiFi signal icons.
  Uses Material 3 color tokens for consistent theming.
-->
<script lang="ts">
  export let strength: number = 0;
  const bars = [1, 2, 3, 4, 5];
  
  function getBarColor(barIndex: number, currentStrength: number): string {
    if (currentStrength < barIndex) return 'inactive';
    if (currentStrength >= 4) return 'excellent';
    if (currentStrength >= 3) return 'good';
    return 'poor';
  }
</script>

<div class="bars" aria-label="Connection strength {strength}/5" title="{strength}/5 bars">
  {#each bars as n}
    <div
      class="bar bar-{getBarColor(n, strength)}"
      class:active={strength >= n}
      style="height: {n * 3.5}px; transition-delay: {n * 30}ms"
    ></div>
  {/each}
</div>

<style>
  .bars {
    display: inline-flex;
    align-items: flex-end;
    gap: 2.5px;
  }

  .bar {
    width: 4px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--md-sys-color-on-surface-variant) 20%, transparent);
    opacity: 0.35;
    transition: all var(--motion-duration-medium) var(--motion-easing-standard);
    transform-origin: bottom;
  }

  .bar.active {
    opacity: 1;
    transform: scaleY(1);
  }

  .bar:not(.active) {
    transform: scaleY(0.6);
  }

  .bar-excellent.active {
    background: linear-gradient(180deg, var(--md-sys-color-success), color-mix(in srgb, var(--md-sys-color-success) 85%, #000));
    box-shadow: 0 0 4px color-mix(in srgb, var(--md-sys-color-success) 40%, transparent);
  }

  .bar-good.active {
    background: linear-gradient(180deg, var(--md-sys-color-warning), color-mix(in srgb, var(--md-sys-color-warning) 85%, #000));
  }

  .bar-poor.active {
    background: linear-gradient(180deg, var(--md-sys-color-error), color-mix(in srgb, var(--md-sys-color-error) 85%, #000));
  }
</style>
