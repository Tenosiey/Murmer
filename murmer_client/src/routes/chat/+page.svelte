<script lang="ts">
  import { onMount, onDestroy, afterUpdate, tick } from 'svelte';
import { chat } from '$lib/stores/chat';
import { roles } from '$lib/stores/roles';
  import { session } from '$lib/stores/session';
  import { voice, voiceStats } from '$lib/stores/voice';
  import { selectedServer, servers } from '$lib/stores/servers';
  import { onlineUsers } from '$lib/stores/online';
  import { allUsers, offlineUsers } from '$lib/stores/users';
  import { voiceUsers } from '$lib/stores/voiceUsers';
  import { volume, outputDeviceId } from '$lib/stores/settings';
  import { get } from 'svelte/store';
  import { goto } from '$app/navigation';
  import ConnectionBars from '$lib/components/ConnectionBars.svelte';
  import SettingsModal from '$lib/components/SettingsModal.svelte';
  import PingDot from '$lib/components/PingDot.svelte';
  import ContextMenu from '$lib/components/ContextMenu.svelte';
  import { ping } from '$lib/stores/ping';
  import { channels } from '$lib/stores/channels';
  import { voiceChannels } from '$lib/stores/voiceChannels';
  import { leftSidebarWidth, rightSidebarWidth } from '$lib/stores/layout';
