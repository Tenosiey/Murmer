<script lang="ts">
  import { goto } from '$app/navigation';
  import { servers, selectedServer, type ServerEntry } from '$lib/stores/servers';
  import { session } from '$lib/stores/session';
  import { onMount } from 'svelte';
  import { get } from 'svelte/store';

  onMount(() => {
    if (!get(session).user) goto('/login');
  });

  let newServer = '';
  let newName = '';

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
</script>

<div class="p-4">
  <div class="flex items-center justify-between mb-4">
    <h1 class="text-xl font-bold">Servers</h1>
    <div class="space-x-2 flex items-center">
      <span class="text-sm">{$session.user}</span>
      <button class="bg-gray-300 px-2 py-1 rounded" on:click={logout}>Logout</button>
    </div>
  </div>
  <div class="mb-4 flex space-x-2">
    <input class="border p-2 rounded flex-1" bind:value={newName} placeholder="Server name" />
    <input class="border p-2 rounded flex-1" bind:value={newServer} placeholder="host:port or ws://url" />
    <button class="bg-blue-500 text-white px-4 py-2 rounded" on:click={add}>Add</button>
  </div>
  <ul>
    {#each $servers as server}
      <li class="mb-2 flex space-x-2">
        <button
          class="bg-gray-200 p-2 rounded flex-1 text-left"
          on:click={() => join(server)}
          title={server.url}
        >
          {server.name}
        </button>
        <button
          class="bg-red-500 text-white px-2 rounded"
          on:click={() => removeServer(server.url)}
        >
          Delete
        </button>
      </li>
    {/each}
  </ul>
</div>
