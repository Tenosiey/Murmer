<script lang="ts">
  import { session } from '$lib/stores/session';
  import { goto } from '$app/navigation';
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  let username = '';

  onMount(() => {
    const existing = get(session).user;
    if (existing) {
      goto('/servers');
    }
  });

  function login() {
    session.set({ user: username });
    goto('/servers');
  }
</script>

<div>
  <h1>Login</h1>
  <input bind:value={username} placeholder="Username" />
  <button on:click={login}>Login</button>
</div>
