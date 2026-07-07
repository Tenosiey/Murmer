<!--
  Login page for Murmer. Users supply a display name that is persisted in the
  session store and then redirected to their server list. The page guards
  against authenticated users revisiting the login screen.
-->
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

<main class="login-page">
  <div class="login-column">
    <header class="login-intro">
      <div class="eyebrow">Welcome back</div>
      <h1>Sign in to Murmer</h1>
      <p class="body-muted">
        Secure voice and text for your communities. Choose a display name to get started —
        your identity key stays on this device.
      </p>
    </header>

    <form class="login-card surface-card" on:submit|preventDefault={login} aria-labelledby="login-heading">
      <h2 id="login-heading" class="sr-only">Your details</h2>
      <label class="field">
        <span>Display name</span>
        <input
          bind:value={username}
          placeholder="e.g. Phoenix"
          autocomplete="username"
          autocapitalize="none"
          spellcheck={false}
          required
        />
      </label>
      <button type="submit" class="btn btn-primary">Continue</button>
      <p class="hint">Only used inside your active server. You can change it later in Settings.</p>
    </form>

    <ul class="feature-list" aria-label="Highlights">
      <li>
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="3" y="11" width="18" height="11" rx="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>
        <span>Keyed sessions live locally — you keep control of your identity.</span>
      </li>
      <li>
        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 2a3 3 0 0 0-3 3v7a3 3 0 0 0 6 0V5a3 3 0 0 0-3-3Z"/><path d="M19 10v2a7 7 0 0 1-14 0v-2"/><line x1="12" y1="19" x2="12" y2="22"/></svg>
        <span>Jump into voice channels instantly with adaptive activation modes.</span>
      </li>
    </ul>
  </div>
</main>

<style>
  .login-page {
    min-height: 100vh;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-5);
  }

  .login-column {
    width: min(400px, 100%);
    display: flex;
    flex-direction: column;
    gap: var(--space-5);
  }

  .login-intro {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .login-intro h1 {
    font-size: var(--text-2xl);
  }

  .login-intro p {
    margin: 0;
  }

  .login-card {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    padding: var(--space-5);
  }

  .login-card .btn {
    min-height: var(--control-height-lg);
  }

  .hint {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-sm);
  }

  .feature-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .feature-list li {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    color: var(--color-muted);
    font-size: var(--text-sm);
    line-height: 1.5;
  }

  .feature-list svg {
    flex-shrink: 0;
    margin-top: 0.125rem;
    color: var(--color-primary);
  }
</style>
