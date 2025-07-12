<script lang="ts">
  import { onMount, onDestroy, afterUpdate, tick } from 'svelte';
  import { chat } from '$lib/stores/chat';
  import { session } from '$lib/stores/session';
  import { voice, voiceStats } from '$lib/stores/voice';
  import { selectedServer, servers } from '$lib/stores/servers';
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

  function isCode(text: string): boolean {
    return /^```[\s\S]*```$/.test(text.trim());
  }

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
    const entry = servers.get(url);
    chat.connect(url, async () => {
      const u = get(session).user;
      if (u) chat.sendRaw({ type: 'presence', user: u, password: entry?.password });
      chat.sendRaw({ type: 'join', channel: currentChannel });
      await scrollBottom();
    });
  });

  onDestroy(() => {
    chat.disconnect();
    voice.leave();
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
      chat.sendRaw({
        type: 'chat',
        user: $session.user ?? 'anon',
        image: img,
        time: new Date().toLocaleTimeString()
      });
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
    chat.sendRaw({ type: 'join', channel: ch });
    scrollBottom();
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

  function leaveServer() {
    chat.disconnect();
    voice.leave();
    selectedServer.set(null);
    goto('/servers');
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
  async function scrollBottom() {
    await tick();
    if (messagesContainer) {
      messagesContainer.scrollTop = messagesContainer.scrollHeight;
    }
  }
  let lastLength = 0;

  afterUpdate(() => {
    if (messagesContainer) {
      const filteredLength = $chat.filter(m => (m.channel ?? 'general') === currentChannel).length;
      if (filteredLength !== lastLength) {
        lastLength = filteredLength;
        messagesContainer.scrollTop = messagesContainer.scrollHeight;
      }
    }
  });
</script>

  <div class="page">
    <div class="channels">
      {#each channels as ch}
        <button
          class:active={ch === currentChannel}
          on:click={() => joinChannel(ch)}
        >
          {ch}
        </button>
      {/each}
    </div>
    <div class="chat">
      <div class="header">
        <h1>{currentChannel}</h1>
        <div class="actions">
          <span class="user">{$session.user}</span>
          <button class="icon" on:click={openSettings} title="Settings">⚙️</button>
          <button class="icon" on:click={leaveServer} title="Leave Server">⬅️</button>
          <button class="icon" on:click={logout} title="Logout">🚪</button>
        </div>
      </div>
      <SettingsModal open={settingsOpen} close={closeSettings} />
      <div class="messages" bind:this={messagesContainer}>
        {#each $chat.filter(m => (m.channel ?? 'general') === currentChannel) as msg}
          <div class="message">
            <span class="timestamp">{msg.time}</span>
            <span class="username">{msg.user}</span>
            <span class="content">
              {#if msg.text}
                {#if isCode(msg.text)}
                  <pre><code>{msg.text.trim().slice(3, -3)}</code></pre>
                {:else}
                  {msg.text}
                {/if}
              {/if}
              {#if msg.image}
                <img src={msg.image as string} alt="" />
              {/if}
            </span>
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
        <button class="send" on:click={send}>Send</button>
      </div>

      {#if inVoice}
        <button class="join-voice" on:click={leaveVoice}>Leave Voice</button>
      {:else}
        <button class="join-voice" on:click={joinVoice}>Join Voice</button>
      {/if}

      {#each $voice as peer (peer.id)}
        <audio autoplay use:stream={peer.stream}></audio>
      {/each}
  </div>
  <div class="sidebar">
      <h2>Online</h2>
      <ul>
        {#each $onlineUsers as user}
          <li><span class="status online"></span>{user}</li>
        {/each}
      </ul>
      <h2>Voice</h2>
      <ul>
        {#each $voiceUsers as user}
          <li>
            <span class="status voice"></span>
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
    height: 100vh;
  }

  .channels {
    width: 140px;
    background: var(--color-panel);
    padding: 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .channels button {
    width: 100%;
    padding: 0.4rem 0.2rem;
    border: none;
    background: transparent;
    color: var(--color-text);
    cursor: pointer;
    text-align: left;
    border-radius: 4px;
    transition: background 0.2s ease;
  }

  .channels button:hover {
    background: rgba(255, 255, 255, 0.05);
  }

  .channels button.active {
    background: var(--color-accent);
  }

  .chat {
    flex: 1;
    display: flex;
    flex-direction: column;
    background: #181825;
    padding: 0.5rem;
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
    color: var(--color-text);
    cursor: pointer;
    font-size: 1.2rem;
    transition: color 0.2s;
  }

  .icon:hover {
    color: var(--color-accent-alt);
  }

  .messages {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding-right: 0.5rem;
  }

  .message {
    display: flex;
    flex-direction: column;
    background: var(--color-panel);
    padding: 0.4rem 0.6rem;
    border-radius: 6px;
  }

  .timestamp {
    font-size: 0.75rem;
    color: #a1a1aa;
  }

  .username {
    font-weight: 600;
    color: #7c3aed;
  }

  .content {
    white-space: pre-wrap;
  }

  .content img {
    max-width: min(100%, 500px);
    max-height: 500px;
    border-radius: 4px;
    margin-top: 0.25rem;
  }

  textarea {
    width: 100%;
    resize: none;
    padding: 0.5rem;
    background: #2e2e40;
    border: 1px solid #444;
    color: var(--color-text);
  }

  .send {
    margin-left: 0.5rem;
    padding: 0.5rem 1rem;
    background: var(--color-accent);
    border: none;
    color: white;
    cursor: pointer;
    transition: background 0.2s ease;
  }

  .send:hover {
    background: var(--color-accent-alt);
  }

  .join-voice {
    position: fixed;
    bottom: 1rem;
    left: 1rem;
    padding: 0.6rem 1rem;
    background: var(--color-accent-alt);
    color: white;
    border: none;
    border-radius: 999px;
    cursor: pointer;
    box-shadow: 0 4px 10px rgba(0, 0, 0, 0.4);
    transition: background 0.2s ease, transform 0.1s;
  }

  .join-voice:hover {
    background: var(--color-accent);
    transform: scale(1.05);
  }

  .sidebar {
    width: 200px;
    margin-left: 0.5rem;
    background: var(--color-panel);
    padding: 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .sidebar ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  .sidebar li {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    margin-bottom: 0.25rem;
  }

  .status {
    width: 0.6rem;
    height: 0.6rem;
    border-radius: 50%;
    display: inline-block;
  }

  .status.online {
    background: #22c55e;
  }

  .status.voice {
    background: #0ea5e9;
  }
</style>
