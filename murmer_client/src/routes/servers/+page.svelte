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

<div class="servers-page">
  <div class="header">
    <h1>Servers</h1>
    <div class="actions">
      <span class="user">{$session.user}</span>
      <button class="action-button" on:click={openSettings} title="Settings">⚙️</button>
      <button class="action-button danger" on:click={logout} title="Logout">🚪</button>
    </div>
  </div>
  <SettingsModal open={settingsOpen} close={closeSettings} />
  <form class="add" on:submit|preventDefault={add}>
    <input bind:value={newName} placeholder="Server name" />
    <input bind:value={newServer} placeholder="host:port, ws://url or invite link" />
    <input type="password" bind:value={newPassword} placeholder="Password (optional)" />
    <button type="submit">Add</button>
  </form>
  {#if error}
    <div class="error">{error}</div>
  {/if}
  <ul class="list">
    {#each $servers as server}
      <li>
        <button on:click={() => join(server)} title={server.url}>{server.name}</button>
        <StatusDot online={$serverStatus[server.url]} />
        <button class="secondary" type="button" on:click={() => copyInvite(server)}>Copy invite</button>
        {#if copiedServer === server.url}
          <span class="copied">Copied!</span>
        {/if}
        <button class="del" on:click={() => removeServer(server.url)}>Delete</button>
      </li>
    {/each}
  </ul>
</div>

<style>
  .servers-page {
    max-width: 600px;
    margin: 2rem auto;
    padding: 2rem;
    background: var(--color-panel-elevated);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    border: 1px solid var(--color-border);
  }

  .add {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 1rem;
    margin-bottom: 2rem;
    padding: 1.5rem;
    background: var(--color-panel);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border-subtle);
  }

  input {
    padding: 0.75rem;
    background: var(--color-bg-elevated);
    border: 1px solid var(--color-border-subtle);
    color: var(--color-text);
    width: 100%;
    border-radius: var(--radius-sm);
    transition: var(--transition);
  }

  .add button {
    width: 100%;
  }

  button {
    padding: 0.75rem 1rem;
    background: var(--color-accent);
    border: none;
    color: white;
    cursor: pointer;
    border-radius: var(--radius-sm);
    font-weight: 500;
    transition: var(--transition);
  }

  button.del {
    background: var(--color-error);
  }

  button.secondary {
    background: var(--color-panel-elevated);
    color: var(--color-text);
    border: 1px solid var(--color-border-subtle);
  }

  button.secondary:hover {
    background: var(--color-bg-elevated);
    border-color: var(--color-border);
  }

  button:hover {
    background: var(--color-accent-hover);
    transform: translateY(-1px);
    box-shadow: var(--shadow-sm);
  }

  button.del:hover {
    background: #dc2626;
  }

  .error {
    margin-top: 1rem;
    padding: 0.75rem 1rem;
    background: rgba(239, 68, 68, 0.15);
    border: 1px solid rgba(239, 68, 68, 0.35);
    border-radius: var(--radius-sm);
    color: var(--color-error);
  }

  .copied {
    color: var(--color-success);
    font-size: 0.9rem;
    font-weight: 600;
  }

  .list {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .list li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    padding: 1rem;
    background: var(--color-panel);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border-subtle);
    transition: var(--transition);
  }

  .list li:hover {
    background: var(--color-bg-elevated);
    border-color: var(--color-border);
  }

  .list li button:first-child {
    flex-grow: 1;
    text-align: left;
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 2rem;
    padding-bottom: 1rem;
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .header h1 {
    margin: 0;
    color: var(--color-text);
    font-size: 1.5rem;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 1rem;
  }

  .user {
    font-weight: 600;
    color: var(--color-text-muted);
  }

  .action-button {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 2.5rem;
    height: 2.5rem;
    background: var(--color-panel);
    border: 1px solid var(--color-border-subtle);
    color: var(--color-text-muted);
    cursor: pointer;
    font-size: 1.1rem;
    transition: var(--transition);
    border-radius: var(--radius-sm);
  }

  .action-button:hover {
    background: var(--color-bg-elevated);
    color: var(--color-text);
    border-color: var(--color-border);
    transform: translateY(-1px);
    box-shadow: var(--shadow-sm);
  }

  .action-button.danger {
    color: var(--color-error);
  }

  .action-button.danger:hover {
    background: var(--color-error);
    color: white;
    border-color: var(--color-error);
  }
</style>
