<script lang="ts">
  import { onMount, afterUpdate } from 'svelte';
  import { chat } from '$lib/stores/chat';
  import { session } from '$lib/stores/session';
  import { voice, voiceStats } from '$lib/stores/voice';
  import { selectedServer } from '$lib/stores/servers';
  import { onlineUsers } from '$lib/stores/online';
  import { voiceUsers } from '$lib/stores/voiceUsers';
  import { volume } from '$lib/stores/settings';
  import { get } from 'svelte/store';
  import { goto } from '$app/navigation';
  import ConnectionBars from '$lib/components/ConnectionBars.svelte';
  import SettingsModal from '$lib/components/SettingsModal.svelte';
  function strength(user: string): number {
    const stats = get(voiceStats)[user];
    return stats ? stats.strength : 0;
  }
  let message = '';
  let fileInput: HTMLInputElement;
  let inVoice = false;
  let settingsOpen = false;
  const channels = ['general', 'random'];
  let currentChannel = 'general';

  function stream(node: HTMLAudioElement, media: MediaStream) {
    node.srcObject = media;
    const unsub = volume.subscribe((v) => {
      node.volume = v;
    });
    return {
      update(newStream: MediaStream) {
        node.srcObject = newStream;
      },
      destroy() {
        unsub();
      }
    };
  }

  onMount(() => {
    if (!get(session).user) {
      goto('/login');
      return;
    }
    const url = get(selectedServer) ?? 'ws://localhost:3001/ws';
    chat.connect(url, () => {
      const u = get(session).user;
      if (u) chat.sendRaw({ type: 'presence', user: u });
      chat.sendRaw({ type: 'join', channel: currentChannel });
    });
  });

  function sendText() {
    if (message.trim() === '') return;
    chat.send($session.user ?? 'anon', message);
    message = '';
  }

  async function sendImage() {
    const file = fileInput?.files?.[0];
    if (!file) {
      if (import.meta.env.DEV) console.log('sendImage: no file selected');
      return;
    }
    const selected = get(selectedServer) ?? 'ws://localhost:3001/ws';
    const u = new URL(selected);
    // convert ws:// -> http:// and strip trailing "/ws" from the path so that
    // requests target the HTTP API root rather than the WebSocket endpoint
    u.protocol = u.protocol.replace('ws', 'http');
    if (u.pathname.endsWith('/ws')) u.pathname = u.pathname.slice(0, -3);
    const base = u.toString().replace(/\/$/, '');
    const form = new FormData();
    form.append('file', file);
    if (import.meta.env.DEV) console.log('Uploading image to', base + '/upload', file);
    try {
      const res = await fetch(base + '/upload', { method: 'POST', body: form });
      if (import.meta.env.DEV) console.log('Upload response status:', res.status);
      if (!res.ok) {
        throw new Error(`upload failed with status ${res.status}`);
      }
      const data = await res.json();
      if (import.meta.env.DEV) console.log('Upload response data:', data);
      const url = data.url as string;
      const img = url.startsWith('http') ? url : base + url;
      chat.sendRaw({ type: 'chat', user: $session.user ?? 'anon', image: img });
    } catch (e) {
      console.error('upload failed', e);
    } finally {
      if (fileInput) fileInput.value = '';
    }
  }

  async function send() {
    const file = fileInput?.files?.[0];
    const hasMessage = message.trim() !== '';
    if (!file && !hasMessage) return;
    if (file) await sendImage();
    if (hasMessage) sendText();
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

  let messagesContainer: HTMLDivElement;
  let lastLength = 0;

  afterUpdate(() => {
    if (messagesContainer && $chat.length !== lastLength) {
      lastLength = $chat.length;
      messagesContainer.scrollTop = messagesContainer.scrollHeight;
    }
  });
</script>

  <div class="page">
    <div class="chat">
      <div>
        <h1>Text Channel</h1>
        <div>
          <span>{$session.user}</span>
          <button on:click={openSettings}>Settings</button>
          <button on:click={logout}>Logout</button>
        </div>
      </div>
      <SettingsModal open={settingsOpen} close={closeSettings} />
      <div>
        {#each channels as ch}
          <button on:click={() => joinChannel(ch)}>
            {ch}
          </button>
        {/each}
      </div>
      <div bind:this={messagesContainer}>
        {#each $chat as msg}
          <div>
            <b>{msg.user}:</b>
            {#if msg.text}{msg.text}{/if}
            {#if msg.image}
              <a
                href={msg.image as string}
                target="_blank"
                rel="noopener noreferrer"
                >{msg.image}</a
              >
            {/if}
          </div>
        {/each}
      </div>
      <div>
        <textarea
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
        <input type="file" bind:this={fileInput} accept="image/*" />
        <button on:click={send}>Send</button>
      </div>
      {#if inVoice}
        <button on:click={leaveVoice}>
          Leave Voice
        </button>
      {:else}
        <button on:click={joinVoice}>
          Join Voice
        </button>
      {/if}

      {#each $voice as peer (peer.id)}
        <audio autoplay use:stream={peer.stream}></audio>
      {/each}
  </div>
  <div class="sidebar">
      <h2>Online</h2>
      <ul>
        {#each $onlineUsers as user}
          <li>{user}</li>
        {/each}
      </ul>
      <h2>Voice</h2>
      <ul>
        {#each $voiceUsers as user}
          <li>
            <span>{user}</span>
            {#if user !== $session.user}
              <ConnectionBars strength={strength(user)} />
            {/if}
          </li>
        {/each}
      </ul>
  </div>
</div>

<style>
  .page {
    display: flex;
  }
  .chat {
    flex: 1;
  }
  .sidebar {
    width: 200px;
    margin-left: 1rem;
  }
</style>
