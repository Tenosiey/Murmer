<script lang="ts">
  import { onMount } from 'svelte';
  import { chat } from '$lib/stores/chat';
  import { session } from '$lib/stores/session';
  let message = '';

  onMount(() => {
    chat.connect('ws://localhost:3001/ws');
  });

  function send() {
    chat.send($session.user ?? 'anon', message);
    message = '';
  }

  function joinVoice() {
    console.log('join voice stub');
    new RTCPeerConnection();
  }
</script>

<div class="flex flex-col h-screen p-4">
  <h1 class="text-xl font-bold mb-4">Text Channel</h1>
  <div class="flex-1 overflow-y-auto mb-4 space-y-2">
    {#each $chat as msg}
      <div><b>{msg.user}:</b> {msg.text}</div>
    {/each}
  </div>
  <div class="flex space-x-2">
    <input class="flex-1 border p-2 rounded" bind:value={message} placeholder="Message" />
    <button class="bg-blue-500 text-white px-4 py-2 rounded" on:click={send}>Send</button>
  </div>
  <button class="mt-4 bg-green-500 text-white px-4 py-2 rounded self-start" on:click={joinVoice}>
    Join Voice (stub)
  </button>
</div>
