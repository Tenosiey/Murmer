<script lang="ts">
  export let strength: number = 0;
  const bars = [1, 2, 3, 4, 5];
</script>

<div class="flex items-end ml-1">
  {#each bars as n}
    <div
      class="w-1 mx-px bg-gray-400"
      class:bg-green-500={strength >= n}
      style="height: {n * 0.25}rem"
    ></div>
  {/each}
</div>
