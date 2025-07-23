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

  onMount(() => {
    if (!get(session).user) goto('/login');
    serverStatus.start();
  });

  onDestroy(() => {
    serverStatus.stop();
  });

  let newServer = '';
  let newName = '';
  let newPassword = '';
  let settingsOpen = false;


  function add() {
    if (newServer.trim()) {
      const entry: ServerEntry = { url: normalizeServerUrl(newServer), name: newName.trim() || newServer };
      if (newPassword.trim()) entry.password = newPassword;
      servers.add(entry);
      newServer = '';
      newName = '';
      newPassword = '';
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
</script>

<div class="servers-page">
  <div class="header">
    <h1>Servers</h1>
    <div class="actions">
      <span class="user">{$session.user}</span>
      <button class="icon" on:click={openSettings} title="Settings">⚙️</button>
      <button class="icon" on:click={logout} title="Logout">🚪</button>
    </div>
  </div>
  <SettingsModal open={settingsOpen} close={closeSettings} />
  <form class="add" on:submit|preventDefault={add}>
    <input bind:value={newName} placeholder="Server name" />
    <input bind:value={newServer} placeholder="host:port or ws://url" />
    <input type="password" bind:value={newPassword} placeholder="Password (optional)" />
    <button type="submit">Add</button>
  </form>
  <ul class="list">
    {#each $servers as server}
      <li>
        <button on:click={() => join(server)} title={server.url}>{server.name}</button>
        <StatusDot online={$serverStatus[server.url]} />
        <button class="del" on:click={() => removeServer(server.url)}>Delete</button>
      </li>
    {/each}
  </ul>
</div>

<style>
  .servers-page {
    max-width: 500px;
    margin: 2rem auto;
    padding: 1rem;
    background: var(--color-panel);
    border-radius: 8px;
    box-shadow: 0 2px 6px rgba(0, 0, 0, 0.3);
  }

  .add {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 1rem;
  }

  input {
    padding: 0.4rem;
    background: #2e2e40;
    border: 1px solid #444;
    color: var(--color-text);
    width: 100%;
  }

  .add button {
    width: 100%;
  }

  button {
    padding: 0.4rem 0.6rem;
    background: var(--color-accent);
    border: none;
    color: white;
    cursor: pointer;
    transition: background 0.2s ease;
  }

  button.del {
    background: #b91c1c;
  }

  button:hover {
    background: var(--color-accent-alt);
  }

  .list {
    list-style: none;
    padding: 0;
    margin: 0;
  }

  .list li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.25rem;
    margin-bottom: 0.25rem;
    padding: 0.4rem;
    background: #2e2e40;
    border-radius: 4px;
  }

  .list li button:first-child {
    flex-grow: 1;
    text-align: left;
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 0.5rem;
  }
  .actions {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }
  .icon {
    background: none;
    border: none;
    cursor: pointer;
    font-size: 1.2rem;
  }
</style>
