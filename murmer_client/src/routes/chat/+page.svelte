<script lang="ts">
  import { onMount, afterUpdate } from 'svelte';
  import { chat } from '$lib/stores/chat';
  import { session } from '$lib/stores/session';
  import { voice } from '$lib/stores/voice';
  import { selectedServer } from '$lib/stores/servers';
  import { onlineUsers } from '$lib/stores/online';
  import { get } from 'svelte/store';
  let message = '';
  let inVoice = false;
  const channels = ['general', 'random'];
  let currentChannel = 'general';

  function stream(node: HTMLAudioElement, media: MediaStream) {
    node.srcObject = media;
    return {
      update(newStream: MediaStream) {
        node.srcObject = newStream;
      }
    };
  }

  onMount(() => {
    const url = get(selectedServer) ?? 'ws://localhost:3001/ws';
    chat.connect(url, () => {
      const u = get(session).user;
      if (u) chat.sendRaw({ type: 'presence', user: u });
      chat.sendRaw({ type: 'join', channel: currentChannel });
    });
  });

  function send() {
    if (message.trim() === '') return;
    chat.send($session.user ?? 'anon', message);
    message = '';
  }

  function joinChannel(ch: string) {
    if (ch === currentChannel) return;
    currentChannel = ch;
    chat.clear();
    chat.sendRaw({ type: 'join', channel: ch });
  }

  function joinVoice() {
    if ($session.user) {
      voice.join($session.user);
      inVoice = true;
    }
  }

  function leaveVoice() {
    voice.leave();
    inVoice = false;
  }

  let messagesContainer: HTMLDivElement;
  let lastLength = 0;

  afterUpdate(() => {
    if (messagesContainer && $chat.length !== lastLength) {
      lastLength = $chat.length;
      messagesContainer.scrollTop = messagesContainer.scrollHeight;
    }
  });
</script>

  <div class="flex h-screen">
    <div class="flex flex-col flex-1 p-4">
      <h1 class="text-xl font-bold mb-4">Text Channel</h1>
      <div class="mb-4 space-x-2">
        {#each channels as ch}
          <button
            class="px-2 py-1 rounded {ch === currentChannel ? 'bg-blue-500 text-white' : 'bg-gray-200'}"
            on:click={() => joinChannel(ch)}>
            {ch}
          </button>
        {/each}
      </div>
      <div class="flex-1 overflow-y-auto mb-4 space-y-2" bind:this={messagesContainer}>
        {#each $chat as msg}
          <div class="whitespace-pre-wrap"><b>{msg.user}:</b> {msg.text}</div>
        {/each}
      </div>
      <div class="flex space-x-2">
        <textarea
          class="flex-1 border p-2 rounded"
          bind:value={message}
          rows="2"
          placeholder="Message"
          on:keydown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault();
              send();
            }
          }}
        ></textarea>
        <button class="bg-blue-500 text-white px-4 py-2 rounded" on:click={send}>Send</button>
      </div>
      {#if inVoice}
        <button class="mt-4 bg-red-500 text-white px-4 py-2 rounded self-start" on:click={leaveVoice}>
          Leave Voice
        </button>
      {:else}
        <button class="mt-4 bg-green-500 text-white px-4 py-2 rounded self-start" on:click={joinVoice}>
          Join Voice
        </button>
      {/if}

      {#each $voice as peer (peer.id)}
        <audio autoplay use:stream={peer.stream}></audio>
      {/each}
    </div>
    <div class="w-48 p-4 border-l overflow-y-auto">
      <h2 class="text-lg font-bold mb-2">Online</h2>
      <ul class="space-y-1">
        {#each $onlineUsers as user}
          <li>{user}</li>
        {/each}
      </ul>
    </div>
  </div>
