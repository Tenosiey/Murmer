<!-- Displays a simple connection quality indicator using five bars. -->
<script lang="ts">
  export let strength: number = 0;
  const bars = [1, 2, 3, 4, 5];
</script>

<div>
  {#each bars as n}
    {strength >= n ? '#' : '-'}
  {/each}
</div>
