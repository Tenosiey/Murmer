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

<form class="login-container" on:submit|preventDefault={login}>
  <h1>Login</h1>
  <input bind:value={username} placeholder="Username" />
  <button type="submit">Login</button>
</form>

<style>
  .login-container {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    max-width: 300px;
    margin: 20vh auto 0;
    padding: 1.5rem;
    background: var(--color-panel);
    border-radius: 8px;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.3);
  }

  input {
    padding: 0.5rem;
    background: #2e2e40;
    border: 1px solid #444;
    color: var(--color-text);
  }

  button {
    padding: 0.5rem;
    background: var(--color-accent);
    border: none;
    color: white;
    cursor: pointer;
    transition: background 0.2s ease;
  }

  button:hover {
    background: var(--color-accent-alt);
  }
</style>