import { loadKeyPair, sign } from '$lib/keypair';
import { renderMarkdown } from '$lib/markdown';
  function pingToStrength(ms: number): number {
    return ms === 0 ? 5 : ms < 50 ? 5 : ms < 100 ? 4 : ms < 200 ? 3 : ms < 400 ? 2 : 1;
  }

  let serverStrength = 0;
  $: serverStrength = pingToStrength($ping);

  let message = '';
  let fileInput: HTMLInputElement;
  let messageInput: HTMLTextAreaElement;
  let previewUrl: string | null = null;
  let menuOpen = false;
  let menuX = 0;
  let menuY = 0;

  function handleFileChange() {
    const file = fileInput?.files?.[0];
    if (previewUrl) {
      URL.revokeObjectURL(previewUrl);
      previewUrl = null;
    }
    if (file) {
      previewUrl = URL.createObjectURL(file);
    }
  }

  function autoResize() {
    if (messageInput) {
      messageInput.style.height = 'auto';
      const h = Math.min(messageInput.scrollHeight, 400);
      messageInput.style.height = h + 'px';
    }
  }

  $: autoResize();
  let inVoice = false;
  let settingsOpen = false;
  let currentChatChannel = 'general';
  let currentVoiceChannel: string | null = null;

  $: if ($channels.length && !$channels.includes(currentChatChannel)) {
    currentChatChannel = $channels[0];
    chat.sendRaw({ type: 'join', channel: currentChatChannel });
  }


  function stream(node: HTMLAudioElement, media: MediaStream) {
    node.srcObject = media;
    const unsubVol = volume.subscribe((v) => {
      node.volume = v;
    });
    const applySink = async (id: string | null) => {
      if ((node as any).setSinkId) {
        try {
          await (node as any).setSinkId(id || '');
        } catch (e) {
          console.error('Failed to set output device', e);
        }
      }
    };
    const unsubOut = outputDeviceId.subscribe((id) => {
      applySink(id);
    });
    applySink($outputDeviceId);
    return {
      update(newStream: MediaStream) {
        node.srcObject = newStream;
      },
      destroy() {
        unsubVol();
        unsubOut();
      }
    };
  }

  onMount(() => {
    if (!get(session).user) {
      goto('/login');
      return;
    }
    roles.set({});
    const url = get(selectedServer) ?? 'ws://localhost:3001/ws';
    const entry = servers.get(url);
    chat.connect(url, async () => {
      const u = get(session).user;
      if (u) {
        const kp = loadKeyPair();
        const ts = Date.now().toString();
        chat.sendRaw({
          type: 'presence',
          user: u,
          publicKey: kp.publicKey,
          timestamp: ts,
          signature: sign(ts, kp.secretKey),
          password: entry?.password
        });
      }
      // Presence response already loads history for the default channel,
      // so avoid sending an extra join message which would duplicate chat
      // history on initial connect. Joining is still handled when the
      // user switches channels.
      ping.start();
      await scrollBottom();
    });
  });

  onDestroy(() => {
    chat.disconnect();
    if (currentVoiceChannel) {
      voice.leave(currentVoiceChannel);
    }
    ping.stop();
    roles.set({});
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
      if (previewUrl) {
        URL.revokeObjectURL(previewUrl);
        previewUrl = null;
      }
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
    if (ch === currentChatChannel) return;
    currentChatChannel = ch;
    chat.clear();
    chat.sendRaw({ type: 'join', channel: ch });
    scrollBottom();
  }

  function joinVoice() {
    if ($session.user && currentVoiceChannel) {
      voice.join($session.user, currentVoiceChannel);
      inVoice = true;
    }
  }

  function leaveVoice() {
    if (currentVoiceChannel) {
      voice.leave(currentVoiceChannel);
    }
    inVoice = false;
  }

  function leaveServer() {
    chat.disconnect();
    if (currentVoiceChannel) {
      voice.leave(currentVoiceChannel);
    }
    selectedServer.set(null);
    goto('/servers');
  }

  function createChannelPrompt() {
    const name = prompt('New channel name');
    if (name) channels.create(name);
  }

  function createVoiceChannelPrompt() {
    const name = prompt('New voice channel name');
    if (!name) return;
    voiceChannels.create(name);
    if ($session.user) {
      if (inVoice && currentVoiceChannel) {
        voice.leave(currentVoiceChannel);
      }
      currentVoiceChannel = name;
      voice.join($session.user, name);
      inVoice = true;
      scrollBottom();
    }
  }

  function joinVoiceChannel(ch: string) {
    if ($session.user) {
      if (inVoice && currentVoiceChannel) {
        voice.leave(currentVoiceChannel);
      }
      currentVoiceChannel = ch;
      voice.join($session.user, ch);
      inVoice = true;
      scrollBottom();
    }
  }

  let menuChannel: string | null = null;
  let menuVoiceChannel: string | null = null;
  function openChannelMenu(event: MouseEvent, channel?: string, voice?: boolean) {
    event.preventDefault();
    event.stopPropagation();
    menuX = event.clientX;
    menuY = event.clientY;
    menuChannel = null;
    menuVoiceChannel = null;
    if (channel) {
      if (voice) menuVoiceChannel = channel;
      else menuChannel = channel;
    }
    menuOpen = true;
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

  $: channelMenuItems = [
    { label: 'Create Text Channel', action: createChannelPrompt },
    { label: 'Create Voice Channel', action: createVoiceChannelPrompt },
    ...(menuChannel ? [{ label: 'Delete Channel', action: () => channels.remove(menuChannel!) }] : []),
    ...(menuVoiceChannel ? [{ label: 'Delete Voice Channel', action: () => voiceChannels.remove(menuVoiceChannel!) }] : [])
  ];

  let messagesContainer: HTMLDivElement;
  async function scrollBottom() {
    await tick();
    if (messagesContainer) {
      messagesContainer.scrollTop = messagesContainer.scrollHeight;
    }
  }
  let lastLength = 0;
  let loadingHistory = false;
  let prevHeight = 0;

  function earliestId(): number | null {
    let min: number | null = null;
    for (const m of $chat) {
      if ((m.channel ?? 'general') === currentChatChannel && typeof m.id === 'number') {
        if (min === null || m.id! < min) min = m.id as number;
      }
    }
    return min;
  }

  function onScroll() {
    if (!messagesContainer || loadingHistory) return;
    if (messagesContainer.scrollTop < 100) {
      const id = earliestId();
      if (id !== null && id > 1) {
        loadingHistory = true;
        prevHeight = messagesContainer.scrollHeight;
        chat.loadHistory(currentChatChannel, id);
      }
    }
  }

  chat.on('history', async () => {
    await tick();
    if (messagesContainer) {
      messagesContainer.scrollTop = messagesContainer.scrollHeight - prevHeight;
    }
    loadingHistory = false;
  });

  afterUpdate(() => {
    if (messagesContainer) {
      const filteredLength = $chat.filter(m => (m.channel ?? 'general') === currentChatChannel).length;
      if (filteredLength !== lastLength) {
        lastLength = filteredLength;
        if (!loadingHistory) {
          messagesContainer.scrollTop = messagesContainer.scrollHeight;
        }
      }
    }
  });

  let startX = 0;
  let resizingLeft = false;
  let resizingRight = false;

  function startLeftResize(e: MouseEvent) {
    resizingLeft = true;
    startX = e.clientX;
  }

  function startRightResize(e: MouseEvent) {
    resizingRight = true;
    startX = e.clientX;
  }

  function stopResize() {
    resizingLeft = false;
    resizingRight = false;
  }

  function handleMouseMove(e: MouseEvent) {
    if (resizingLeft) {
      const diff = e.clientX - startX;
      startX = e.clientX;
      leftSidebarWidth.update((w) => Math.max(80, w + diff));
    } else if (resizingRight) {
      const diff = startX - e.clientX;
      startX = e.clientX;
      rightSidebarWidth.update((w) => Math.max(80, w + diff));
    }
  }

  onMount(() => {
    window.addEventListener('mousemove', handleMouseMove);
    window.addEventListener('mouseup', stopResize);
  });

  onDestroy(() => {
    window.removeEventListener('mousemove', handleMouseMove);
    window.removeEventListener('mouseup', stopResize);
  });
</script>

  <div class="page">
    <div class="channels" role="navigation" on:contextmenu={openChannelMenu} style="width: {$leftSidebarWidth}px">
      <h3 class="section">Chat Channels</h3>
      {#each $channels as ch}
        <button
          class:active={ch === currentChatChannel}
          on:click={() => joinChannel(ch)}
          on:contextmenu={(e) => openChannelMenu(e, ch)}
        >
          <span class="chan-icon">#</span> {ch}
        </button>
      {/each}
      <h3 class="section">Voice Channels</h3>
      {#each $voiceChannels as ch}
        <div class="voice-group">
          <button on:click={() => joinVoiceChannel(ch)} on:contextmenu={(e) => openChannelMenu(e, ch, true)}>
            <span class="chan-icon">🔊</span> {ch}
          </button>
          {#if $voiceUsers[ch]?.length}
            <ul class="voice-user-list">
              {#each $voiceUsers[ch] as user}
                <li>
                  <span class="status voice"></span>
                  <span
                    class="username"
                    style={$roles[user]?.color ? `color: ${$roles[user].color}` : ''}
                    >{user}</span
                  >
                  {#if $roles[user]}
                    <span
                      class="role"
                      style={$roles[user].color ? `color: ${$roles[user].color}` : ''}
                      >{$roles[user].role}</span
                    >
                  {/if}
                  <ConnectionBars
                    strength={user === $session.user ? serverStrength : ($voiceStats[user]?.strength ?? 0)}
                  />
                </li>
              {/each}
            </ul>
          {/if}
        </div>
      {/each}
    </div>
    <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
    <div class="resizer" role="separator" aria-label="Resize channel list" on:mousedown={startLeftResize}></div>
    <div class="chat">
      <div class="header">
        <h1>{currentChatChannel}</h1>
        <div class="actions">
          <span class="user">{$session.user}</span>
          <PingDot ping={$ping} />
          <ConnectionBars strength={serverStrength} />
          <button class="icon" on:click={openSettings} title="Settings">⚙️</button>
          <button class="icon" on:click={leaveServer} title="Leave Server">⬅️</button>
          <button class="icon" on:click={logout} title="Logout">🚪</button>
        </div>
      </div>
      <SettingsModal open={settingsOpen} close={closeSettings} />
      <div class="messages" bind:this={messagesContainer} on:scroll={onScroll}>
        {#each $chat.filter(m => (m.channel ?? 'general') === currentChatChannel) as msg}
          <div class="message">
            <span class="timestamp">{msg.time}</span>
            <span class="username">{msg.user}</span>
            {#if msg.user && $roles[msg.user]}
              <span class="role" style={$roles[msg.user].color ? `color: ${$roles[msg.user].color}` : ''}>
                {$roles[msg.user].role}
              </span>
            {/if}
            <span class="content">
              {#if msg.text}
                {@html renderMarkdown(msg.text)}
              {/if}
              {#if msg.image}
                <img src={msg.image as string} alt="" />
              {/if}
            </span>
          </div>
        {/each}
      </div>
      <div class="input-row">
        <textarea
          bind:value={message}
          bind:this={messageInput}
          rows="1"
          placeholder="Message"
          on:input={autoResize}
          on:keydown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault();
              send();
            }
          }}
        ></textarea>
        <input
          id="fileInputElem"
          type="file"
          class="file-input"
          bind:this={fileInput}
          accept="image/*"
          on:change={handleFileChange}
        />
        <div class="controls">
          <button
            type="button"
            class="file-button"
            title="Upload image"
            aria-label="Upload image"
            on:click={() => fileInput.click()}
          >
            <svg
              xmlns="http://www.w3.org/2000/svg"
              fill="none"
              viewBox="0 0 24 24"
              stroke-width="1.5"
              stroke="currentColor"
              width="24"
              height="24"
              aria-hidden="true"
            >
              <path
                stroke-linecap="round"
                stroke-linejoin="round"
                d="m2.25 15.75 5.159-5.159a2.25 2.25 0 0 1 3.182 0l5.159 5.159m-1.5-1.5 1.409-1.409a2.25 2.25 0 0 1 3.182 0l2.909 2.909m-18 3.75h16.5a1.5 1.5 0 0 0 1.5-1.5V6a1.5 1.5 0 0 0-1.5-1.5H3.75A1.5 1.5 0 0 0 2.25 6v12a1.5 1.5 0 0 0 1.5 1.5Zm10.5-11.25h.008v.008h-.008V8.25Zm.375 0a.375.375 0 1 1-.75 0 .375.375 0 0 1 .75 0Z"
              />
            </svg>
          </button>
          {#if previewUrl}
            <img src={previewUrl} alt="preview" class="preview" />
          {/if}
          <button class="send" on:click={send} title="Send message" aria-label="Send message">
            <svg
              xmlns="http://www.w3.org/2000/svg"
              fill="currentColor"
              viewBox="0 0 24 24"
              width="24"
              height="24"
              aria-hidden="true"
            >
              <path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z" />
            </svg>
          </button>
        </div>
        <div class="spacer"></div>
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
    <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
    <div class="resizer" role="separator" aria-label="Resize user list" on:mousedown={startRightResize}></div>
    <div class="sidebar" style="width: {$rightSidebarWidth}px">
      <h2>Users</h2>
      <h3>Online</h3>
      <ul>
        {#each $onlineUsers as user}
          <li>
            <span class="status online"></span>
            <span
              class="username"
              style={$roles[user]?.color ? `color: ${$roles[user].color}` : ''}
              >{user}</span
            >
            {#if $roles[user]}
              <span
                class="role"
                style={$roles[user].color ? `color: ${$roles[user].color}` : ''}
                >{$roles[user].role}</span
              >
            {/if}
          </li>
        {/each}
      </ul>
      <h3>Offline</h3>
      <ul>
        {#each $offlineUsers as user}
          <li>
            <span class="status offline"></span>
            <span
              class="username"
              style={$roles[user]?.color ? `color: ${$roles[user].color}` : ''}
              >{user}</span
            >
            {#if $roles[user]}
              <span
                class="role"
                style={$roles[user].color ? `color: ${$roles[user].color}` : ''}
                >{$roles[user].role}</span
              >
            {/if}
          </li>
        {/each}
      </ul>
  </div>
</div>

<ContextMenu bind:open={menuOpen} x={menuX} y={menuY} items={channelMenuItems} />

<style>
  .page {
    display: flex;
    height: 100vh;
  }

  .channels {
    min-width: 80px;
    background: var(--color-panel);
    padding: 0.5rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
  }

  .channels .section {
    margin: 0.2rem 0;
    font-size: 0.9rem;
    color: #aaa;
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

  .chan-icon {
    margin-right: 0.25rem;
  }

  .channels button:hover {
    background: rgba(255, 255, 255, 0.05);
  }

  .channels button.active {
    background: var(--color-accent);
  }

  .resizer {
    width: 1px;
    cursor: col-resize;
    flex-shrink: 0;
    background: transparent;
    transition: background 0.2s;
  }
  .resizer:hover {
    background: var(--color-accent-alt);
  }

  .voice-group {
    display: flex;
    flex-direction: column;
    margin-bottom: 0.25rem;
  }

  .voice-user-list {
    list-style: none;
    margin: 0.25rem 0 0 1rem;
    padding: 0;
  }

  .voice-user-list li {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    margin-bottom: 0.25rem;
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
    background: var(--color-panel);
    padding: 0.5rem 0.75rem;
    border-radius: 6px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4);
    margin-bottom: 0.75rem;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .user {
    font-weight: 600;
    margin-right: 0.25rem;
  }

  .icon {
    background: none;
    border: none;
    color: var(--color-text);
    cursor: pointer;
    font-size: 1.3rem;
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
    padding-bottom: 0.5rem;
    scrollbar-width: thin;
  }

  .messages::-webkit-scrollbar {
    width: 8px;
    height: 8px;
  }

  .messages::-webkit-scrollbar-track {
    background: var(--color-panel);
  }

  .messages::-webkit-scrollbar-thumb {
    background: var(--color-accent);
    border-radius: 4px;
  }

  .messages::-webkit-scrollbar-thumb:hover {
    background: var(--color-accent-alt);
  }

  .input-row {
    display: flex;
    padding-right: 0.5rem;
    align-items: flex-end;
  }

  .spacer {
    width: 0.5rem;
    flex-shrink: 0;
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

  .role {
    margin-left: 0.25rem;
    font-size: 0.75rem;
  }

  .content {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .content pre {
    background: #1e1e2e;
    padding: 0.25rem 0.5rem;
    border-radius: 4px;
    overflow-x: auto;
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
    overflow-y: auto;
    max-height: 400px;
  }

  .file-input {
    display: none;
  }

  .file-button,
  .send {
    margin-left: 0.5rem;
    width: 2.5rem;
    height: 2.5rem;
    padding: 0;
    background: var(--color-accent);
    border: none;
    color: white;
    cursor: pointer;
    transition: background 0.2s ease;
    border-radius: 4px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .file-button:hover,
  .send:hover {
    background: var(--color-accent-alt);
  }

  .controls {
    display: flex;
    align-items: center;
  }

  .preview {
    margin-left: 0.5rem;
    max-width: 80px;
    max-height: 80px;
    border-radius: 4px;
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
    min-width: 80px;
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

  :global(.status.online) {
    background: #22c55e;
  }

  :global(.status.offline) {
    background: #6b7280;
  }

  :global(.offline) {
    color: #9ca3af;
  }

  .status.voice {
    background: #0ea5e9;
  }
</style>
