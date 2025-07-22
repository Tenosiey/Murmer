<!--
  Displays connection quality as five bars.
  Higher bars indicate better strength, similar to WiFi signal icons.
-->
<script lang="ts">
  export let strength: number = 0;
  const bars = [1, 2, 3, 4, 5];
</script>

<div class="bars" aria-label="Connection strength" title={`${strength}/5`}>
  {#each bars as n}
    <div class="bar" class:active={strength >= n} style={`height: ${n * 3}px`}></div>
  {/each}
</div>

<style>
  .bars {
    display: inline-flex;
    align-items: flex-end;
    gap: 2px;
  }

  .bar {
    width: 4px;
    background: #4b5563;
    opacity: 0.4;
  }

  .bar.active {
    background: #22c55e;
    opacity: 1;
  }
</style>
