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

<main class="login-view">
  <section class="hero">
    <div class="badge">Welcome back</div>
    <h1>Sign in to Murmer</h1>
    <p class="subtitle">
      Connect instantly with your communities using secure voice and text. Pick a display name to continue.
    </p>
  </section>
  <form class="login-card" on:submit|preventDefault={login} aria-labelledby="login-heading">
    <div class="card-header">
      <h2 id="login-heading">Your details</h2>
      <p>We only use this name inside your active server.</p>
    </div>
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
    <button type="submit" class="cta">Continue</button>
    <p class="hint">You can change this later from Settings.</p>
  </form>
</main>

<style>
  .login-view {
    min-height: calc(100vh - 4rem);
    padding: clamp(2rem, 5vw, 4rem);
    display: grid;
    gap: clamp(2rem, 4vw, 6rem);
    align-items: center;
    justify-items: center;
    grid-template-columns: repeat(auto-fit, minmax(280px, 420px));
  }

  .hero {
    display: flex;
    flex-direction: column;
    gap: 1.25rem;
    text-align: center;
    max-width: 420px;
  }

  .hero h1 {
    font-size: clamp(2rem, 4vw, 2.9rem);
    margin: 0;
    letter-spacing: -0.02em;
  }

  .subtitle {
    margin: 0;
    color: var(--color-muted);
    line-height: 1.6;
    font-size: 1rem;
  }

  .badge {
    align-self: center;
    padding: 0.35rem 0.9rem;
    border-radius: 999px;
    font-size: 0.75rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    background: color-mix(in srgb, var(--color-primary) 18%, transparent);
    color: var(--color-secondary);
    font-weight: 600;
  }

  .login-card {
    width: min(420px, 100%);
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
    padding: clamp(1.75rem, 4vw, 2.75rem);
    border-radius: var(--radius-lg);
    background: color-mix(in srgb, var(--color-surface-elevated) 78%, transparent);
    box-shadow: var(--shadow-sm);
    border: 1px solid var(--color-surface-outline);
    backdrop-filter: var(--blur-elevated);
  }

  .card-header {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .card-header h2 {
    margin: 0;
    font-size: 1.35rem;
    letter-spacing: -0.01em;
  }

  .card-header p {
    margin: 0;
    color: var(--color-muted);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    font-weight: 500;
    color: var(--color-on-surface-variant);
  }

  .field input {
    width: 100%;
  }

  .cta {
    background: linear-gradient(135deg, var(--color-primary), var(--color-secondary));
    color: var(--color-on-primary);
    padding: 0.9rem;
    border-radius: var(--radius-sm);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    font-size: 1rem;
    box-shadow: 0 18px 38px rgba(97, 81, 211, 0.35);
  }

  .cta:hover {
    transform: translateY(-1px);
    box-shadow: 0 20px 44px rgba(97, 81, 211, 0.42);
  }

  .cta:active {
    transform: translateY(0);
    box-shadow: 0 12px 24px rgba(97, 81, 211, 0.28);
  }

  .hint {
    margin: 0;
    color: var(--color-muted);
    font-size: 0.9rem;
  }

  @media (max-width: 720px) {
    .login-view {
      grid-template-columns: 1fr;
      justify-items: stretch;
    }

    .hero {
      text-align: left;
      align-items: flex-start;
    }

    .badge {
      align-self: flex-start;
    }
  }
</style>
