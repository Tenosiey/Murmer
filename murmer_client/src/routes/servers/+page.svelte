<!--
  Server selector and management screen. Users can add, edit and remove server
  entries, manage invite links and open the settings modal. The view also keeps
  server reachability indicators up to date.
-->
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

<main class="servers-page page-container">
  <header class="page-header">
    <div class="title-group">
      <div class="eyebrow">Server hub</div>
      <h1 class="headline-hero">Choose where to connect</h1>
      <p class="body-muted">Curate the spaces you visit most often and jump in with a single tap.</p>
      <nav class="page-tabs" aria-label="Primary navigation">
        <a href="/servers" class="tab active" aria-current="page">Servers</a>
        <a href="/chat" class="tab">Enter chat</a>
      </nav>
    </div>
    <div class="account-card surface-card" aria-live="polite">
      <div class="avatar" aria-hidden="true">{($session.user ?? '??').slice(0, 2).toUpperCase()}</div>
      <div class="account-meta">
        <span class="label">Signed in as</span>
        <strong>{$session.user}</strong>
        <span class="meta-sub">Stay visible across devices</span>
      </div>
      <div class="quick-actions">
        <button type="button" class="ghost" on:click={openSettings}>
          <svg
            width="20"
            height="20"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path
              d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915"
            />
            <circle cx="12" cy="12" r="3" />
          </svg>
          <span class="sr-only">Open settings</span>
        </button>
        <button type="button" class="ghost danger" on:click={logout}>
          <svg
            width="20"
            height="20"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
            aria-hidden="true"
          >
            <path d="M12 2v10" />
            <path d="M18.4 6.6a9 9 0 1 1-12.77.04" />
          </svg>
          <span class="sr-only">Sign out</span>
        </button>
      </div>
    </div>
  </header>

  <SettingsModal open={settingsOpen} close={closeSettings} />

  <section class="create-card surface-card" aria-labelledby="create-title">
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
      <button type="submit" class="button-primary primary-action">Save server</button>
    </form>
  </section>

  <section class="server-list" aria-live="polite">
    <header class="section-header">
      <h2>Saved servers</h2>
      <span class="count">{$servers.length} saved</span>
    </header>
    {#if $servers.length === 0}
      <div class="empty-state surface-tonal surface-outline">
        <h3>No servers yet</h3>
        <p>Add your first server to start chatting and sharing voice rooms.</p>
      </div>
    {:else}
      <div class="grid">
        {#each $servers as server}
          <article class="server-card surface-card">
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
                <svg
                  width="18"
                  height="18"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  stroke-width="1.8"
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  aria-hidden="true"
                >
                  <path d="M10 11v6" />
                  <path d="M14 11v6" />
                  <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6" />
                  <path d="M3 6h18" />
                  <path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" />
                </svg>
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
  .servers-page {
    gap: clamp(2rem, 5vw, 3.75rem);
  }

  .page-header {
    display: flex;
    gap: clamp(2rem, 5vw, 4rem);
    align-items: center;
    flex-wrap: wrap;
    justify-content: space-between;
  }

  .title-group {
    max-width: 560px;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .page-tabs {
    display: inline-flex;
    gap: 0.5rem;
    padding: 0.4rem;
    border-radius: 999px;
    background: color-mix(in srgb, var(--md-sys-color-surface-container) 75%, transparent);
    border: 1px solid var(--md-sys-color-outline);
    width: fit-content;
  }

  .page-tabs .tab {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0.45rem 1.1rem;
    border-radius: 999px;
    color: var(--md-sys-color-muted);
    text-decoration: none;
    font-weight: 600;
    transition: all var(--transition);
  }

  .page-tabs .tab::after {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
    background: linear-gradient(135deg, color-mix(in srgb, var(--md-sys-color-primary) 20%, transparent), transparent);
    opacity: 0;
    transition: opacity var(--transition);
  }

  .page-tabs .tab:hover::after {
    opacity: 1;
  }

  .page-tabs .tab.active {
    color: var(--md-sys-color-on-surface);
    background: var(--md-sys-color-surface-container-high);
    box-shadow: var(--shadow-xs);
  }

  .account-card {
    position: relative;
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 1.1rem;
    align-items: center;
    padding: 1.4rem 1.6rem;
    overflow: hidden;
    min-width: 260px;
  }

  .account-card::after {
    content: '';
    position: absolute;
    inset: -40% -20% 40% auto;
    background: radial-gradient(circle at top right, rgba(137, 112, 255, 0.4), transparent 60%);
    pointer-events: none;
  }

  .avatar {
    width: 3rem;
    height: 3rem;
    border-radius: 1rem;
    display: grid;
    place-items: center;
    font-weight: 700;
    font-size: 1.1rem;
    background: linear-gradient(135deg, var(--md-sys-color-primary), var(--md-sys-color-secondary));
    color: var(--md-sys-color-on-primary);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.1);
  }

  .account-meta {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    position: relative;
    z-index: 1;
  }

  .account-meta .label {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--md-sys-color-muted);
  }

  .account-meta strong {
    font-size: 1.05rem;
    letter-spacing: -0.01em;
  }

  .meta-sub {
    font-size: 0.85rem;
    color: color-mix(in srgb, var(--md-sys-color-muted) 90%, transparent);
  }

  .quick-actions {
    display: flex;
    gap: 0.6rem;
    position: relative;
    z-index: 1;
  }

  .ghost,
  .secondary {
    border-radius: var(--radius-sm);
    font-weight: 600;
    letter-spacing: 0.01em;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.4rem;
    padding: 0.7rem 1.15rem;
    transition: all var(--transition);
    border: 1px solid color-mix(in srgb, var(--md-sys-color-outline-variant) 80%, transparent);
    background: color-mix(in srgb, var(--md-sys-color-surface-container-high) 88%, transparent);
    color: color-mix(in srgb, var(--md-sys-color-on-surface) 90%, var(--md-sys-color-muted) 10%);
  }

  .secondary {
    background: color-mix(in srgb, var(--md-sys-color-primary) 18%, var(--md-sys-color-surface-container) 82%);
    color: var(--md-sys-color-on-surface);
    border-color: color-mix(in srgb, var(--md-sys-color-primary) 28%, transparent);
  }

  .ghost svg {
    width: 1.1rem;
    height: 1.1rem;
  }

  .ghost:hover,
  .secondary:hover,
  .button-primary:hover {
    transform: translateY(-1px);
    box-shadow: var(--shadow-xs);
  }

  .ghost.danger {
    color: var(--md-sys-color-error);
    border-color: color-mix(in srgb, var(--md-sys-color-error) 45%, transparent);
    background: color-mix(in srgb, var(--md-sys-color-error) 16%, transparent);
  }

  .ghost.danger:hover {
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--md-sys-color-error) 55%, transparent);
  }

  .quick-actions .ghost {
    width: 2.75rem;
    height: 2.75rem;
    border-radius: 0.95rem;
    padding: 0.55rem;
  }

  .card-actions .ghost {
    width: 2.6rem;
    height: 2.6rem;
    border-radius: 0.85rem;
    padding: 0.45rem;
  }

  .create-card {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
    gap: clamp(1.5rem, 4vw, 2.8rem);
    padding: clamp(2rem, 4vw, 2.75rem);
    position: relative;
    overflow: hidden;
  }

  .create-card::after {
    content: '';
    position: absolute;
    inset: auto auto -40% -30%;
    width: 360px;
    height: 360px;
    background: radial-gradient(circle, rgba(255, 163, 215, 0.22) 0%, transparent 70%);
    pointer-events: none;
  }

  .card-copy {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    position: relative;
    z-index: 1;
  }

  .card-copy h2 {
    margin: 0;
    font-size: 1.45rem;
    letter-spacing: -0.01em;
  }

  .card-copy p {
    margin: 0;
    color: var(--md-sys-color-muted);
    line-height: 1.5;
  }

  .add {
    display: grid;
    gap: 1rem;
    position: relative;
    z-index: 1;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    font-weight: 600;
    color: var(--md-sys-color-on-surface-variant);
  }

  .field span {
    font-size: 0.85rem;
  }

  .primary-action {
    justify-self: start;
  }

  .server-list {
    display: flex;
    flex-direction: column;
    gap: 1.6rem;
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
    color: var(--md-sys-color-muted);
  }

  .empty-state {
    padding: clamp(2.4rem, 6vw, 3.6rem);
    border-radius: var(--radius-lg);
    text-align: center;
    display: grid;
    gap: 0.75rem;
  }

  .empty-state h3 {
    margin: 0;
    font-size: 1.32rem;
  }

  .empty-state p {
    margin: 0;
    color: var(--md-sys-color-muted);
  }

  .grid {
    display: grid;
    gap: clamp(1.3rem, 3vw, 1.9rem);
    grid-template-columns: repeat(auto-fit, minmax(250px, 1fr));
  }

  .server-card {
    position: relative;
    padding: 1.6rem;
    gap: 0.85rem;
    outline: none;
  }

  .server-card::after {
    content: '';
    position: absolute;
    inset: 0;
    border-radius: inherit;
    border: 1px solid color-mix(in srgb, var(--md-sys-color-outline) 65%, transparent);
    opacity: 0.4;
    pointer-events: none;
  }

  .server-card:focus-visible {
    border-color: color-mix(in srgb, var(--md-sys-color-primary) 45%, transparent);
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--md-sys-color-primary) 22%, transparent);
  }

  .server-card h3 {
    margin: 0;
    font-size: 1.22rem;
    letter-spacing: -0.01em;
    position: relative;
    z-index: 1;
  }

  .meta {
    margin: 0;
    color: var(--md-sys-color-muted);
    font-family: var(--font-mono);
    font-size: 0.82rem;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    position: relative;
    z-index: 1;
  }

  .status {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    color: var(--md-sys-color-muted);
    font-size: 0.82rem;
    position: relative;
    z-index: 1;
  }

  .card-actions {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.85rem;
    position: relative;
    z-index: 1;
  }

  .card-actions .secondary {
    flex: 1;
  }

  .card-actions .secondary span:first-child {
    font-size: 1.1rem;
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
  }
</style>
