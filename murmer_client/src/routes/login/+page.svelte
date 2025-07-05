<script lang="ts">
  import { session } from '$lib/stores/session';
  import { goto } from '$app/navigation';
  let username = '';
  function login() {
    session.set({ user: username });
    goto('/servers');
  }
</script>

<div class="flex flex-col items-center justify-center h-screen space-y-4">
  <h1 class="text-2xl font-bold">Login</h1>
  <input class="border p-2 rounded" bind:value={username} placeholder="Username" />
  <button class="bg-blue-500 text-white px-4 py-2 rounded" on:click={login}>Login</button>
</div>
