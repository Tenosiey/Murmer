<!--
  Primary chat surface. Handles WebSocket lifecycle, message rendering, voice
  channel state and peripheral UI such as sidebars and context menus. The
  module coordinates many Svelte stores to keep the interface reactive.
-->
<script lang="ts">
  import { onMount, onDestroy, afterUpdate, tick } from 'svelte';
  import { chat } from '$lib/stores/chat';
  import { roles } from '$lib/stores/roles';
  import { session } from '$lib/stores/session';
  import { voice } from '$lib/stores/voice';
  import { selectedServer, servers } from '$lib/stores/servers';
  import { onlineUsers } from '$lib/stores/online';
  import { offlineUsers } from '$lib/stores/users';
  import { volume, outputDeviceId, outputMuted, microphoneMuted, userVolumes } from '$lib/stores/settings';
  import { setRemoteSpeaking } from '$lib/stores/voiceSpeaking';
  import { get } from 'svelte/store';
  import { goto } from '$app/navigation';
  import SettingsModal from '$lib/components/SettingsModal.svelte';
  import LinkPreview from '$lib/components/LinkPreview.svelte';
  import ContextMenu from '$lib/components/ContextMenu.svelte';
  import SearchOverlay from '$lib/components/SearchOverlay.svelte';
  import HelpOverlay from '$lib/components/HelpOverlay.svelte';
  import VolumeMenu from '$lib/components/VolumeMenu.svelte';
  import ChannelSidebar from '$lib/components/chat/ChannelSidebar.svelte';
  import ChatHeader from '$lib/components/chat/ChatHeader.svelte';
  import PinnedBar from '$lib/components/chat/PinnedBar.svelte';
  import UserList from '$lib/components/chat/UserList.svelte';
  import { ping } from '$lib/stores/ping';
  import { channels } from '$lib/stores/channels';
  import { voiceChannels } from '$lib/stores/voiceChannels';
  import { categories } from '$lib/stores/categories';
  import type { CategoryInfo } from '$lib/types';
  import { leftSidebarWidth, rightSidebarWidth, focusMode } from '$lib/stores/layout';
  import { channelTopics } from '$lib/stores/channelTopics';
  import { statuses, STATUS_LABELS, USER_STATUS_VALUES } from '$lib/stores/status';
  import { pinned } from '$lib/stores/pins';
  import type { PinnedEntry } from '$lib/stores/pins';
  import { screenSharePeers, viewScreenShare, leaveScreenShareAsViewer } from '$lib/stores/screenShare';
  import ScreenShareViewer from '$lib/components/ScreenShareViewer.svelte';
  import { loadKeyPair, sign } from '$lib/keypair';
  import { renderMarkdown } from '$lib/markdown';
  import type { Message, UserStatus, ScreenSharePeer } from '$lib/types';
  import {
    pingToStrength,
    buildMessageBlocks,
    describeDuration,
    ephemeralInfo,
    formatFileSize,
    promptVoicePreset,
    reactionEntries,
    type MessageBlock
  } from '$lib/chat/helpers';
  import {
    MODERATOR_ROLES,
    MESSAGE_INPUT_MAX_HEIGHT,
    MAX_TOPIC_LENGTH,
    MIN_EPHEMERAL_SECONDS,
    MAX_EPHEMERAL_SECONDS,
    VOICE_QUALITY_PRESETS
  } from '$lib/chat/constants';

  let serverStrength = 0;
  $: serverStrength = pingToStrength($ping);

  let message = '';
  let fileInput: HTMLInputElement;
  let messageInput: HTMLTextAreaElement;
  let inputScrollable = false;
  let previewUrl: string | null = null;
  let pendingFile: File | null = null;
  let dragDepth = 0;
  $: dragActive = dragDepth > 0;
  let menuOpen = false;
  let menuX = 0;
  let menuY = 0;
  let statusMap: Record<string, UserStatus> = {};

  let pinnedEntries: PinnedEntry[] = [];
  let highlightedMessageId: number | null = null;
  let pendingScrollToMessage: number | null = null;
  let highlightTimer: ReturnType<typeof setTimeout> | null = null;
  let currentUserCanModerate = false;

  let commandFeedback: string | null = null;
  let commandFeedbackType: 'info' | 'error' = 'info';
  let feedbackTimer: ReturnType<typeof setTimeout> | null = null;

  let searchOpen = false;
  let searchOverlay: SearchOverlay;

  let helpOpen = false;
  let helpOverlay: HelpOverlay;

  let now = Date.now();
  let expiryTicker: number | null = null;

  let viewingScreenShare: ScreenSharePeer | null = null;
  let pendingScreenShareView: string | null = null;

  let channelMessages: Message[] = [];
  let messageBlocks: MessageBlock[] = [];

  function setCommandFeedback(message: string, type: 'info' | 'error' = 'info') {
    commandFeedback = message;
    commandFeedbackType = type;
    if (feedbackTimer) {
      clearTimeout(feedbackTimer);
    }
    feedbackTimer = setTimeout(() => {
      commandFeedback = null;
      feedbackTimer = null;
    }, 4000);
  }

  function clearCommandFeedback() {
    if (feedbackTimer) {
      clearTimeout(feedbackTimer);
      feedbackTimer = null;
    }
    commandFeedback = null;
    commandFeedbackType = 'info';
  }

  function openHelp() {
    clearCommandFeedback();
    helpOpen = true;
    helpOverlay?.focusPanel();
  }

  function closeHelp() {
    helpOpen = false;
  }

  function setPendingFile(file: File | null) {
    if (previewUrl) {
      URL.revokeObjectURL(previewUrl);
      previewUrl = null;
    }
    pendingFile = file;
    if (pendingFile && pendingFile.type.startsWith('image/')) {
      previewUrl = URL.createObjectURL(pendingFile);
    }
  }

  function handleFileChange() {
    setPendingFile(fileInput?.files?.[0] ?? null);
  }

  function clearPendingFile() {
    if (fileInput) fileInput.value = '';
    setPendingFile(null);
  }

  function handlePaste(event: ClipboardEvent) {
    const items = event.clipboardData?.items;
    if (!items) return;
    for (const item of items) {
      if (item.kind === 'file') {
        const file = item.getAsFile();
        if (file) {
          event.preventDefault();
          setPendingFile(file);
          return;
        }
      }
    }
  }

  function dragHasFiles(event: DragEvent): boolean {
    return Array.from(event.dataTransfer?.types ?? []).includes('Files');
  }

  function handleDragEnter(event: DragEvent) {
    if (!dragHasFiles(event)) return;
    event.preventDefault();
    dragDepth += 1;
  }

  function handleDragOver(event: DragEvent) {
    if (!dragHasFiles(event)) return;
    event.preventDefault();
    if (event.dataTransfer) event.dataTransfer.dropEffect = 'copy';
  }

  function handleDragLeave(event: DragEvent) {
    if (!dragHasFiles(event)) return;
    dragDepth = Math.max(0, dragDepth - 1);
  }

  function handleDrop(event: DragEvent) {
    if (!dragHasFiles(event)) return;
    event.preventDefault();
    dragDepth = 0;
    const file = Array.from(event.dataTransfer?.files ?? [])[0] ?? null;
    if (file) setPendingFile(file);
  }

  function autoResize() {
    if (messageInput) {
      messageInput.style.height = 'auto';
      const h = Math.min(messageInput.scrollHeight, MESSAGE_INPUT_MAX_HEIGHT);
      messageInput.style.height = h + 'px';
      inputScrollable = messageInput.scrollHeight > h;
    } else {
      inputScrollable = false;
    }
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

  $: statusMap = (() => {
    const map: Record<string, UserStatus> = { ...$statuses };
    for (const user of $onlineUsers) {
      if (!map[user]) {
        map[user] = 'online';
      }
    }
    for (const user of $offlineUsers) {
      if (!map[user]) {
        map[user] = 'offline';
      }
    }
    return map;
  })();

  $: currentUserCanModerate = (() => {
    const user = $session.user;
    if (!user) return false;
    const info = $roles[user];
    if (!info) return false;
    return MODERATOR_ROLES.some((role) => info.role?.toLowerCase() === role.toLowerCase());
  })();

  $: autoResize();
  let inVoice = false;
  let settingsOpen = false;
  let currentChatChannelId: number = 0;
  let initialChannelSet = false;
  let currentVoiceChannelId: number | null = null;
  let currentTopic = '';
  $: currentChatChannelName = $channels.find(c => c.id === currentChatChannelId)?.name ?? '';

  $: if (pendingScreenShareView && $screenSharePeers) {
    const peer = $screenSharePeers.find(p => p.userId === pendingScreenShareView);
    if (peer) {
      viewingScreenShare = peer;
      pendingScreenShareView = null;
    }
  }

  $: if (viewingScreenShare && $screenSharePeers) {
    if (!$screenSharePeers.find(p => p.userId === viewingScreenShare?.userId)) {
      viewingScreenShare = null;
    }
  }

  $: channelMessages = $chat.filter((m) => m.channelId === currentChatChannelId);
  $: messageBlocks = buildMessageBlocks(channelMessages);
  $: pinnedEntries = $pinned[currentChatChannelId] ?? [];

  $: if ($channels.length && !$channels.some((c) => c.id === currentChatChannelId)) {
    currentChatChannelId = $channels[0].id;
    loadingHistory = false;
    if (initialChannelSet) {
      chat.sendRaw({ type: 'join', channelId: currentChatChannelId });
    } else {
      initialChannelSet = true;
    }
  }

  $: currentTopic = $channelTopics[currentChatChannelId] ?? '';

  function stream(node: HTMLAudioElement, data: { stream: MediaStream, userId: string }) {
    node.srcObject = data.stream;
    let currentUserId = data.userId;

    let audioContext: AudioContext | null = null;
    let analyser: AnalyserNode | null = null;
    let sourceNode: MediaStreamAudioSourceNode | null = null;
    let frameId: number | null = null;
    let buffer: Uint8Array<ArrayBuffer> | null = null;

    const updateVolume = () => {
      if ($outputMuted) {
        node.volume = 0;
      } else {
        const globalVol = $volume;
        const userVol = $userVolumes[currentUserId] ?? 1.0;
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

    const stopMeter = () => {
      if (frameId !== null) {
        cancelAnimationFrame(frameId);
        frameId = null;
      }
      if (sourceNode) {
        try {
          sourceNode.disconnect();
        } catch (err) {
          if (import.meta.env.DEV) console.warn('Failed to disconnect source node', err);
        }
        sourceNode = null;
      }
      if (analyser) {
        try {
          analyser.disconnect();
        } catch (err) {
          if (import.meta.env.DEV) console.warn('Failed to disconnect analyser', err);
        }
        analyser = null;
      }
      buffer = null;
      if (audioContext) {
        audioContext.close().catch((err) => {
          if (import.meta.env.DEV) console.warn('Failed to close audio context', err);
        });
        audioContext = null;
      }
      setRemoteSpeaking(currentUserId, false);
    };

    const startMeter = (stream: MediaStream | null | undefined) => {
      stopMeter();
      if (!stream) return;
      const Ctor: typeof AudioContext | undefined =
        (window as any).AudioContext || (window as any).webkitAudioContext;
      if (!Ctor) return;

      try {
        audioContext = new Ctor();
        if (audioContext.state === 'suspended') {
          audioContext.resume().catch(() => {
            /* ignore */
          });
        }
        sourceNode = audioContext.createMediaStreamSource(stream);
        analyser = audioContext.createAnalyser();
        analyser.fftSize = 512;
        buffer = new Uint8Array(new ArrayBuffer(analyser.fftSize)) as Uint8Array<ArrayBuffer>;

        sourceNode.connect(analyser);

        const update = () => {
          if (!analyser || !buffer) return;
          analyser.getByteTimeDomainData(buffer);
          let sum = 0;
          for (let i = 0; i < buffer.length; i++) {
            const value = (buffer[i] - 128) / 128;
            sum += value * value;
          }
          const rms = Math.sqrt(sum / buffer.length);
          const speaking = rms > 0.04;
          setRemoteSpeaking(currentUserId, speaking);
          frameId = requestAnimationFrame(update);
        };
        update();
      } catch (error) {
        if (import.meta.env.DEV) {
          console.warn('Failed to start voice activity meter', error);
        }
      }
    };

    startMeter(data.stream);

    return {
      update(newData: { stream: MediaStream, userId: string }) {
        node.srcObject = newData.stream;
        if (currentUserId !== newData.userId) {
          setRemoteSpeaking(currentUserId, false);
          currentUserId = newData.userId;
        }
        startMeter(newData.stream);
        updateVolume();
      },
      destroy() {
        unsubVol();
        unsubMute();
        unsubUserVol();
        unsubOut();
        stopMeter();
      }
    };
  }

  onMount(() => {
    if (!get(session).user) {
      goto('/login');
      return;
    }
    roles.set({});
    expiryTicker = window.setInterval(() => {
      now = Date.now();
    }, 1000);
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
    chat.off('history', handleHistory);
    chat.off('message-deleted', handleMessageDeleted);
    chat.disconnect();
    if (currentVoiceChannelId !== null) {
      voice.leave(currentVoiceChannelId);
    }
    ping.stop();
    roles.set({});
    if (highlightTimer) {
      clearTimeout(highlightTimer);
      highlightTimer = null;
    }
    if (feedbackTimer) {
      clearTimeout(feedbackTimer);
      feedbackTimer = null;
    }
    if (expiryTicker !== null) {
      window.clearInterval(expiryTicker);
      expiryTicker = null;
    }
  });

  function sendText() {
    const trimmed = message.trim();
    if (trimmed === '') return;
    if (trimmed.startsWith('/')) {
      if (handleSlashCommand(trimmed)) {
        message = '';
        autoResize();
        return;
      }
    }
    chat.send($session.user ?? 'anon', message);
    message = '';
    autoResize();
  }

  async function sendFile() {
    const file = pendingFile;
    if (!file) {
      if (import.meta.env.DEV) console.log('sendFile: no file selected');
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
    if (import.meta.env.DEV) console.log('Uploading file to', base + '/upload', file);
    try {
      const res = await fetch(base + '/upload', { method: 'POST', body: form });
      if (import.meta.env.DEV) console.log('Upload response status:', res.status);
      if (res.status === 415) {
        setCommandFeedback('This file type is not allowed on the server.', 'error');
        return;
      }
      if (res.status === 413) {
        setCommandFeedback('File is too large to upload.', 'error');
        return;
      }
      if (!res.ok) {
        throw new Error(`upload failed with status ${res.status}`);
      }
      const data = await res.json();
      if (import.meta.env.DEV) console.log('Upload response data:', data);
      const url = data.url as string;
      const absolute = url.startsWith('http') ? url : base + url;
      const now = new Date();
      if (data.kind === 'image' || file.type.startsWith('image/')) {
        chat.sendRaw({
          type: 'chat',
          user: $session.user ?? 'anon',
          image: absolute,
          time: now.toLocaleTimeString(),
          timestamp: now.toISOString()
        });
      } else {
        chat.sendRaw({
          type: 'chat',
          user: $session.user ?? 'anon',
          attachment: {
            url: absolute,
            name: typeof data.name === 'string' ? data.name : file.name,
            size: typeof data.size === 'number' ? data.size : file.size
          },
          time: now.toLocaleTimeString(),
          timestamp: now.toISOString()
        });
      }
    } catch (e) {
      console.error('upload failed', e);
      setCommandFeedback('File upload failed.', 'error');
    } finally {
      clearPendingFile();
    }
  }

  async function send() {
    const hasMessage = message.trim() !== '';
    if (!pendingFile && !hasMessage) return;
    if (pendingFile) await sendFile();
    if (hasMessage) sendText();
  }

  function handleSlashCommand(raw: string): boolean {
    clearCommandFeedback();
    const content = raw.slice(1).trim();
    if (!content) {
      return true;
    }
    const [command] = content.split(/\s+/);
    const commandName = command.toLowerCase();
    const rest = content.slice(command.length).trim();
    const currentUser = get(session).user;

    switch (commandName) {
      case 'help': {
        openHelp();
        return true;
      }
      case 'me': {
        if (!rest) {
          setCommandFeedback('Usage: /me <action>', 'error');
          return true;
        }
        chat.send(currentUser ?? 'anon', `_${rest}_`);
        return true;
      }
      case 'shrug': {
        const shrug = '¯\_(ツ)_/¯';
        const text = rest ? `${rest} ${shrug}` : shrug;
        chat.send(currentUser ?? 'anon', text);
        return true;
      }
      case 'topic': {
        if (rest.length > MAX_TOPIC_LENGTH) {
          setCommandFeedback(`Topics are limited to ${MAX_TOPIC_LENGTH} characters.`, 'error');
          return true;
        }
        channelTopics.setTopic(currentChatChannelId, rest);
        setCommandFeedback(rest ? 'Updated the channel topic.' : 'Cleared the channel topic.');
        return true;
      }
      case 'status': {
        if (!rest) {
          setCommandFeedback('Usage: /status <online|away|busy|offline>', 'error');
          return true;
        }
        const normalized = rest.toLowerCase();
        const match = USER_STATUS_VALUES.find((value) => value === normalized);
        if (match) {
          statuses.setSelf(match);
          setCommandFeedback(`Status set to ${STATUS_LABELS[match]}.`);
        } else {
          setCommandFeedback(
            `Unknown status "${rest}". Options: ${USER_STATUS_VALUES.join(', ')}.`,
            'error'
          );
        }
        return true;
      }
      case 'focus': {
        const active = get(focusMode);
        focusMode.set(!active);
        setCommandFeedback(active ? 'Focus mode disabled.' : 'Focus mode enabled.');
        return true;
      }
      case 'ephemeral':
      case 'temp': {
        if (!rest) {
          setCommandFeedback('Usage: /ephemeral <seconds> <message>', 'error');
          return true;
        }
        const parts = rest.split(/\s+/);
        const durationPart = parts.shift();
        const contentText = parts.join(' ').trim();
        if (!durationPart || contentText === '') {
          setCommandFeedback('Usage: /ephemeral <seconds> <message>', 'error');
          return true;
        }
        const parsedDuration = Number(durationPart);
        if (!Number.isFinite(parsedDuration)) {
          setCommandFeedback('Ephemeral duration must be a number of seconds.', 'error');
          return true;
        }
        let durationSeconds = Math.round(parsedDuration);
        if (durationSeconds <= 0) {
          setCommandFeedback('Ephemeral duration must be positive.', 'error');
          return true;
        }
        const belowMinimum = durationSeconds < MIN_EPHEMERAL_SECONDS;
        const aboveMaximum = durationSeconds > MAX_EPHEMERAL_SECONDS;
        durationSeconds = Math.min(
          Math.max(durationSeconds, MIN_EPHEMERAL_SECONDS),
          MAX_EPHEMERAL_SECONDS
        );
        if (!currentUser) {
          setCommandFeedback('You must be signed in to send messages.', 'error');
          return true;
        }
        const expires = new Date(Date.now() + durationSeconds * 1000);
        chat.sendEphemeral(currentUser, contentText, expires.toISOString());
        let feedback = `Ephemeral message will expire in ${describeDuration(durationSeconds)}.`;
        if (belowMinimum) {
          feedback += ` Minimum duration is ${describeDuration(MIN_EPHEMERAL_SECONDS)}.`;
        } else if (aboveMaximum) {
          feedback += ` Maximum duration is ${describeDuration(MAX_EPHEMERAL_SECONDS)}.`;
        }
        setCommandFeedback(feedback.trim());
        return true;
      }
      case 'search': {
        openSearch(rest);
        if (rest) {
          tick().then(() => searchOverlay?.triggerSearch());
        }
        return true;
      }
      default: {
        setCommandFeedback(`Unknown command: /${commandName}`, 'error');
        return true;
      }
    }
  }

  function openSearch(initialQuery = '') {
    clearCommandFeedback();
    searchOpen = true;
    searchOverlay?.openWith(initialQuery);
  }

  function closeSearch() {
    searchOpen = false;
  }

  function handleSearchResult(msg: Message) {
    if (typeof msg.id !== 'number') return;
    focusMessage(msg.id);
  }

  function doSearch(query: string): Promise<Message[]> {
    return chat.search(currentChatChannelId, query, 50);
  }

  function joinChannel(id: number) {
    if (id === currentChatChannelId) return;
    currentChatChannelId = id;
    loadingHistory = false;
    chat.clear();
    chat.sendRaw({ type: 'join', channelId: id });
    scrollBottom();
  }


  function leaveVoice() {
    if (currentVoiceChannelId !== null) {
      voice.leave(currentVoiceChannelId);
    }
    inVoice = false;
    // Close any active screen share viewer
    viewingScreenShare = null;
    pendingScreenShareView = null;
    // Clean up screen share viewer session
    leaveScreenShareAsViewer();
  }

  async function handleViewScreenShare(userId: string) {
    try {
      if (!$session.user || currentVoiceChannelId === null) {
        alert('You must be in a voice channel to view screen shares');
        return;
      }

      if (userId === $session.user) return;
      
      pendingScreenShareView = userId;
      await viewScreenShare(userId, $session.user, currentVoiceChannelId);

      // Check if the peer stream is already available (rare but possible)
      const peer = $screenSharePeers.find(p => p.userId === userId);
      if (peer) {
        viewingScreenShare = peer;
        pendingScreenShareView = null;
      }
    } catch (error) {
      pendingScreenShareView = null;
      console.error('Failed to view screen share:', error);
      alert('Failed to view screen share');
    }
  }

  function closeScreenShareViewer() {
    viewingScreenShare = null;
  }

  function leaveServer() {
    chat.disconnect();
    if (currentVoiceChannelId !== null) {
      voice.leave(currentVoiceChannelId);
    }
    selectedServer.set(null);
    goto('/servers');
  }

  function createChannelPrompt(categoryId: number | null = null) {
    const name = prompt('New channel name');
    if (name) channels.create(name, categoryId);
  }

  function createVoiceChannelPrompt(categoryId: number | null = null) {
    const name = prompt('New voice channel name');
    if (!name) return;
    const preset = promptVoicePreset();
    voiceChannels.create(name, preset, categoryId);
  }

  function joinVoiceChannel(id: number) {
    if ($session.user) {
      if (inVoice && currentVoiceChannelId !== null) {
        voice.leave(currentVoiceChannelId);
      }
      currentVoiceChannelId = id;
      const info = $voiceChannels.find((vc) => vc.id === id);
      voice.join($session.user, id, info);
      inVoice = true;
      scrollBottom();
    }
  }

  let menuChannelId: number | null = null;
  let menuVoiceChannelId: number | null = null;
  let menuCategoryId: number | null = null;
  let volumeMenuOpen = false;
  let volumeMenuX = 0;
  let volumeMenuY = 0;
  let volumeMenuUser: string | null = null;

  function closeVolumeMenu() {
    volumeMenuOpen = false;
    volumeMenuUser = null;
  }

  let userRoleMenuOpen = false;
  let userRoleMenuX = 0;
  let userRoleMenuY = 0;
  let userRoleMenuTarget: string | null = null;

  const ASSIGNABLE_ROLES = ['Owner', 'Admin', 'Mod'] as const;

  $: currentUserIsOwner = (() => {
    const user = $session.user;
    if (!user) return false;
    const info = $roles[user];
    return info?.role?.toLowerCase() === 'owner';
  })();

  function openUserRoleMenu(event: MouseEvent, user: string) {
    if (!currentUserIsOwner) return;
    if (user === $session.user) return;
    event.preventDefault();
    event.stopPropagation();
    userRoleMenuX = event.clientX;
    userRoleMenuY = event.clientY;
    userRoleMenuTarget = user;
    userRoleMenuOpen = true;
  }

  function assignRole(user: string, role: string) {
    chat.sendRaw({ type: 'set-role', user, role });
  }

  function removeRole(user: string) {
    chat.sendRaw({ type: 'remove-role', user });
  }

  $: userRoleMenuItems = (() => {
    if (!userRoleMenuTarget) return [];
    const target = userRoleMenuTarget;
    const currentRole = $roles[target]?.role;
    const items: { label: string; action: () => void; danger?: boolean; icon?: string }[] = [];
    for (const role of ASSIGNABLE_ROLES) {
      if (currentRole?.toLowerCase() === role.toLowerCase()) continue;
      items.push({ label: `Set as ${role}`, action: () => assignRole(target, role) });
    }
    if (currentRole) {
      items.push({ label: 'Remove Role', action: () => removeRole(target), danger: true });
    }
    return items;
  })();

  function openChannelMenu(event: MouseEvent, channelId?: number, voice?: boolean) {
    event.preventDefault();
    event.stopPropagation();
    menuX = event.clientX;
    menuY = event.clientY;
    menuChannelId = null;
    menuVoiceChannelId = null;
    menuCategoryId = null;
    if (channelId != null) {
      if (voice) menuVoiceChannelId = channelId;
      else menuChannelId = channelId;
    }
    menuOpen = true;
  }

  function openCategoryMenu(event: MouseEvent, category: CategoryInfo) {
    event.preventDefault();
    event.stopPropagation();
    menuX = event.clientX;
    menuY = event.clientY;
    menuChannelId = null;
    menuVoiceChannelId = null;
    menuCategoryId = category.id;
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

  function editTopic() {
    const existing = $channelTopics[currentChatChannelId] ?? '';
    const input = prompt('Set channel topic', existing);
    if (input === null) return;
    if (input.length > MAX_TOPIC_LENGTH) {
      setCommandFeedback(`Topics are limited to ${MAX_TOPIC_LENGTH} characters.`, 'error');
      return;
    }
    channelTopics.setTopic(currentChatChannelId, input);
  }

  function canDeleteMessage(msg: Message): boolean {
    const current = $session.user;
    if (!current || typeof msg.id !== 'number') return false;
    if (msg.user === current) return true;
    return currentUserCanModerate;
  }

  function canEditMessage(msg: Message): boolean {
    const current = $session.user;
    if (!current || typeof msg.id !== 'number') return false;
    if (typeof msg.text !== 'string' || msg.text.trim() === '') return false;
    return msg.user === current;
  }

  function editChatMessage(msg: Message) {
    if (typeof msg.id !== 'number' || typeof msg.text !== 'string') return;
    const input = prompt('Edit message', msg.text);
    if (input === null) return;
    const trimmed = input.trim();
    if (trimmed === '' || input === msg.text) return;
    chat.edit(msg.id, input);
  }

  function canPinMessage(msg: Message): boolean {
    return typeof msg.id === 'number';
  }

  function isMessagePinned(msg: Message): boolean {
    if (typeof msg.id !== 'number') return false;
    return pinned.isPinned(currentChatChannelId, msg.id);
  }

  function togglePinMessage(msg: Message) {
    if (typeof msg.id !== 'number') return;
    if (isMessagePinned(msg)) {
      pinned.unpin(currentChatChannelId, msg.id);
    } else {
      pinned.pin(currentChatChannelId, msg);
    }
  }

  async function deleteChatMessage(msg: Message) {
    if (typeof msg.id !== 'number') return;
    const confirmation = await Promise.resolve(confirm('Delete this message?') as boolean | Promise<boolean>);
    if (!confirmation) return;
    chat.delete(msg.id);
  }

  function highlightMessageById(messageId: number): boolean {
    if (!messagesContainer) return false;
    const element = messagesContainer.querySelector<HTMLDivElement>(`[data-message-id="${messageId}"]`);
    if (!element) return false;
    element.scrollIntoView({ behavior: 'smooth', block: 'center' });
    highlightedMessageId = messageId;
    if (highlightTimer) {
      clearTimeout(highlightTimer);
    }
    highlightTimer = setTimeout(() => {
      if (highlightedMessageId === messageId) {
        highlightedMessageId = null;
      }
    }, 2000);
    return true;
  }

  function focusMessage(messageId: number) {
    if (!Number.isFinite(messageId)) return;
    if (highlightMessageById(messageId)) return;
    pendingScrollToMessage = messageId;
    chat.loadHistory(currentChatChannelId, messageId + 1, 200);
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
          const channels = $voiceChannels;
          if (channels.length) {
            joinVoiceChannel(currentVoiceChannelId ?? channels[0].id);
          }
        }
        break;
      default:
        return;
    }
  }

  function createCategoryPrompt() {
    const name = prompt('New category name');
    if (name) categories.create(name);
  }

  function renameCategoryPrompt(id: number) {
    const cat = $categories.find((c) => c.id === id);
    const name = prompt('Rename category', cat?.name ?? '');
    if (name) categories.rename(id, name);
  }

  function buildMoveToItems(channelId: number, voice: boolean): Array<{ label: string; action: () => void }> {
    const items: Array<{ label: string; action: () => void }> = [];
    const currentCh = voice
      ? $voiceChannels.find((c) => c.id === channelId)
      : $channels.find((c) => c.id === channelId);
    const currentCatId = currentCh?.categoryId ?? null;

    if (currentCatId !== null) {
      items.push({
        label: 'Move to: (no category)',
        action: () => channels.move(channelId, null, voice)
      });
    }

    for (const cat of $categories) {
      if (cat.id !== currentCatId) {
        items.push({
          label: `Move to: ${cat.name}`,
          action: () => channels.move(channelId, cat.id, voice)
        });
      }
    }

    return items;
  }

  $: channelMenuItems = [
    { label: 'Create Text Channel', action: () => createChannelPrompt() },
    { label: 'Create Voice Channel', action: () => createVoiceChannelPrompt() },
    { label: 'Create Category', action: createCategoryPrompt },
    ...(menuChannelId != null
      ? [
          ...buildMoveToItems(menuChannelId, false),
          { label: 'Delete Channel', action: () => channels.remove(menuChannelId!), danger: true }
        ]
      : []),
    ...(menuVoiceChannelId != null
      ? [
          ...VOICE_QUALITY_PRESETS.map((preset) => ({
            label:
              preset.bitrate && preset.bitrate > 0
                ? `Set Voice Quality: ${preset.label} (${Math.round(preset.bitrate / 1000)} kbps)`
                : `Set Voice Quality: ${preset.label}`,
            action: () =>
              voiceChannels.configure(menuVoiceChannelId!, {
                quality: preset.quality,
                bitrate: preset.bitrate
              })
          })),
          ...buildMoveToItems(menuVoiceChannelId, true),
          { label: 'Delete Voice Channel', action: () => voiceChannels.remove(menuVoiceChannelId!), danger: true }
        ]
      : []),
    ...(menuCategoryId != null
      ? [
          { label: 'Create Text Channel Here', action: () => createChannelPrompt(menuCategoryId) },
          { label: 'Create Voice Channel Here', action: () => createVoiceChannelPrompt(menuCategoryId) },
          { label: 'Rename Category', action: () => renameCategoryPrompt(menuCategoryId!) },
          { label: 'Delete Category', action: () => categories.remove(menuCategoryId!), danger: true }
        ]
      : [])
  ];

  let messagesContainer: HTMLDivElement;
  async function scrollBottom() {
    await tick();
    if (messagesContainer) {
      setScrollTop(messagesContainer.scrollHeight);
    }
  }
  let lastLength = 0;
  let loadingHistory = false;
  let prevHeight = 0;
  let programmaticScroll = false;

  function earliestId(): number | null {
    let min: number | null = null;
    for (const m of $chat) {
      if (m.channelId === currentChatChannelId && typeof m.id === 'number') {
        if (min === null || m.id! < min) min = m.id as number;
      }
    }
    return min;
  }

  function setScrollTop(value: number) {
    if (!messagesContainer) return;
    programmaticScroll = true;
    messagesContainer.scrollTop = value;
    requestAnimationFrame(() => { programmaticScroll = false; });
  }

  function onScroll() {
    if (!messagesContainer || loadingHistory || programmaticScroll) return;
    if (messagesContainer.scrollTop < 100) {
      const id = earliestId();
      if (id !== null && id > 1) {
        loadingHistory = true;
        prevHeight = messagesContainer.scrollHeight;
        chat.loadHistory(currentChatChannelId, id);
      }
    }
  }

  const handleHistory = async () => {
    await tick();
    if (messagesContainer) {
      setScrollTop(messagesContainer.scrollHeight - prevHeight);
    }
    loadingHistory = false;
    if (pendingScrollToMessage !== null) {
      const target = pendingScrollToMessage;
      if (highlightMessageById(target)) {
        pendingScrollToMessage = null;
      }
    }
  };
  chat.on('history', handleHistory);

  const handleMessageDeleted = (event: Message) => {
    const messageId = (event.id as number | undefined) ?? (event.messageId as number | undefined);
    const channelId = (event as any).channelId ?? currentChatChannelId;
    if (typeof messageId !== 'number') return;
    pinned.removeMessage(channelId, messageId);
    if (highlightedMessageId === messageId) {
      highlightedMessageId = null;
    }
    if (pendingScrollToMessage === messageId) {
      pendingScrollToMessage = null;
    }
  };
  chat.on('message-deleted', handleMessageDeleted);

  afterUpdate(() => {
    const handledPending =
      pendingScrollToMessage !== null && highlightMessageById(pendingScrollToMessage);
    if (handledPending) {
      pendingScrollToMessage = null;
    }
    if (messagesContainer) {
      const filteredLength = $chat.filter((m) => m.channelId === currentChatChannelId).length;
      if (filteredLength !== lastLength) {
        lastLength = filteredLength;
        if (!loadingHistory && !handledPending) {
          setScrollTop(messagesContainer.scrollHeight);
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
    <ChannelSidebar
      {currentChatChannelId}
      {currentVoiceChannelId}
      {inVoice}
      {serverStrength}
      onJoinChannel={joinChannel}
      onJoinVoiceChannel={joinVoiceChannel}
      onOpenChannelMenu={openChannelMenu}
      onOpenCategoryMenu={openCategoryMenu}
      onOpenUserVolumeMenu={openUserVolumeMenu}
      onViewScreenShare={handleViewScreenShare}
      onLeaveVoice={leaveVoice}
      onToggleMicrophone={toggleMicrophone}
      onToggleOutput={toggleOutput}
    />
    <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
    <div class="resizer" role="separator" aria-label="Resize channel list" on:mousedown={startLeftResize}></div>
    <!-- svelte-ignore a11y-no-static-element-interactions -->
    <div
      class="chat"
      on:dragenter={handleDragEnter}
      on:dragover={handleDragOver}
      on:dragleave={handleDragLeave}
      on:drop={handleDrop}
    >
      {#if dragActive}
        <div class="drop-overlay" aria-hidden="true">
          <span>Drop file to upload</span>
        </div>
      {/if}
      <ChatHeader
        channelId={currentChatChannelId}
        channelName={currentChatChannelName}
        topic={currentTopic}
        {serverStrength}
        {statusMap}
        onEditTopic={editTopic}
        onOpenSearch={() => openSearch()}
        onOpenSettings={openSettings}
        onLeaveServer={leaveServer}
        onLogout={logout}
      />
      <SettingsModal open={settingsOpen} close={closeSettings} />
      <HelpOverlay bind:this={helpOverlay} open={helpOpen} onClose={closeHelp} />
      <SearchOverlay
        bind:this={searchOverlay}
        open={searchOpen}
        onClose={closeSearch}
        onSearch={doSearch}
        onFocusResult={handleSearchResult}
        {now}
      />
      <PinnedBar
        channelId={currentChatChannelId}
        entries={pinnedEntries}
        messages={channelMessages}
        onFocusMessage={focusMessage}
      />
      <div class="messages-shell">
        <div class="messages" bind:this={messagesContainer} on:scroll={onScroll}>
          {#each messageBlocks as block (block.key)}
            {#if block.kind === 'separator'}
              <div class="day-separator" role="separator" aria-label={`Messages from ${block.label}`}>
                <span>{block.label}</span>
              </div>
            {:else if block.kind === 'message'}
              <div
                class="message"
                data-message-id={typeof block.message.id === 'number' ? block.message.id : undefined}
                class:highlighted={highlightedMessageId === block.message.id}
              >
                <span class="timestamp">{block.message.time}</span>
                <span class="username">{block.message.user}</span>
                {#if block.message.bot}
                  <span class="bot-badge">BOT</span>
                {/if}
                {#if block.message.user && $roles[block.message.user]}
                  <span
                    class="role"
                    style={$roles[block.message.user].color ? `color: ${$roles[block.message.user].color}` : ''}
                  >
                    {$roles[block.message.user].role}
                  </span>
                {/if}
                <div class="content-wrapper">
                  <span class="content">
                  {#if block.message.text}
                    {@html renderMarkdown(block.message.text)}
                  {/if}
                  {#if block.message.edited}
                    <span
                      class="edited-badge"
                      title={block.message.editedAt ? `Edited ${new Date(block.message.editedAt).toLocaleString()}` : 'Edited'}
                    >
                      (edited)
                    </span>
                  {/if}
                  {#if block.links.length > 0}
                    <div class="link-previews">
                      {#each block.links as link (link)}
                        <LinkPreview url={link} />
                      {/each}
                    </div>
                  {/if}
                  {#if block.message.image}
                    <img src={block.message.image as string} alt="" />
                  {/if}
                  {#if block.message.attachment}
                    <a
                      class="attachment-card"
                      href={block.message.attachment.url}
                      download={block.message.attachment.name}
                      target="_blank"
                      rel="noopener noreferrer"
                    >
                      <span class="attachment-icon" aria-hidden="true">📎</span>
                      <span class="attachment-details">
                        <span class="attachment-name">{block.message.attachment.name}</span>
                        {#if block.message.attachment.size > 0}
                          <span class="attachment-size">{formatFileSize(block.message.attachment.size)}</span>
                        {/if}
                      </span>
                    </a>
                  {/if}
                  {#if block.message.ephemeral}
                    {@const eInfo = ephemeralInfo(block.message, now)}
                    {#if eInfo}
                      <span
                        class="ephemeral-badge"
                        title={eInfo.absolute ?? undefined}
                      >
                        {eInfo.label}
                      </span>
                    {/if}
                  {/if}
                </span>
                  {#if typeof block.message.id === 'number' && (canPinMessage(block.message) || canDeleteMessage(block.message))}
                    <div class="message-actions">
                      {#if canEditMessage(block.message)}
                        <button
                          type="button"
                          class="message-action"
                          on:click={() => editChatMessage(block.message)}
                          title="Edit message"
                        >
                          ✏️
                        </button>
                      {/if}
                      {#if canPinMessage(block.message)}
                        <button
                          type="button"
                          class="message-action"
                          class:active={isMessagePinned(block.message)}
                          on:click={() => togglePinMessage(block.message)}
                          title={isMessagePinned(block.message) ? 'Unpin message' : 'Pin message'}
                        >
                          📌
                        </button>
                      {/if}
                      {#if canDeleteMessage(block.message)}
                        <button
                          type="button"
                          class="message-action danger"
                          on:click={() => deleteChatMessage(block.message)}
                          title="Delete message"
                        >
                          🗑️
                        </button>
                      {/if}
                    </div>
                  {/if}
                </div>
                {#if typeof block.message.id === 'number'}
                  <div class="reactions">
                    {#each reactionEntries(block.message) as reaction (reaction.emoji)}
                      <button
                        class="reaction-chip"
                        class:active={reaction.users.includes($session.user ?? '')}
                        on:click={() =>
                          toggleReaction(block.message.id as number, reaction.emoji, reaction.users)}
                        title={reaction.users.join(', ')}
                      >
                        <span class="emoji">{reaction.emoji}</span>
                        <span class="count">{reaction.users.length}</span>
                      </button>
                    {/each}
                    <button class="reaction-chip add" on:click={() => addReactionPrompt(block.message.id as number)}>
                      +
                    </button>
                  </div>
                {/if}
              </div>
            {/if}
          {/each}
        </div>
      </div>
      <div class="input-row">
        {#if commandFeedback}
          <div class={`command-feedback ${commandFeedbackType}`}>{commandFeedback}</div>
        {/if}
        <textarea
          class:scrollable={inputScrollable}
          bind:value={message}
          bind:this={messageInput}
          rows="1"
          placeholder="Message"
          on:input={autoResize}
          on:paste={handlePaste}
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
          on:change={handleFileChange}
        />
        <div class="controls">
          {#if pendingFile}
            <div class="preview-container">
              {#if previewUrl}
                <img src={previewUrl} alt="preview" class="preview" />
              {:else}
                <span class="file-chip" title={pendingFile.name}>
                  <span aria-hidden="true">📎</span>
                  <span class="file-chip-name">{pendingFile.name}</span>
                  {#if pendingFile.size > 0}
                    <span class="file-chip-size">{formatFileSize(pendingFile.size)}</span>
                  {/if}
                </span>
              {/if}
              <button class="preview-remove" on:click={clearPendingFile} aria-label="Remove file">
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
              title="Upload file"
              aria-label="Upload file"
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

      {#each $voice as peer (peer.id)}
        <audio autoplay use:stream={{ stream: peer.stream, userId: peer.id }}></audio>
      {/each}
    </div>
    <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
    <div class="resizer" role="separator" aria-label="Resize user list" on:mousedown={startRightResize}></div>
    <UserList
      {statusMap}
      {currentUserIsOwner}
      onUserContextMenu={openUserRoleMenu}
    />
</div>

<ContextMenu bind:open={menuOpen} x={menuX} y={menuY} items={channelMenuItems} />
<ContextMenu bind:open={userRoleMenuOpen} x={userRoleMenuX} y={userRoleMenuY} items={userRoleMenuItems} />

<VolumeMenu
  open={volumeMenuOpen}
  x={volumeMenuX}
  y={volumeMenuY}
  user={volumeMenuUser}
  onClose={closeVolumeMenu}
/>

{#if viewingScreenShare}
  <ScreenShareViewer peer={viewingScreenShare} onClose={closeScreenShareViewer} />
{/if}

<style>
  .page {
    display: flex;
    height: 100vh;
    padding: clamp(1.25rem, 2.5vw, 1.75rem);
    gap: clamp(0.75rem, 2vw, 1rem);
  }

  .page.focus {
    padding-inline: clamp(1.5rem, 4vw, 3rem);
  }

  /* .channels/.sidebar are the root elements of ChannelSidebar/UserList;
     :global escapes Svelte's per-component style scoping. */
  .page.focus :global(.channels),
  .page.focus :global(.sidebar),
  .page.focus .resizer {
    display: none;
  }

  .page.focus .chat {
    max-width: 1080px;
    margin: 0 auto;
  }

  .resizer {
    width: 6px;
    cursor: col-resize;
    position: relative;
    flex-shrink: 0;
  }

  .resizer::after {
    content: '';
    position: absolute;
    inset: calc(50% - 18px) auto;
    left: 50%;
    width: 2px;
    height: 36px;
    border-radius: var(--radius-pill);
    background: color-mix(in srgb, var(--color-on-surface) 12%, transparent);
    transform: translateX(-50%);
  }

  .chat {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
    min-width: 0;
    position: relative;
  }

  /* pointer-events: none keeps drag events flowing to .chat underneath. */
  .drop-overlay {
    position: absolute;
    inset: 0;
    z-index: 30;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-lg);
    border: 2px dashed color-mix(in srgb, var(--color-primary) 55%, transparent);
    background: color-mix(in srgb, var(--color-surface-elevated) 72%, transparent);
    pointer-events: none;
  }

  .drop-overlay span {
    padding: 0.6rem 1.2rem;
    border-radius: var(--radius-pill);
    background: color-mix(in srgb, var(--color-primary) 18%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-primary) 40%, transparent);
    color: var(--color-on-surface);
    font-weight: 700;
    font-size: var(--text-md);
    letter-spacing: 0.02em;
  }

  .messages-shell {
    flex: 1;
    min-height: 0;
    border-radius: var(--radius-lg);
    background: color-mix(in srgb, var(--color-surface-raised) 90%, transparent);
    border: 1px solid var(--color-surface-outline);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.04);
    overflow: hidden;
    display: flex;
  }

  .messages {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.8rem;
    padding: clamp(0.9rem, 2vw, 1.25rem);
  }

  .day-separator {
    display: flex;
    align-items: center;
    gap: 0.75rem;
    color: var(--color-muted);
    font-size: var(--text-sm);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .day-separator::before,
  .day-separator::after {
    content: '';
    flex: 1;
    border-top: 1px solid color-mix(in srgb, var(--color-surface-outline) 70%, transparent);
    opacity: 0.7;
  }

  .day-separator span {
    padding: 0.2rem 0.75rem;
    border-radius: var(--radius-pill);
    background: color-mix(in srgb, var(--color-surface-elevated) 78%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-primary) 18%, transparent);
    color: var(--color-muted);
    font-weight: 600;
  }

  .message {
    display: grid;
    grid-template-columns: auto auto 1fr;
    column-gap: 0.55rem;
    row-gap: 0.3rem;
    align-items: baseline;
    padding: 0.65rem 0.9rem;
    border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--color-primary) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-primary) 12%, transparent);
    transition: transform var(--transition), box-shadow var(--transition);
    /* Isolate layout/style recalculation per message so long histories stay cheap. */
    contain: layout style;
  }

  .message:hover {
    transform: translateY(-1px);
    box-shadow: var(--shadow-xs);
  }

  .message.highlighted {
    border-color: color-mix(in srgb, var(--color-secondary) 45%, transparent);
    box-shadow: 0 0 0 2px color-mix(in srgb, var(--color-secondary) 28%, transparent);
  }

  .message .timestamp {
    font-size: var(--text-xs);
    color: var(--color-muted);
    font-family: 'JetBrains Mono', 'Fira Code', monospace;
    opacity: 0.7;
  }

  .message .username {
    font-weight: 600;
    color: var(--color-on-surface);
  }

  .message .role {
    font-size: var(--text-sm);
    font-weight: 600;
    align-self: center;
  }

  .bot-badge {
    display: inline-flex;
    align-items: center;
    justify-self: start;
    padding: 0.1rem 0.4rem;
    border-radius: var(--radius-xs, 4px);
    font-size: var(--text-xs);
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    background: color-mix(in srgb, var(--color-primary) 22%, transparent);
    color: var(--color-primary);
    border: 1px solid color-mix(in srgb, var(--color-primary) 35%, transparent);
    align-self: center;
    line-height: 1.3;
  }

  .content-wrapper {
    grid-column: 1 / -1;
    display: flex;
    gap: 0.75rem;
    align-items: flex-start;
  }

  .message .content {
    flex: 1;
    color: var(--color-on-surface);
    line-height: 1.65;
  }

  .edited-badge {
    margin-left: 0.35rem;
    font-size: var(--text-xs);
    color: var(--color-muted);
    font-style: italic;
  }

  .ephemeral-badge {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    margin-top: 0.5rem;
    padding: 0.2rem 0.6rem;
    border-radius: var(--radius-pill);
    font-size: var(--text-xs);
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    background: color-mix(in srgb, var(--color-warning) 15%, transparent);
    color: color-mix(in srgb, var(--color-warning) 80%, var(--color-on-surface) 20%);
    width: fit-content;
  }

  .message-actions {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
  }

  .message-action {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0.25rem 0.45rem;
    border-radius: var(--radius-sm);
    border: 1px solid color-mix(in srgb, var(--color-primary) 18%, transparent);
    background: color-mix(in srgb, var(--color-surface-elevated) 82%, transparent);
    color: var(--color-on-surface);
    cursor: pointer;
    font-size: var(--text-sm);
    transition: background var(--transition), border-color var(--transition), transform var(--transition);
  }

  .message-action:hover,
  .message-action.active {
    background: color-mix(in srgb, var(--color-primary) 18%, transparent);
    border-color: color-mix(in srgb, var(--color-primary) 36%, transparent);
    transform: translateY(-1px);
  }

  .message-action.danger {
    color: color-mix(in srgb, var(--color-error) 80%, var(--color-on-surface));
  }

  .message-action.danger:hover {
    background: color-mix(in srgb, var(--color-error) 18%, transparent);
    border-color: color-mix(in srgb, var(--color-error) 32%, transparent);
  }

  .link-previews {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    margin-top: 0.65rem;
  }

  .message .content :global(code) {
    background: color-mix(in srgb, var(--color-primary) 25%, transparent);
    padding: 0.15rem 0.35rem;
    border-radius: 6px;
    font-size: 0.9em;
  }

  .message .content :global(pre) {
    background: color-mix(in srgb, var(--color-surface-elevated) 88%, transparent);
    border-radius: var(--radius-sm);
    padding: 0.9rem;
    overflow-x: auto;
    border: 1px solid color-mix(in srgb, var(--color-primary) 18%, transparent);
  }

  .message .content :global(pre code) {
    display: block;
    padding: 0;
    margin: 0;
    background: transparent;
    font-family: 'JetBrains Mono', 'Fira Code', monospace;
    font-size: 0.9em;
  }

  .message .content :global(.hljs) {
    color: var(--color-on-surface);
  }

  .message .content :global(.hljs-comment),
  .message .content :global(.hljs-quote) {
    color: color-mix(in srgb, var(--color-muted) 92%, transparent);
    font-style: italic;
  }

  .message .content :global(.hljs-keyword),
  .message .content :global(.hljs-selector-tag),
  .message .content :global(.hljs-subst) {
    color: color-mix(in srgb, var(--color-secondary) 80%, var(--color-on-surface) 20%);
  }

  .message .content :global(.hljs-string),
  .message .content :global(.hljs-doctag),
  .message .content :global(.hljs-regexp) {
    color: color-mix(in srgb, var(--color-tertiary) 80%, var(--color-on-surface) 20%);
  }

  .message .content :global(.hljs-title),
  .message .content :global(.hljs-section),
  .message .content :global(.hljs-function),
  .message .content :global(.hljs-name) {
    color: color-mix(in srgb, var(--color-primary) 78%, var(--color-on-surface) 22%);
  }

  .message .content :global(.hljs-number),
  .message .content :global(.hljs-literal),
  .message .content :global(.hljs-symbol),
  .message .content :global(.hljs-bullet) {
    color: color-mix(in srgb, var(--color-warning) 75%, var(--color-on-surface) 25%);
  }

  .message .content :global(.hljs-attr),
  .message .content :global(.hljs-attribute),
  .message .content :global(.hljs-variable),
  .message .content :global(.hljs-template-variable) {
    color: color-mix(in srgb, var(--color-success) 75%, var(--color-on-surface) 25%);
  }

  .message .content :global(.hljs-meta),
  .message .content :global(.hljs-meta .hljs-string) {
    color: color-mix(in srgb, var(--color-primary) 65%, var(--color-on-surface) 35%);
  }

  .message img {
    max-width: min(420px, 100%);
    border-radius: var(--radius-md);
    margin-top: 0.65rem;
    border: 1px solid color-mix(in srgb, var(--color-primary) 14%, transparent);
    box-shadow: var(--shadow-xs);
  }

  .attachment-card {
    display: inline-flex;
    align-items: center;
    gap: 0.6rem;
    margin-top: 0.65rem;
    padding: 0.55rem 0.85rem;
    border-radius: var(--radius-md);
    border: 1px solid color-mix(in srgb, var(--color-primary) 18%, transparent);
    background: color-mix(in srgb, var(--color-surface-elevated) 82%, transparent);
    color: var(--color-on-surface);
    text-decoration: none;
    max-width: min(420px, 100%);
    transition: background var(--transition), border-color var(--transition), transform var(--transition);
  }

  .attachment-card:hover {
    background: color-mix(in srgb, var(--color-primary) 14%, transparent);
    border-color: color-mix(in srgb, var(--color-primary) 32%, transparent);
    transform: translateY(-1px);
  }

  .attachment-icon {
    font-size: 1.2rem;
    flex-shrink: 0;
  }

  .attachment-details {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .attachment-name {
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .attachment-size {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .file-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    max-width: 220px;
    padding: 0.35rem 0.6rem;
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, var(--color-surface-raised) 84%, transparent);
    font-size: var(--text-sm);
  }

  .file-chip-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .file-chip-size {
    color: var(--color-muted);
    font-size: var(--text-xs);
    flex-shrink: 0;
  }

  .reactions {
    grid-column: 1 / -1;
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    margin-top: 0.15rem;
  }

  .reaction-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    border-radius: var(--radius-pill);
    padding: 0.3rem 0.65rem;
    font-size: var(--text-sm);
    border: 1px solid color-mix(in srgb, var(--color-primary) 18%, transparent);
    background: color-mix(in srgb, var(--color-primary) 12%, transparent);
    color: var(--color-on-surface);
  }

  .reaction-chip.active {
    background: color-mix(in srgb, var(--color-primary) 32%, transparent);
    border-color: color-mix(in srgb, var(--color-primary) 40%, transparent);
  }

  .reaction-chip.add {
    font-weight: 700;
  }

  .input-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 1rem;
    align-items: end;
    padding: clamp(1rem, 2vw, 1.35rem);
    border-radius: var(--radius-lg);
    background: color-mix(in srgb, var(--color-surface-elevated) 88%, transparent);
    border: 1px solid var(--color-surface-outline);
    box-shadow: var(--shadow-sm);
  }

  .command-feedback {
    grid-column: 1 / -1;
    padding: 0.45rem 0.75rem;
    border-radius: var(--radius-md);
    font-size: var(--text-md);
    font-weight: 600;
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    border: 1px solid color-mix(in srgb, var(--color-success) 25%, transparent);
    background: color-mix(in srgb, var(--color-success) 12%, transparent);
    color: color-mix(in srgb, var(--color-success) 80%, var(--color-on-surface) 20%);
  }

  .command-feedback.error {
    border-color: color-mix(in srgb, var(--color-error) 32%, transparent);
    background: color-mix(in srgb, var(--color-error) 12%, transparent);
    color: color-mix(in srgb, var(--color-error) 85%, var(--color-on-surface) 15%);
  }

  textarea {
    width: 100%;
    min-height: 3rem;
    max-height: 360px;
    resize: none;
    overflow-y: hidden;
    overflow-x: hidden;
    border-radius: var(--radius-md);
    border: 1px solid color-mix(in srgb, var(--color-primary) 14%, transparent);
    background: color-mix(in srgb, var(--color-surface-raised) 84%, transparent);
    color: var(--color-on-surface);
    padding: 0.85rem 1rem;
    line-height: 1.5;
  }

  textarea.scrollable {
    overflow-y: auto;
  }

  .controls {
    display: flex;
    align-items: flex-end;
    gap: 0.9rem;
  }

  .input-controls {
    display: flex;
    align-items: center;
    gap: 0.65rem;
  }

  .preview-container {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.4rem;
    background: color-mix(in srgb, var(--color-secondary) 12%, transparent);
    padding: 0.55rem;
    border-radius: var(--radius-md);
    border: 1px dashed color-mix(in srgb, var(--color-secondary) 32%, transparent);
  }

  .preview-container img {
    max-width: 120px;
    max-height: 120px;
    border-radius: var(--radius-sm);
  }

  .preview-remove {
    background: transparent;
    color: var(--color-secondary);
    border: none;
    font-size: var(--text-sm);
  }

  .file-input {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
    border: 0;
  }

  .file-button,
  .send {
    width: 2.75rem;
    height: 2.75rem;
    border-radius: 0.9rem;
    border: 1px solid color-mix(in srgb, var(--color-primary) 16%, transparent);
    background: color-mix(in srgb, var(--color-primary) 10%, transparent);
    color: var(--color-secondary);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }

  .file-button svg,
  .send svg {
    width: 1.2rem;
    height: 1.2rem;
  }

  .file-button:hover,
  .send:hover {
    transform: translateY(-1px);
    box-shadow: var(--shadow-xs);
  }

  .send {
    background: linear-gradient(135deg, var(--color-primary), var(--color-secondary));
    color: var(--color-on-primary);
    border-color: transparent;
  }

  @media (max-width: 1280px) {
    .page {
      flex-direction: column;
      height: auto;
      min-height: 100vh;
    }

    .page :global(.channels),
    .page :global(.sidebar) {
      width: 100%;
      order: 0;
    }

    .resizer {
      display: none;
    }

    .chat {
      order: 1;
    }

    .page :global(.sidebar) {
      order: 2;
    }
  }

  @media (max-width: 768px) {
    .page {
      padding: clamp(1rem, 4vw, 1.5rem);
    }

    .input-row {
      grid-template-columns: 1fr;
    }

    .controls {
      justify-content: flex-end;
    }
  }
</style>

