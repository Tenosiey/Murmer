<script lang="ts">
  import { goto } from '$app/navigation';
  import { servers, selectedServer, type ServerEntry } from '$lib/stores/servers';
  import { session } from '$lib/stores/session';
  import { onMount, onDestroy } from 'svelte';
  import { get } from 'svelte/store';
  import SettingsModal from '$lib/components/SettingsModal.svelte';
  import { normalizeServerUrl } from '$lib/utils';
  import { serverStatus } from '$lib/stores/serverStatus';
  import StatusDot from '$lib/components/StatusDot.svelte';
  import { createInviteLink, parseInviteLink } from '$lib/invite';

  onMount(() => {
    if (!get(session).user) goto('/login');
    serverStatus.start();
  });

  onDestroy(() => {
    serverStatus.stop();
    clearCopyTimeout();
  });

  let newServer = '';
  let newName = '';
  let newPassword = '';
  let settingsOpen = false;
  let error: string | null = null;
  let copiedServer: string | null = null;
  let copyTimeout: ReturnType<typeof setTimeout> | null = null;

  function clearCopyTimeout() {
    if (copyTimeout) {
      clearTimeout(copyTimeout);
      copyTimeout = null;
    }
  }

  function add() {
    error = null;
    const rawServer = newServer.trim();
    if (!rawServer) {
      error = 'Enter a server address or invite link.';
      return;
    }

    const trimmedName = newName.trim();
    const trimmedPassword = newPassword.trim();
    let entry: ServerEntry;

    if (rawServer.startsWith('murmer://')) {
      const parsed = parseInviteLink(rawServer);
      if (!parsed) {
        error = 'That invite link could not be parsed.';
        return;
      }
      entry = {
        url: parsed.url,
        name: trimmedName || parsed.name || parsed.url
      };
      const password = trimmedPassword || parsed.password;
      if (password) {
        entry.password = password;
      }
    } else {
      entry = {
        url: normalizeServerUrl(rawServer),
        name: trimmedName || rawServer
      };
      if (trimmedPassword) {
        entry.password = trimmedPassword;
      }
    }

    servers.add(entry);
    newServer = '';
    newName = '';
    newPassword = '';
  }

  function join(server: ServerEntry) {
    selectedServer.set(server.url);
    goto('/chat');
  }

  function removeServer(url: string) {
    servers.remove(url);
  }

  function logout() {
    session.set({ user: null });
    goto('/login');
  }

  function openSettings() {
    settingsOpen = true;
  }

  function closeSettings() {
    settingsOpen = false;
  }

  async function copyInvite(server: ServerEntry) {
    error = null;
    const invite = createInviteLink(server);
    try {
      if (typeof navigator !== 'undefined' && navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(invite);
      } else if (typeof document !== 'undefined') {
        const textarea = document.createElement('textarea');
        textarea.value = invite;
        textarea.setAttribute('readonly', '');
        textarea.style.position = 'absolute';
        textarea.style.left = '-9999px';
        document.body.appendChild(textarea);
        textarea.select();
        document.execCommand('copy');
        document.body.removeChild(textarea);
      } else {
        throw new Error('Clipboard API not available');
      }
      copiedServer = server.url;
      clearCopyTimeout();
      copyTimeout = setTimeout(() => {
        if (copiedServer === server.url) {
          copiedServer = null;
        }
        copyTimeout = null;
      }, 2000);
    } catch (err) {
      error = 'Could not copy the invite link. Please copy it manually.';
      if (import.meta.env.DEV) {
        console.error('Failed to copy invite link', err);
      }
    }
  }
</script>

