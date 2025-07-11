<script lang="ts">
  import { goto } from '$app/navigation';
  import { servers, selectedServer, type ServerEntry } from '$lib/stores/servers';
  import { session } from '$lib/stores/session';
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';
  import SettingsModal from '$lib/components/SettingsModal.svelte';

  onMount(() => {
    if (!get(session).user) goto('/login');
  });

  let newServer = '';
  let newName = '';
  let settingsOpen = false;

  function normalize(input: string): string {
    let url = input.trim();
    if (!/^wss?:\/\//.test(url)) {
      if (/^https?:\/\//.test(url)) {
        url = url.replace(/^http/, 'ws');
      } else {
        url = `ws://${url.replace(/\/$/, '')}/ws`;
      }
    }
    return url;
  }

  function add() {
    if (newServer.trim()) {
      servers.add({ url: normalize(newServer), name: newName.trim() || newServer });
      newServer = '';
      newName = '';
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

<div>
  <div>
    <h1>Servers</h1>
    <div>
      <span>{$session.user}</span>
      <button on:click={openSettings}>Settings</button>
      <button on:click={logout}>Logout</button>
    </div>
  </div>
  <SettingsModal open={settingsOpen} close={closeSettings} />
  <div>
    <input bind:value={newName} placeholder="Server name" />
    <input bind:value={newServer} placeholder="host:port or ws://url" />
    <button on:click={add}>Add</button>
  </div>
  <ul>
    {#each $servers as server}
      <li>
        <button
          on:click={() => join(server)}
          title={server.url}
        >
          {server.name}
        </button>
        <button on:click={() => removeServer(server.url)}>Delete</button>
      </li>
    {/each}
  </ul>
</div>
