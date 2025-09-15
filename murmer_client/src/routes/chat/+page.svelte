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
  import { volume, outputDeviceId, outputMuted, microphoneMuted, userVolumes, setUserVolume, voiceMode, voiceActivity, isPttActive } from '$lib/stores/settings';
  import { get } from 'svelte/store';
  import { goto } from '$app/navigation';
  import ConnectionBars from '$lib/components/ConnectionBars.svelte';
  import SettingsModal from '$lib/components/SettingsModal.svelte';
  import PingDot from '$lib/components/PingDot.svelte';
  import ContextMenu from '$lib/components/ContextMenu.svelte';
  import { ping } from '$lib/stores/ping';
  import { channels } from '$lib/stores/channels';
  import { voiceChannels } from '$lib/stores/voiceChannels';
  import { leftSidebarWidth, rightSidebarWidth, focusMode } from '$lib/stores/layout';
import { loadKeyPair, sign } from '$lib/keypair';
import { renderMarkdown } from '$lib/markdown';
import type { Message } from '$lib/types';
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

  function reactionEntries(msg: Message | undefined) {
    if (!msg) return [] as Array<{ emoji: string; users: string[] }>;
    return Object.entries(msg.reactions ?? {})
      .map(([emoji, users]) => ({ emoji, users }))
      .filter((entry) => entry.users.length > 0);
  }

  function toggleReaction(messageId: number | undefined, emoji: string, users: string[]) {
    if (typeof messageId !== 'number') return;
    const current = $session.user;
    if (!current) return;
    const hasReaction = users.includes(current);
    chat.react(messageId, emoji, hasReaction ? 'remove' : 'add');
  }

  function addReactionPrompt(messageId: number | undefined) {
    if (typeof messageId !== 'number') return;
    const emoji = prompt('React with emoji')?.trim();
    if (!emoji) return;
    chat.react(messageId, emoji, 'add');
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


  function stream(node: HTMLAudioElement, data: { stream: MediaStream, userId: string }) {
    node.srcObject = data.stream;
    
    const updateVolume = () => {
      if ($outputMuted) {
        node.volume = 0;
      } else {
        const globalVol = $volume;
        const userVol = $userVolumes[data.userId] ?? 1.0;
        node.volume = globalVol * userVol;
      }
    };
    
    const unsubVol = volume.subscribe(() => updateVolume());
    const unsubMute = outputMuted.subscribe(() => updateVolume());
    const unsubUserVol = userVolumes.subscribe(() => updateVolume());
    
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
    updateVolume(); // Initial volume setting
    
    return {
      update(newData: { stream: MediaStream, userId: string }) {
        node.srcObject = newData.stream;
        data.userId = newData.userId;
        updateVolume();
      },
      destroy() {
        unsubVol();
        unsubMute();
        unsubUserVol();
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
  let volumeMenuOpen = false;
  let volumeMenuX = 0;
  let volumeMenuY = 0;
  let volumeMenuUser: string | null = null;

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

  function openUserVolumeMenu(event: MouseEvent, user: string) {
    event.preventDefault();
    event.stopPropagation();
    volumeMenuX = event.clientX;
    volumeMenuY = event.clientY;
    volumeMenuUser = user;
    volumeMenuOpen = true;
  }

  function closeVolumeMenu() {
    volumeMenuOpen = false;
    volumeMenuUser = null;
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

  function toggleFocusMode() {
    focusMode.update((v) => !v);
  }

  function toggleMicrophone() {
    microphoneMuted.update(muted => !muted);
  }

  function toggleOutput() {
    outputMuted.update(muted => !muted);
  }

  function handleGlobalShortcut(event: KeyboardEvent) {
    if (event.defaultPrevented) return;
    const isModifier = event.ctrlKey || event.metaKey;
    if (!isModifier || !event.shiftKey || event.altKey) return;

    const key = event.key.length === 1 ? event.key.toLowerCase() : event.key;

    switch (key) {
      case 'f':
        event.preventDefault();
        toggleFocusMode();
        break;
      case 'm':
        event.preventDefault();
        toggleMicrophone();
        break;
      case 'o':
        event.preventDefault();
        toggleOutput();
        break;
      case 's':
        event.preventDefault();
        openSettings();
        break;
      case 'v':
        event.preventDefault();
        if (inVoice) {
          leaveVoice();
        } else {
          if (!currentVoiceChannel) {
            const channels = $voiceChannels;
            if (channels.length) {
              currentVoiceChannel = channels[0];
            } else {
              break;
            }
          }
          joinVoice();
        }
        break;
      default:
        return;
    }
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
    window.addEventListener('keydown', handleGlobalShortcut);
  });

  onDestroy(() => {
    window.removeEventListener('mousemove', handleMouseMove);
    window.removeEventListener('mouseup', stopResize);
    window.removeEventListener('keydown', handleGlobalShortcut);
  });
</script>

  <div class="page" class:focus={$focusMode}>
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
                <li 
                  on:contextmenu={(e) => user !== $session.user && openUserVolumeMenu(e, user)}
                  class:clickable={user !== $session.user}
                >
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
          <div class="user">{$session.user}</div>
          <div class="connection-info">
            <PingDot ping={$ping} />
            <ConnectionBars strength={serverStrength} />
          </div>
          <button
            class="action-button focus-toggle"
            class:focusActive={$focusMode}
            aria-pressed={$focusMode}
            on:click={toggleFocusMode}
            title={$focusMode ? 'Exit focus mode' : 'Enter focus mode'}
          >
            {$focusMode ? 'Restore' : 'Focus'}
          </button>
          <button class="action-button" on:click={openSettings} title="Settings">⚙️</button>
          <button class="action-button" on:click={leaveServer} title="Leave Server">⬅️</button>
          <button class="action-button danger" on:click={logout} title="Logout">🚪</button>
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
            {#if typeof msg.id === 'number'}
              <div class="reactions">
                {#each reactionEntries(msg) as reaction (reaction.emoji)}
                  <button
                    class="reaction-chip"
                    class:active={reaction.users.includes($session.user ?? '')}
                    on:click={() => toggleReaction(msg.id as number, reaction.emoji, reaction.users)}
                    title={reaction.users.join(', ')}
                  >
                    <span class="emoji">{reaction.emoji}</span>
                    <span class="count">{reaction.users.length}</span>
                  </button>
                {/each}
                <button class="reaction-chip add" on:click={() => addReactionPrompt(msg.id as number)}>+</button>
              </div>
            {/if}
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
          {#if previewUrl}
            <div class="preview-container">
              <img src={previewUrl} alt="preview" class="preview" />
              <button class="preview-remove" on:click={() => { fileInput.value = ''; if (previewUrl) URL.revokeObjectURL(previewUrl); previewUrl = null; }} aria-label="Remove image">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <line x1="18" y1="6" x2="6" y2="18"></line>
                  <line x1="6" y1="6" x2="18" y2="18"></line>
                </svg>
              </button>
            </div>
          {/if}
          <div class="input-controls">
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
                aria-hidden="true"
              >
                <path
                  stroke-linecap="round"
                  stroke-linejoin="round"
                  d="m2.25 15.75 5.159-5.159a2.25 2.25 0 0 1 3.182 0l5.159 5.159m-1.5-1.5 1.409-1.409a2.25 2.25 0 0 1 3.182 0l2.909 2.909m-18 3.75h16.5a1.5 1.5 0 0 0 1.5-1.5V6a1.5 1.5 0 0 0-1.5-1.5H3.75A1.5 1.5 0 0 0 2.25 6v12a1.5 1.5 0 0 0 1.5 1.5Zm10.5-11.25h.008v.008h-.008V8.25Zm.375 0a.375.375 0 1 1-.75 0 .375.375 0 0 1 .75 0Z"
                />
              </svg>
            </button>
            <button class="send" on:click={send} title="Send message" aria-label="Send message">
              <svg
                xmlns="http://www.w3.org/2000/svg"
                fill="currentColor"
                viewBox="0 0 24 24"
                aria-hidden="true"
              >
                <path d="M2.01 21L23 12 2.01 3 2 10l15 2-15 2z" />
              </svg>
            </button>
          </div>
        </div>
      </div>

      <div class="voice-controls-panel">
        {#if inVoice}
          <div class="voice-controls-header">Voice Controls</div>
          <div class="voice-controls-buttons">
            <button class="voice-control-btn leave" on:click={leaveVoice}>
              <span class="btn-icon">⬅️</span>
              <span class="btn-text">Leave Voice</span>
            </button>
            <button 
              class="voice-control-btn mute" 
              class:muted={$microphoneMuted}
              class:active={$voiceMode === 'vad' && $voiceActivity}
              class:ptt-active={$voiceMode === 'ptt' && $isPttActive}
              on:click={toggleMicrophone}
              title={$microphoneMuted ? 'Unmute Microphone' : 'Mute Microphone'}
            >
              <span class="btn-icon">
                {#if $microphoneMuted}
                  🎤🚫
                {:else if $voiceMode === 'vad' && $voiceActivity}
                  🎤✨
                {:else if $voiceMode === 'ptt' && $isPttActive}
                  🎤🔥
                {:else}
                  🎤
                {/if}
              </span>
              <span class="btn-text">
                {#if $microphoneMuted}
                  Unmute Mic
                {:else if $voiceMode === 'continuous'}
                  Always On
                {:else if $voiceMode === 'vad'}
                  Voice Detection
                {:else if $voiceMode === 'ptt'}
                  Push to Talk
                {:else}
                  Mute Mic
                {/if}
              </span>
            </button>
            <button 
              class="voice-control-btn mute" 
              class:muted={$outputMuted}
              on:click={toggleOutput}
              title={$outputMuted ? 'Unmute Output' : 'Mute Output'}
            >
              <span class="btn-icon">{$outputMuted ? '🔇' : '🔊'}</span>
              <span class="btn-text">{$outputMuted ? 'Unmute Out' : 'Mute Out'}</span>
            </button>
          </div>
        {:else}
          <button class="voice-control-btn join" on:click={joinVoice}>
            <span class="btn-icon">🔊</span>
            <span class="btn-text">Join Voice</span>
          </button>
        {/if}
      </div>

      {#each $voice as peer (peer.id)}
        <audio autoplay use:stream={{ stream: peer.stream, userId: peer.id }}></audio>
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

{#if volumeMenuOpen && volumeMenuUser}
  <!-- svelte-ignore a11y-no-static-element-interactions -->
  <!-- svelte-ignore a11y-click-events-have-key-events -->
  <div class="volume-menu-overlay" on:click={closeVolumeMenu}>
    <!-- svelte-ignore a11y-no-static-element-interactions -->
    <!-- svelte-ignore a11y-click-events-have-key-events -->
    <div 
      class="volume-menu" 
      style="left: {volumeMenuX}px; top: {volumeMenuY}px;"
      on:click|stopPropagation
    >
      <div class="volume-menu-header">
        <span class="volume-menu-user">{volumeMenuUser}</span>
        <span class="volume-menu-title">Volume Control</span>
      </div>
      <div class="volume-menu-content">
        <div class="volume-control-row">
          <span class="volume-icon">🔊</span>
          <input
            class="volume-menu-slider"
            type="range"
            min="0"
            max="1"
            step="0.01"
            value={$userVolumes[volumeMenuUser] ?? 1.0}
            on:input={(e) => {
              if (!volumeMenuUser) return;
              setUserVolume(volumeMenuUser, parseFloat(e.currentTarget.value));
            }}
          />
          <span class="volume-percentage">{Math.round(($userVolumes[volumeMenuUser] ?? 1.0) * 100)}%</span>
        </div>
        <div class="volume-presets">
          <button class="preset-btn" on:click={() => volumeMenuUser && setUserVolume(volumeMenuUser, 0)}>Mute</button>
          <button class="preset-btn" on:click={() => volumeMenuUser && setUserVolume(volumeMenuUser, 0.5)}>50%</button>
          <button class="preset-btn" on:click={() => volumeMenuUser && setUserVolume(volumeMenuUser, 1.0)}>100%</button>
        </div>
      </div>
    </div>
  </div>
{/if}

<style>
  .page {
    display: flex;
    height: 100vh;
    background: var(--color-bg);
  }

  .page.focus {
    padding: 0 1.5rem;
  }

  .page.focus .channels,
  .page.focus .sidebar,
  .page.focus .resizer {
    display: none;
  }

  .page.focus .chat {
    max-width: 960px;
    margin: 0 auto;
  }

  .channels {
    min-width: 80px;
    background: var(--color-panel);
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    border-right: 1px solid var(--color-border-subtle);
  }

  .channels .section {
    margin: 1rem 0 0.5rem 0;
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--color-text-subtle);
  }

  .channels button {
    width: 100%;
    padding: 0.625rem 0.75rem;
    border: none;
    background: transparent;
    color: var(--color-text-muted);
    cursor: pointer;
    text-align: left;
    border-radius: var(--radius-sm);
    font-weight: 500;
    transition: var(--transition);
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .chan-icon {
    font-size: 1rem;
    opacity: 0.8;
  }

  .channels button:hover {
    background: var(--color-bg-elevated);
    color: var(--color-text);
  }

  .channels button.active {
    background: var(--color-accent);
    color: white;
  }

  .channels button.active .chan-icon {
    opacity: 1;
  }

  .resizer {
    width: 1px;
    cursor: col-resize;
    flex-shrink: 0;
    background: var(--color-border-subtle);
    transition: var(--transition);
  }
  .resizer:hover {
    background: var(--color-accent);
    width: 2px;
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
    padding: 0.25rem;
    border-radius: var(--radius-sm);
    transition: var(--transition);
  }

  .voice-user-list li.clickable {
    cursor: context-menu;
  }

  .voice-user-list li.clickable:hover {
    background: var(--color-bg-elevated);
  }

  .chat {
    flex: 1;
    display: flex;
    flex-direction: column;
    background: var(--color-bg-elevated);
    padding: 1rem;
    gap: 1rem;
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    background: var(--color-panel-elevated);
    padding: 1rem 1.25rem;
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-sm);
    border: 1px solid var(--color-border-subtle);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .user {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 0.75rem;
    background: var(--color-panel);
    border-radius: var(--radius-sm);
    font-weight: 600;
    color: var(--color-text);
    border: 1px solid var(--color-border-subtle);
  }

  .user::before {
    content: "👤";
    font-size: 0.9rem;
    opacity: 0.8;
  }

  .connection-info {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem 0.75rem;
    background: var(--color-panel);
    border-radius: var(--radius-sm);
    border: 1px solid var(--color-border-subtle);
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
    position: relative;
  }

  .action-button.focus-toggle {
    width: auto;
    min-width: 2.5rem;
    padding: 0 0.75rem;
    font-size: 0.85rem;
    font-weight: 600;
    letter-spacing: 0.02em;
  }

  .action-button.focusActive {
    background: var(--color-accent-alt);
    color: white;
    border-color: var(--color-accent-alt);
  }

  .action-button.focusActive:hover {
    background: var(--color-accent-hover);
    border-color: var(--color-accent-hover);
    color: white;
  }

  .action-button:hover {
    background: var(--color-bg-elevated);
    color: var(--color-text);
    border-color: var(--color-border);
    transform: translateY(-1px);
    box-shadow: var(--shadow-sm);
  }

  .action-button:active {
    transform: translateY(0);
  }

  .action-button.danger {
    color: var(--color-error);
  }

  .action-button.danger:hover {
    background: var(--color-error);
    color: white;
    border-color: var(--color-error);
  }

  .messages {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    padding: 0.5rem;
    background: var(--color-panel);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border-subtle);
  }


  .input-row {
    display: flex;
    align-items: flex-end;
    gap: 0.75rem;
    background: var(--color-panel-elevated);
    padding: 1rem;
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border-subtle);
  }


  .message {
    display: flex;
    flex-direction: column;
    background: var(--color-bg-elevated);
    padding: 0.75rem 1rem;
    border-radius: var(--radius-md);
    border: 1px solid var(--color-border-subtle);
    transition: var(--transition);
  }

  .message:hover {
    background: var(--color-panel-elevated);
    border-color: var(--color-border);
  }

  .timestamp {
    font-size: 0.75rem;
    color: var(--color-text-subtle);
    margin-bottom: 0.25rem;
  }

  .username {
    font-weight: 600;
    color: var(--color-accent);
    margin-bottom: 0.25rem;
  }

  .role {
    margin-left: 0.5rem;
    font-size: 0.75rem;
    opacity: 0.8;
  }

  .content {
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  :global(.content p) {
    margin: 0;
    padding: 0;
  }
  
  :global(.content p:last-child) {
    margin-bottom: 0;
  }

  :global(.content pre) {
    background: #1e1e2e;
    padding: 0.25rem 0.5rem;
    border-radius: 4px;
    overflow-x: auto;
    margin: 0;
  }

  .content img {
    max-width: min(100%, 500px);
   max-height: 500px;
   border-radius: 4px;
   margin-top: 0.25rem;
 }

  .reactions {
    margin-top: 0.5rem;
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }

  .reaction-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    border: 1px solid var(--color-border-subtle);
    background: var(--color-panel);
    border-radius: 999px;
    padding: 0.15rem 0.5rem;
    font-size: 0.85rem;
    color: var(--color-text-muted);
    cursor: pointer;
    transition: var(--transition);
  }

  .reaction-chip .emoji {
    font-size: 1rem;
  }

  .reaction-chip .count {
    font-size: 0.7rem;
    opacity: 0.8;
  }

  .reaction-chip:hover {
    border-color: var(--color-accent);
    color: var(--color-text);
  }

  .reaction-chip.active {
    border-color: var(--color-accent);
    background: rgba(124, 58, 237, 0.12);
    color: var(--color-text);
  }

  .reaction-chip.add {
    font-weight: 600;
  }

  textarea {
    flex: 1;
    resize: none;
    padding: 0.75rem;
    background: var(--color-panel);
    border: 1px solid var(--color-border-subtle);
    color: var(--color-text);
    overflow-y: auto;
    max-height: 200px;
    border-radius: var(--radius-sm);
    transition: var(--transition);
  }

  textarea:focus {
    border-color: var(--color-accent);
    box-shadow: 0 0 0 3px rgba(124, 58, 237, 0.1);
  }

  .file-input {
    display: none;
  }

  .input-controls {
    display: flex;
    align-items: flex-end;
    gap: 0.5rem;
  }

  .file-button,
  .send {
    width: 3rem;
    height: 3rem;
    padding: 0;
    background: var(--color-accent);
    border: 1px solid var(--color-accent);
    color: white;
    cursor: pointer;
    transition: var(--transition);
    border-radius: var(--radius-md);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    box-shadow: var(--shadow-sm);
    flex-shrink: 0;
  }

  .file-button {
    background: var(--color-panel);
    color: var(--color-text-muted);
    border-color: var(--color-border);
  }

  .file-button:hover {
    background: var(--color-bg-elevated);
    color: var(--color-text);
    border-color: var(--color-border);
    transform: translateY(-1px);
    box-shadow: var(--shadow-md);
  }

  .send:hover {
    background: var(--color-accent-hover);
    border-color: var(--color-accent-hover);
    transform: translateY(-1px);
    box-shadow: var(--shadow-md);
  }

  .file-button:active,
  .send:active {
    transform: translateY(0);
    box-shadow: var(--shadow-sm);
  }

  .send svg,
  .file-button svg {
    width: 18px;
    height: 18px;
  }

  .controls {
    display: flex;
    align-items: center;
  }

  .preview-container {
    position: relative;
    margin-right: 0.75rem;
    margin-bottom: 0.5rem;
  }

  .preview {
    width: 100px;
    height: 100px;
    object-fit: cover;
    border-radius: var(--radius-md);
    border: 2px solid var(--color-border);
    box-shadow: var(--shadow-sm);
    transition: var(--transition);
  }

  .preview:hover {
    border-color: var(--color-accent);
    box-shadow: var(--shadow-md);
  }

  .preview-remove {
    position: absolute;
    top: -8px;
    right: -8px;
    width: 24px;
    height: 24px;
    background: var(--color-error);
    color: white;
    border: none;
    border-radius: 50%;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    transition: var(--transition);
    box-shadow: var(--shadow-sm);
  }

  .preview-remove:hover {
    background: #dc2626;
    transform: scale(1.1);
    box-shadow: var(--shadow-md);
  }

  .preview-remove svg {
    width: 12px;
    height: 12px;
  }


  .voice-controls-panel {
    position: fixed;
    bottom: 1.5rem;
    left: 1.5rem;
    background: var(--color-panel-elevated);
    border-radius: var(--radius-lg);
    padding: 1rem;
    box-shadow: var(--shadow-lg);
    border: 1px solid var(--color-border);
    min-width: 220px;
    backdrop-filter: blur(10px);
  }

  .voice-controls-header {
    font-size: 0.75rem;
    font-weight: 600;
    color: var(--color-text-subtle);
    margin-bottom: 0.75rem;
    text-align: center;
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .voice-controls-buttons {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .voice-control-btn {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    padding: 0.75rem 1rem;
    background: var(--color-panel);
    color: var(--color-text);
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-sm);
    cursor: pointer;
    transition: var(--transition);
    width: 100%;
    text-align: left;
    font-weight: 500;
  }

  .voice-control-btn:hover {
    background: var(--color-bg-elevated);
    border-color: var(--color-border);
    transform: translateY(-1px);
    box-shadow: var(--shadow-sm);
  }

  .voice-control-btn.join {
    background: var(--color-accent-alt);
    color: white;
    border-color: var(--color-accent-alt);
  }

  .voice-control-btn.join:hover {
    background: var(--color-accent-hover);
    border-color: var(--color-accent-hover);
  }

  .voice-control-btn.leave {
    background: var(--color-error);
    color: white;
    border-color: var(--color-error);
  }

  .voice-control-btn.leave:hover {
    background: #dc2626;
    border-color: #dc2626;
  }

  .voice-control-btn.mute.muted {
    background: var(--color-error);
    color: white;
    border-color: var(--color-error);
  }

  .voice-control-btn.mute.muted:hover {
    background: #dc2626;
    border-color: #dc2626;
  }

  .voice-control-btn.active {
    background: var(--color-accent-alt);
    color: white;
    border-color: var(--color-accent-alt);
    animation: voiceActive 0.8s ease-in-out infinite alternate;
  }

  .voice-control-btn.ptt-active {
    background: var(--color-accent);
    color: white;
    border-color: var(--color-accent);
    box-shadow: 0 0 0 3px rgba(124, 58, 237, 0.3);
  }

  @keyframes voiceActive {
    from { 
      background: var(--color-accent-alt);
      transform: scale(1);
    }
    to { 
      background: var(--color-accent-hover);
      transform: scale(1.02);
    }
  }

  .btn-icon {
    font-size: 1rem;
    width: 1.2rem;
    text-align: center;
  }

  .btn-text {
    font-size: 0.9rem;
    font-weight: 500;
  }

  .sidebar {
    min-width: 80px;
    background: var(--color-panel);
    padding: 1rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
    border-left: 1px solid var(--color-border-subtle);
  }

  .sidebar h2,
  .sidebar h3 {
    margin: 0;
    font-size: 0.75rem;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
    color: var(--color-text-subtle);
  }

  .sidebar h2 {
    font-size: 1rem;
    margin-bottom: 0.5rem;
  }

  .sidebar ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .sidebar li {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.5rem;
    border-radius: var(--radius-sm);
    transition: var(--transition);
  }

  .sidebar li:hover {
    background: var(--color-bg-elevated);
  }

  .status {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    display: inline-block;
    flex-shrink: 0;
  }

  :global(.status.online) {
    background: var(--color-success);
    box-shadow: 0 0 0 2px var(--color-panel);
  }

  :global(.status.offline) {
    background: var(--color-text-subtle);
    box-shadow: 0 0 0 2px var(--color-panel);
  }

  :global(.offline) {
    color: var(--color-text-muted);
  }

  .status.voice {
    background: var(--color-accent-alt);
    box-shadow: 0 0 0 2px var(--color-panel);
  }

  .volume-menu-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    z-index: 1001;
    background: transparent;
  }

  .volume-menu {
    position: fixed;
    background: var(--color-panel-elevated);
    border: 1px solid var(--color-border);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-lg);
    padding: 1rem;
    min-width: 220px;
    z-index: 1002;
    backdrop-filter: blur(8px);
    animation: slideIn 0.2s ease-out;
  }

  .volume-menu-header {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    margin-bottom: 0.75rem;
    padding-bottom: 0.75rem;
    border-bottom: 1px solid var(--color-border-subtle);
  }

  .volume-menu-user {
    font-weight: 600;
    color: var(--color-accent);
    font-size: 0.9rem;
  }

  .volume-menu-title {
    font-size: 0.75rem;
    color: var(--color-text-muted);
    text-transform: uppercase;
    letter-spacing: 0.5px;
  }

  .volume-menu-content {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .volume-control-row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .volume-icon {
    font-size: 1rem;
    opacity: 0.8;
    width: 1.2rem;
    text-align: center;
  }

  .volume-menu-slider {
    flex: 1;
    height: 6px;
    -webkit-appearance: none;
    appearance: none;
    background: var(--color-panel);
    border: 1px solid var(--color-border-subtle);
    border-radius: 3px;
    cursor: pointer;
    margin: 0;
    padding: 0;
    outline: none;
  }

  .volume-menu-slider::-webkit-slider-track {
    width: 100%;
    height: 6px;
    background: transparent;
    border: none;
    border-radius: 3px;
  }

  .volume-menu-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 16px;
    height: 16px;
    background: var(--color-accent);
    border-radius: 50%;
    cursor: pointer;
    border: 2px solid white;
    margin-top: -5px;
    transition: var(--transition);
    box-shadow: var(--shadow-sm);
  }

  .volume-menu-slider::-webkit-slider-thumb:hover {
    background: var(--color-accent-hover);
    transform: scale(1.1);
    box-shadow: var(--shadow-md);
  }

  .volume-menu-slider::-moz-range-track {
    width: 100%;
    height: 6px;
    background: var(--color-panel);
    border: 1px solid var(--color-border-subtle);
    border-radius: 3px;
  }

  .volume-menu-slider::-moz-range-thumb {
    width: 16px;
    height: 16px;
    background: var(--color-accent);
    border-radius: 50%;
    cursor: pointer;
    border: 2px solid white;
    transition: var(--transition);
    box-shadow: var(--shadow-sm);
  }

  .volume-menu-slider::-moz-range-thumb:hover {
    background: var(--color-accent-hover);
    transform: scale(1.1);
    box-shadow: var(--shadow-md);
  }

  .volume-percentage {
    font-size: 0.8rem;
    color: var(--color-text);
    font-weight: 600;
    min-width: 2.5rem;
    text-align: right;
  }

  .volume-presets {
    display: flex;
    gap: 0.5rem;
  }

  .preset-btn {
    flex: 1;
    padding: 0.5rem;
    background: var(--color-panel);
    border: 1px solid var(--color-border-subtle);
    color: var(--color-text);
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-size: 0.8rem;
    font-weight: 500;
    transition: var(--transition);
  }

  .preset-btn:hover {
    background: var(--color-bg-elevated);
    border-color: var(--color-border);
    transform: translateY(-1px);
    box-shadow: var(--shadow-sm);
  }

  .preset-btn:active {
    transform: translateY(0);
  }
</style>