<main class="servers-shell">
  <header class="page-header">
    <div class="title-group">
      <div class="eyebrow">Server hub</div>
      <h1>Choose where to connect</h1>
      <p class="lead">Curate the spaces you visit most often and jump in with a single tap.</p>
    </div>
    <div class="account-card" aria-live="polite">
      <div class="avatar" aria-hidden="true">{($session.user ?? '??').slice(0, 2).toUpperCase()}</div>
      <div class="account-meta">
        <span class="label">Signed in as</span>
        <strong>{$session.user}</strong>
      </div>
      <div class="quick-actions">
        <button type="button" class="ghost" on:click={openSettings}>
          <span aria-hidden="true">⚙️</span>
          <span class="sr-only">Open settings</span>
        </button>
        <button type="button" class="ghost danger" on:click={logout}>
          <span aria-hidden="true">⎋</span>
          <span class="sr-only">Sign out</span>
        </button>
      </div>
    </div>
  </header>

  <SettingsModal open={settingsOpen} close={closeSettings} />

  <section class="create-card" aria-labelledby="create-title">
    <div class="card-copy">
      <h2 id="create-title">Add a server</h2>
      <p>Use a Murmer server address or invite URL. Passwords stay on your device.</p>
    </div>
    <form class="add" on:submit|preventDefault={add}>
      <label class="field">
        <span>Custom name</span>
        <input bind:value={newName} placeholder="Community" />
      </label>
      <label class="field">
        <span>Address</span>
        <input bind:value={newServer} placeholder="host:port or wss://example" required />
      </label>
      <label class="field">
        <span>Password</span>
        <input type="password" bind:value={newPassword} placeholder="Optional" />
      </label>
      <button type="submit" class="primary">Save server</button>
    </form>
  </section>

  <section class="server-list" aria-live="polite">
    <header class="section-header">
      <h2>Saved servers</h2>
      <span class="count">{$servers.length} saved</span>
    </header>
    {#if $servers.length === 0}
      <div class="empty-state">
        <h3>No servers yet</h3>
        <p>Add your first server to start chatting and sharing voice rooms.</p>
      </div>
    {:else}
      <div class="grid">
        {#each $servers as server}
          <article class="server-card">
            <div class="status">
              <StatusDot online={$serverStatus[server.url]} />
              <span class="status-label">{$serverStatus[server.url] ? 'Online' : 'Checking...'}</span>
            </div>
            <h3>{server.name}</h3>
            <p class="meta" title={server.url}>{server.url}</p>
            <div class="card-actions">
              <button type="button" class="secondary" on:click={() => join(server)}>
                <span aria-hidden="true">↗</span>
                <span>Join</span>
              </button>
              <button type="button" class="ghost danger" on:click={() => removeServer(server.url)}>
                <span aria-hidden="true">🗑</span>
                <span class="sr-only">Remove {server.name}</span>
              </button>
            </div>
          </article>
        {/each}
      </div>
    {/if}
  </section>
</main>


<style>
  .servers-shell {
    width: min(1100px, 100%);
    margin: 0 auto;
    padding: clamp(2rem, 5vw, 4rem) clamp(1.5rem, 4vw, 3.5rem) 4rem;
    display: flex;
    flex-direction: column;
    gap: clamp(2rem, 5vw, 3.5rem);
  }

  .page-header {
    display: flex;
    gap: clamp(2rem, 5vw, 4rem);
    align-items: center;
    flex-wrap: wrap;
    justify-content: space-between;
  }

  .title-group {
    max-width: 540px;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .eyebrow {
    text-transform: uppercase;
    letter-spacing: 0.12em;
    font-size: 0.75rem;
    color: var(--color-secondary);
    font-weight: 600;
  }

  .title-group h1 {
    margin: 0;
    font-size: clamp(2.1rem, 4vw, 3rem);
    letter-spacing: -0.02em;
  }

  .lead {
    margin: 0;
    color: var(--color-muted);
    font-size: 1.05rem;
    line-height: 1.6;
  }

  .account-card {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 1rem;
    align-items: center;
    padding: 1.25rem 1.5rem;
    border-radius: var(--radius-lg);
    background: color-mix(in srgb, var(--color-surface-raised) 82%, transparent);
    border: 1px solid var(--color-surface-outline);
    box-shadow: var(--shadow-sm);
    min-width: 260px;
  }

  .avatar {
    width: 3rem;
    height: 3rem;
    border-radius: 1rem;
    display: grid;
    place-items: center;
    font-weight: 700;
    font-size: 1.1rem;
    background: linear-gradient(135deg, var(--color-primary), var(--color-secondary));
    color: var(--color-on-primary);
  }

  .account-meta {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .account-meta .label {
    font-size: 0.75rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-muted);
  }

  .quick-actions {
    display: flex;
    gap: 0.5rem;
  }

  .ghost,
  .secondary,
  .primary {
    border-radius: var(--radius-sm);
    font-weight: 600;
    letter-spacing: 0.01em;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    padding: 0.75rem 1.2rem;
  }

  .primary {
    background: linear-gradient(135deg, var(--color-primary), var(--color-secondary));
    color: var(--color-on-primary);
    box-shadow: 0 16px 30px rgba(97, 81, 211, 0.35);
  }

  .secondary {
    background: color-mix(in srgb, var(--color-primary) 14%, transparent);
    color: var(--color-secondary);
    border: 1px solid color-mix(in srgb, var(--color-primary) 25%, transparent);
  }

  .ghost {
    background: transparent;
    color: var(--color-muted);
    border: 1px solid transparent;
    width: 2.75rem;
    height: 2.75rem;
    padding: 0;
  }

  button.ghost:hover,
  button.secondary:hover,
  button.primary:hover {

    transform: translateY(-1px);
    box-shadow: var(--shadow-xs);
  }

  .ghost.danger {
    color: var(--color-error);
  }

  .ghost.danger:hover {
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--color-error) 60%, transparent);
  }

  .create-card {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: clamp(1.5rem, 4vw, 3rem);
    padding: clamp(1.75rem, 4vw, 2.5rem);
    border-radius: var(--radius-lg);
    background: color-mix(in srgb, var(--color-surface-elevated) 82%, transparent);
    border: 1px solid var(--color-surface-outline);
    box-shadow: var(--shadow-sm);
    backdrop-filter: var(--blur-elevated);
  }

  .card-copy {

    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .card-copy h2 {
    margin: 0;
    font-size: 1.5rem;
  }

  .card-copy p {
    margin: 0;
    color: var(--color-muted);
  }

  .add {
    display: grid;
    gap: 1rem;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    color: var(--color-on-surface-variant);
    font-weight: 500;
  }

  .field span {
    font-size: 0.9rem;
  }

  .server-list {
    display: flex;
    flex-direction: column;
    gap: 1.5rem;
  }

  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 1rem;
  }

  .section-header h2 {
    margin: 0;
    font-size: 1.5rem;
  }

  .count {
    font-size: 0.9rem;
    color: var(--color-muted);
  }

  .empty-state {
    padding: clamp(2.5rem, 6vw, 3.5rem);
    border-radius: var(--radius-lg);
    border: 1px dashed var(--color-surface-outline);
    background: color-mix(in srgb, var(--color-surface-elevated) 68%, transparent);
    text-align: center;
    display: grid;
    gap: 0.75rem;
  }

  .empty-state h3 {
    margin: 0;
    font-size: 1.35rem;
  }

  .empty-state p {
    margin: 0;
    color: var(--color-muted);
  }

  .grid {
    display: grid;
    gap: clamp(1.25rem, 3vw, 1.75rem);
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
  }

  .server-card {
    padding: 1.5rem;
    border-radius: var(--radius-lg);
    background: color-mix(in srgb, var(--color-surface-raised) 88%, transparent);
    border: 1px solid var(--color-surface-outline);
    box-shadow: var(--shadow-xs);
    display: grid;
    gap: 0.75rem;
    outline: none;
  }

  .server-card:focus-visible {
    border-color: var(--color-primary);
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--color-primary) 24%, transparent);
  }

  .server-card h3 {
    margin: 0;
    font-size: 1.25rem;
    letter-spacing: -0.01em;
  }

  .meta {
    margin: 0;
    color: var(--color-muted);
    font-family: 'JetBrains Mono', 'Menlo', 'Fira Code', monospace;
    font-size: 0.85rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .status {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    color: var(--color-muted);
    font-size: 0.85rem;
  }

  .card-actions {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.75rem;
  }

  .sr-only {
    border: 0;
    clip: rect(0 0 0 0);
    height: 1px;
    margin: -1px;
    overflow: hidden;
    padding: 0;
    position: absolute;
    width: 1px;
  }

  @media (max-width: 640px) {
    .account-card {
      width: 100%;
      grid-template-columns: auto 1fr;
      grid-template-areas:
        'avatar meta'
        'actions actions';
    }

    .quick-actions {
      grid-area: actions;
      justify-content: flex-start;
    }

    .servers-shell {
      padding-inline: clamp(1rem, 6vw, 2rem);
    }
  }
</style>
