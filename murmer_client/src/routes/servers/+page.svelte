<script lang="ts">
  import { goto } from '$app/navigation';
  let servers = ['Example Server'];
  function join() {
    goto('/chat');
  }
</script>

<div class="p-4">
  <h1 class="text-xl font-bold mb-4">Servers</h1>
  <ul>
    {#each servers as server}
      <li class="mb-2">
        <button class="bg-gray-200 p-2 rounded w-full text-left" on:click={join}>{server}</button>
      </li>
    {/each}
  </ul>
</div>
