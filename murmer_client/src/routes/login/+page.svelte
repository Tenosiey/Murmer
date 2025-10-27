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

<main class="login-page page-container">
  <div class="login-grid">
    <section class="login-hero">
      <div class="eyebrow">Welcome back</div>
      <h1 class="headline-hero">Sign in to Murmer</h1>
      <p class="body-muted">
        Connect instantly with your communities using secure voice and text. Choose a display name and we will sync the rest across your servers.
      </p>
    <div class="feature-grid" role="list">
      <article class="feature-card surface-tonal surface-outline" role="listitem">
        <span class="feature-icon" aria-hidden="true">🔒</span>
        <div>
          <h3>Secure presence</h3>
          <p>Keyed sessions live locally, so you keep control of your identity.</p>
        </div>
      </article>
      <article class="feature-card surface-tonal surface-outline" role="listitem">
        <span class="feature-icon" aria-hidden="true">🎙️</span>
        <div>
          <h3>Voice ready</h3>
          <p>Jump into channels instantly with adaptive voice activation modes.</p>
        </div>
      </article>
    </div>
    </section>

    <form class="login-card surface-card" on:submit|preventDefault={login} aria-labelledby="login-heading">
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
      <button type="submit" class="button-primary">Continue</button>
      <p class="hint">You can change this later from Settings.</p>
    </form>
  </div>
</main>

<style>
  .login-page {
    position: relative;
    min-height: calc(100vh - 4rem);
  }

  .login-page::after {
    content: '';
    position: absolute;
    inset: 3rem 2rem auto auto;
    width: clamp(160px, 18vw, 220px);
    height: clamp(160px, 18vw, 220px);
    border-radius: 40%;
    background: radial-gradient(circle, rgba(137, 112, 255, 0.28) 0%, transparent 70%);
    filter: blur(0.5px);
    pointer-events: none;
  }

  .login-grid {
    display: grid;
    gap: clamp(2rem, 4vw, 5rem);
    align-items: start;
    grid-template-columns: repeat(auto-fit, minmax(280px, 420px));
  }

  .login-hero {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
    max-width: 520px;
  }

  .feature-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: 1rem;
  }

  .feature-card {
    display: flex;
    gap: 0.9rem;
    padding: 1rem 1.2rem;
    border-radius: var(--radius-md);
    align-items: flex-start;
  }

  .feature-icon {
    font-size: 1.4rem;
  }

  .feature-card h3 {
    margin: 0 0 0.35rem;
    font-size: 1.05rem;
    letter-spacing: -0.01em;
  }

  .feature-card p {
    margin: 0;
    color: var(--md-sys-color-muted);
    line-height: 1.5;
  }

  .login-card {
    position: relative;
    width: min(420px, 100%);
    display: flex;
    flex-direction: column;
    gap: 1.6rem;
    padding: clamp(2rem, 4vw, 2.9rem);
  }

  .login-card::before {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
    border: 1px solid color-mix(in srgb, var(--md-sys-color-primary) 24%, transparent);
    opacity: 0.45;
    pointer-events: none;
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
    color: var(--md-sys-color-muted);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    font-weight: 600;
    color: var(--md-sys-color-on-surface-variant);
  }

  .field span {
    font-size: 0.9rem;
    text-transform: none;
  }

  .field input {
    width: 100%;
  }

  .hint {
    margin: 0;
    color: var(--md-sys-color-muted);
    font-size: 0.9rem;
  }

  @media (max-width: 720px) {
    .login-grid {
      grid-template-columns: 1fr;
    }

    .login-hero {
      max-width: 100%;
    }
  }
  </style>
