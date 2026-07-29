<!--
  Displays connection quality as five bars with smooth animations.
  Higher bars indicate better strength, similar to WiFi signal icons.
  Uses Material 3 color tokens for consistent theming.
-->
<script lang="ts">
  interface Props {
    strength?: number;
    /** The connection dropped and is being repaired; see `voice/recovery.ts`. */
    reconnecting?: boolean;
  }

  let { strength = 0, reconnecting = false }: Props = $props();
  const bars = [1, 2, 3, 4, 5];

  function getBarColor(barIndex: number, currentStrength: number): string {
    if (currentStrength < barIndex) return 'inactive';
    if (currentStrength >= 4) return 'excellent';
    if (currentStrength >= 3) return 'good';
    return 'poor';
  }

  // While a repair is running the bars carry no information — the connection
  // is down, not weak — so they pulse instead of reading as "terrible signal".
  let label = $derived(reconnecting ? 'Reconnecting' : `Connection strength ${strength}/5`);
</script>

<div class="bars" class:reconnecting aria-label={label} title={reconnecting ? 'Reconnecting…' : `${strength}/5 bars`}>
  {#each bars as n}
    <div
      class="bar bar-{getBarColor(n, strength)}"
      class:active={!reconnecting && strength >= n}
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
    width: 3px;
    border-radius: var(--radius-pill);
    background: color-mix(in srgb, var(--color-on-surface-variant) 20%, transparent);
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
    background: var(--color-success);
  }

  .bar-good.active {
    background: var(--color-warning);
  }

  .bar-poor.active {
    background: var(--color-error);
  }

  .bars.reconnecting .bar {
    background: var(--color-warning);
    animation: reconnect-pulse 1.2s var(--motion-easing-standard) infinite;
  }

  /* Each bar picks the animation up a little later, so the row reads as
     activity in progress rather than a single blinking block. */
  .bars.reconnecting .bar:nth-child(2) {
    animation-delay: 0.1s;
  }
  .bars.reconnecting .bar:nth-child(3) {
    animation-delay: 0.2s;
  }
  .bars.reconnecting .bar:nth-child(4) {
    animation-delay: 0.3s;
  }
  .bars.reconnecting .bar:nth-child(5) {
    animation-delay: 0.4s;
  }

  @keyframes reconnect-pulse {
    0%,
    100% {
      opacity: 0.25;
    }
    50% {
      opacity: 1;
    }
  }

  @media (prefers-reduced-motion: reduce) {
    .bars.reconnecting .bar {
      animation: none;
      opacity: 0.6;
    }
  }
</style>
