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
  import { serverIdentityCache } from '$lib/stores/serverIdentity';
  import { httpBaseFromWs } from '$lib/server-url';
  import { connectionError } from '$lib/stores/connection';
  import StatusDot from '$lib/components/StatusDot.svelte';
  import { createInviteLink, parseInviteLink } from '$lib/invite';

  onMount(() => {
    if (!get(session).user) goto('/login');
    serverStatus.start();
    // Surface the reason we were sent back here (wrong password, ban, ...).
    const carried = get(connectionError);
    if (carried) {
      error = carried;
      connectionError.set(null);
    }
  });

  onDestroy(() => {
    serverStatus.stop();
    clearCopyTimeout();
  });

  let newServer = $state('');
  let newName = $state('');
  let newPassword = $state('');
  let settingsOpen = $state(false);
  let error: string | null = $state(null);
  let copiedServer: string | null = $state(null);
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

  /** HTTP base for a stored server URL, for identity icon images. */
  function httpBase(url: string): string {
    try {
      return httpBaseFromWs(url);
    } catch {
      return '';
    }
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
      await navigator.clipboard.writeText(invite);
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
      <h1>Choose where to connect</h1>
      <p class="body-muted">Curate the spaces you visit most often and jump in with a single click.</p>
    </div>
    <div class="account-card surface-card" aria-live="polite">
      <div class="avatar" aria-hidden="true">{($session.user ?? '??').slice(0, 2).toUpperCase()}</div>
      <div class="account-meta">
        <span class="label">Signed in as</span>
        <strong>{$session.user}</strong>
      </div>
      <div class="quick-actions">
        <button type="button" class="icon-btn" onclick={openSettings}>
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
        <button type="button" class="icon-btn danger" onclick={logout}>
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

  {#if error}
    <div class="error-banner" role="alert">{error}</div>
  {/if}

  <section class="create-card surface-card" aria-labelledby="create-title">
    <div class="card-copy">
      <h2 id="create-title">Add a server</h2>
      <p>Use a Murmer server address or invite URL. Passwords stay on your device.</p>
    </div>
    <form class="add" onsubmit={(event) => { event.preventDefault(); add(); }}>
      <label class="field address-field">
        <span>Address</span>
        <input bind:value={newServer} placeholder="host:port or wss://example" required />
      </label>
      <label class="field">
        <span>Custom name</span>
        <input bind:value={newName} placeholder="Community" />
      </label>
      <label class="field">
        <span>Password</span>
        <input type="password" bind:value={newPassword} placeholder="Optional" />
      </label>
      <button type="submit" class="btn btn-primary primary-action">Save server</button>
    </form>
  </section>

  <section class="server-list" aria-live="polite">
    <header class="section-header">
      <h2>Saved servers</h2>
      <span class="count">{$servers.length} saved</span>
    </header>
    {#if $servers.length === 0}
      <div class="empty-state surface-card">
        <h3>No servers yet</h3>
        <p>Add your first server to start chatting and sharing voice rooms.</p>
      </div>
    {:else}
      <div class="grid">
        {#each $servers as server}
          {@const identity = $serverIdentityCache[server.url]}
          <article class="server-card surface-card">
            <div class="status">
              <StatusDot online={$serverStatus[server.url]} />
              <span class="status-label">{$serverStatus[server.url] === null ? 'Checking...' : $serverStatus[server.url] ? 'Online' : 'Offline'}</span>
            </div>
            <div class="card-title">
              {#if identity?.icon && httpBase(server.url)}
                <img
                  class="server-icon"
                  src={httpBase(server.url) + identity.icon}
                  alt=""
                  width="28"
                  height="28"
                  loading="lazy"
                />
              {/if}
              <h3>{identity?.name || server.name}</h3>
            </div>
            <p class="meta" title={server.url}>{server.url}</p>
            {#if identity?.description}
              <p class="description">{identity.description}</p>
            {/if}
            <div class="card-actions">
              <button type="button" class="btn btn-primary join" onclick={() => join(server)}>
                Join
              </button>
              <button
                type="button"
                class="icon-btn"
                onclick={() => copyInvite(server)}
                title={copiedServer === server.url ? 'Copied!' : 'Copy invite link'}
              >
                {#if copiedServer === server.url}
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M20 6 9 17l-5-5"/></svg>
                {:else}
                  <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>
                {/if}
                <span class="sr-only">{copiedServer === server.url ? 'Copied' : 'Copy invite link'}</span>
              </button>
              <button type="button" class="icon-btn danger" onclick={() => removeServer(server.url)}>
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
  .page-header {
    display: flex;
    gap: var(--space-5);
    align-items: flex-start;
    flex-wrap: wrap;
    justify-content: space-between;
  }

  .title-group {
    max-width: 32rem;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .title-group h1 {
    font-size: var(--text-2xl);
  }

  .title-group p {
    margin: 0;
  }

  .account-card {
    display: flex;
    gap: var(--space-3);
    align-items: center;
    padding: var(--space-3) var(--space-4);
  }

  .avatar {
    width: var(--control-height-lg);
    height: var(--control-height-lg);
    border-radius: var(--radius-md);
    display: grid;
    place-items: center;
    font-weight: 600;
    font-size: var(--text-md);
    background: var(--color-primary-container);
    color: var(--color-primary);
    flex-shrink: 0;
  }

  .account-meta {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin-right: var(--space-3);
  }

  .account-meta .label {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .account-meta strong {
    font-size: var(--text-md);
  }

  .quick-actions {
    display: flex;
    gap: var(--space-1);
  }

  .create-card {
    display: grid;
    grid-template-columns: minmax(220px, 1fr) 2fr;
    gap: var(--space-5);
    padding: var(--space-5);
  }

  .card-copy {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .card-copy h2 {
    font-size: var(--text-lg);
  }

  .card-copy p {
    margin: 0;
    color: var(--color-muted);
  }

  .add {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-4);
    align-items: end;
  }

  .add .address-field {
    grid-column: 1 / -1;
  }

  .primary-action {
    justify-self: start;
  }

  .server-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }

  .section-header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: var(--space-3);
  }

  .section-header h2 {
    font-size: var(--text-lg);
  }

  .count {
    font-size: var(--text-sm);
    color: var(--color-muted);
  }

  .empty-state {
    padding: var(--space-7) var(--space-5);
    text-align: center;
    display: grid;
    gap: var(--space-2);
  }

  .empty-state h3 {
    font-size: var(--text-lg);
  }

  .empty-state p {
    margin: 0;
    color: var(--color-muted);
  }

  .grid {
    display: grid;
    gap: var(--space-4);
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
  }

  .server-card {
    padding: var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }

  .card-title {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .server-icon {
    width: 1.75rem;
    height: 1.75rem;
    border-radius: var(--radius-sm);
    object-fit: cover;
    flex-shrink: 0;
  }

  .server-card h3 {
    font-size: var(--text-lg);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .server-card .description {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-sm);
    line-height: 1.4;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }

  .meta {
    margin: 0;
    color: var(--color-muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .status {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-muted);
    font-size: var(--text-sm);
  }

  .card-actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-top: var(--space-3);
  }

  .card-actions .join {
    flex: 1;
  }

  .error-banner {
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-md);
    font-size: var(--text-md);
    font-weight: 500;
    border: 1px solid color-mix(in srgb, var(--color-error) 35%, transparent);
    background: color-mix(in srgb, var(--color-error) 12%, transparent);
    color: var(--color-error);
  }

  @media (max-width: 720px) {
    .create-card {
      grid-template-columns: 1fr;
    }

    .add {
      grid-template-columns: 1fr;
    }

    .account-card {
      width: 100%;
    }
  }
</style>
