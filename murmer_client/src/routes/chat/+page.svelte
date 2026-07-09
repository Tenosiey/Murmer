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
  import { typing } from '$lib/stores/typing';
  import { unread } from '$lib/stores/unread';
  import { threadData } from '$lib/stores/thread';
  import { dm } from '$lib/stores/dm';
  import { screenSharePeers, viewScreenShare, leaveScreenShareAsViewer } from '$lib/stores/screenShare';
  import ScreenShareViewer from '$lib/components/ScreenShareViewer.svelte';
  import { loadKeyPair, sign } from '$lib/keypair';
  import { connection, connectionError } from '$lib/stores/connection';
  import { describeServerError, isFatalConnectionError } from '$lib/errors';
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
    searchResultPreview,
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

  let replyingTo: Message | null = null;
  let threadRootId: number | null = null;
  let threadReplyText = '';
  let dmText = '';
  const dmConversations = dm.conversations;
  const dmActivePeer = dm.activePeer;
  let threadMessages: Message[] = [];
  let threadReplyCounts = new Map<number, number>();
  /* Last-read message id captured when entering the channel; the "New"
     divider stays anchored there until the user switches channels. */
  let unreadMarkerAfterId = 0;
  let typingLabel: string | null = null;

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
  $: messageBlocks = buildMessageBlocks(channelMessages, {
    unreadAfterId: unreadMarkerAfterId,
    currentUser: $session.user
  });
  $: pinnedEntries = $pinned[currentChatChannelId] ?? [];

  $: if ($channels.length && !$channels.some((c) => c.id === currentChatChannelId)) {
    currentChatChannelId = $channels[0].id;
    unreadMarkerAfterId = unread.getLastRead(currentChatChannelId);
    unread.setActive(currentChatChannelId);
    loadingHistory = false;
    if (initialChannelSet) {
      chat.sendRaw({ type: 'join', channelId: currentChatChannelId });
    } else {
      initialChannelSet = true;
    }
  }

  function latestMessageId(messages: Message[]): number | null {
    let max: number | null = null;
    for (const m of messages) {
      if (typeof m.id === 'number' && (max === null || m.id > max)) max = m.id;
    }
    return max;
  }

  // Everything rendered in the active channel counts as read.
  $: {
    const latest = latestMessageId(channelMessages);
    if (latest !== null) unread.markRead(currentChatChannelId, latest);
  }

  $: typingLabel = (() => {
    const users = Object.entries($typing[currentChatChannelId] ?? {})
      .filter(([user, expiry]) => user !== $session.user && expiry > now)
      .map(([user]) => user);
    if (users.length === 0) return null;
    if (users.length === 1) return `${users[0]} is typing…`;
    if (users.length === 2) return `${users[0]} and ${users[1]} are typing…`;
    return 'Several people are typing…';
  })();

  $: threadReplyCounts = (() => {
    const map = new Map<number, number>();
    for (const m of channelMessages) {
      if (typeof m.threadId === 'number') {
        map.set(m.threadId, (map.get(m.threadId) ?? 0) + 1);
      }
    }
    return map;
  })();

  /* The panel merges the server's thread snapshot with live messages from the
     store, so replies arriving while the thread is open show up immediately. */
  $: threadMessages = (() => {
    if (threadRootId === null) return [];
    const byId = new Map<number, Message>();
    const data = $threadData;
    if (data && data.rootId === threadRootId) {
      for (const m of data.messages) {
        if (typeof m.id === 'number') byId.set(m.id, m);
      }
    }
    for (const m of channelMessages) {
      if (typeof m.id !== 'number') continue;
      if (m.id === threadRootId || m.threadId === threadRootId) byId.set(m.id, m);
    }
    return [...byId.values()].sort((a, b) => (a.id as number) - (b.id as number));
  })();

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

  function connectToServer() {
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
      if (currentChatChannelId > 0) {
        // Reconnecting: the server placed us back in the default channel,
        // so rejoin the channel the user was viewing.
        chat.sendRaw({ type: 'join', channelId: currentChatChannelId });
      }
      ping.start();
      await scrollBottom();
    });
  }

  function retryConnect() {
    ping.stop();
    connectToServer();
  }

  function leaveToServers(message: string | null = null) {
    connectionError.set(message);
    ping.stop();
    chat.disconnect();
    goto('/servers');
  }

  const handleServerError = (msg: Message) => {
    const code = typeof msg.message === 'string' ? msg.message : '';
    const description = describeServerError(code);
    if (isFatalConnectionError(code)) {
      // The server closes the connection after these errors; return to the
      // server list and explain why there.
      leaveToServers(description);
      return;
    }
    setCommandFeedback(description, 'error');
  };
  chat.on('error', handleServerError);

  const handleForceDisconnect = (msg: Message) => {
    if (!msg.user || msg.user !== get(session).user) return;
    const action = msg.action === 'banned' ? 'banned from' : 'kicked from';
    const by = typeof msg.by === 'string' && msg.by ? ` by ${msg.by}` : '';
    leaveToServers(`You were ${action} this server${by}.`);
  };
  chat.on('force-disconnect', handleForceDisconnect);

  const handleUserMuted = (msg: Message) => {
    if (typeof msg.user !== 'string') return;
    if (msg.user === get(session).user) {
      const until = typeof msg.until === 'string' ? ` until ${new Date(msg.until).toLocaleString()}` : '';
      setCommandFeedback(`You have been muted${until}.`, 'error');
      return;
    }
    setCommandFeedback(`${msg.user} has been muted.`);
  };
  chat.on('user-muted', handleUserMuted);

  const handleUserUnmuted = (msg: Message) => {
    if (typeof msg.user !== 'string') return;
    if (msg.user === get(session).user) {
      setCommandFeedback('You are no longer muted.');
      return;
    }
    setCommandFeedback(`${msg.user} has been unmuted.`);
  };
  chat.on('user-unmuted', handleUserUnmuted);

  const handleUserUnbanned = (msg: Message) => {
    if (typeof msg.user !== 'string') return;
    setCommandFeedback(`${msg.user} has been unbanned.`);
  };
  chat.on('user-unbanned', handleUserUnbanned);

  onMount(() => {
    if (!get(session).user) {
      goto('/login');
      return;
    }
    roles.set({});
    expiryTicker = window.setInterval(() => {
      now = Date.now();
    }, 1000);
    connectToServer();
  });

  onDestroy(() => {
    chat.off('history', handleHistory);
    chat.off('message-deleted', handleMessageDeleted);
    chat.off('error', handleServerError);
    chat.off('force-disconnect', handleForceDisconnect);
    chat.off('user-muted', handleUserMuted);
    chat.off('user-unmuted', handleUserUnmuted);
    chat.off('user-unbanned', handleUserUnbanned);
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
    const replyTarget = typeof replyingTo?.id === 'number' ? replyingTo.id : undefined;
    chat.send($session.user ?? 'anon', message, replyTarget);
    replyingTo = null;
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
        // Backslash-escaped so markdown doesn't italicize the face.
        const shrug = '¯\\\\\\_(ツ)\\_/¯';
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
    unreadMarkerAfterId = unread.getLastRead(id);
    unread.setActive(id);
    replyingTo = null;
    closeThread();
    loadingHistory = false;
    chat.clear();
    chat.sendRaw({ type: 'join', channelId: id });
    scrollBottom();
  }

  function startReply(msg: Message) {
    if (typeof msg.id !== 'number') return;
    replyingTo = msg;
    messageInput?.focus();
  }

  function cancelReply() {
    replyingTo = null;
  }

  function openThread(rootId: number) {
    threadRootId = rootId;
    threadReplyText = '';
    chat.loadThread(rootId);
  }

  function closeThread() {
    threadRootId = null;
    threadReplyText = '';
  }

  function sendThreadReply() {
    const trimmed = threadReplyText.trim();
    if (trimmed === '' || threadRootId === null) return;
    chat.send($session.user ?? 'anon', trimmed, threadRootId);
    threadReplyText = '';
  }

  function openDm(user: string) {
    if (user === $session.user) return;
    closeThread();
    dmText = '';
    dm.open(user);
    chat.loadDmHistory(user);
  }

  function closeDm() {
    dm.close();
    dmText = '';
  }

  function sendDmMessage() {
    const peer = $dmActivePeer;
    const trimmed = dmText.trim();
    if (!peer || trimmed === '') return;
    chat.sendDm(peer, trimmed);
    dmText = '';
  }

  $: dmMessages = $dmActivePeer ? ($dmConversations[$dmActivePeer] ?? []) : [];

  function handleComposerInput() {
    autoResize();
    if (message.trim().length > 0) {
      chat.sendTyping();
    }
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

  async function joinVoiceChannel(id: number) {
    if (!$session.user) return;
    if (inVoice && currentVoiceChannelId !== null) {
      voice.leave(currentVoiceChannelId);
    }
    const info = $voiceChannels.find((vc) => vc.id === id);
    try {
      await voice.join($session.user, id, info);
    } catch (error) {
      console.error('Failed to join voice channel', error);
      inVoice = false;
      setCommandFeedback('Could not access your microphone. Check the permission and input device.', 'error');
      return;
    }
    currentVoiceChannelId = id;
    inVoice = true;
    scrollBottom();
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

  /** Mirror of the server's moderation ranking: requesters must strictly
   *  outrank their target for kick/ban/mute to be allowed. */
  function moderationRank(role: string | undefined): number {
    switch (role?.toLowerCase()) {
      case 'owner':
        return 3;
      case 'admin':
        return 2;
      case 'mod':
        return 1;
      default:
        return 0;
    }
  }

  $: currentUserIsOwner = (() => {
    const user = $session.user;
    if (!user) return false;
    const info = $roles[user];
    return info?.role?.toLowerCase() === 'owner';
  })();

  $: currentUserModerationRank = moderationRank(
    $session.user ? $roles[$session.user]?.role : undefined
  );

  function canModerate(target: string): boolean {
    if (target === $session.user) return false;
    return currentUserModerationRank > moderationRank($roles[target]?.role);
  }

  function openUserRoleMenu(event: MouseEvent, user: string) {
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

  function kickUser(user: string) {
    chat.sendRaw({ type: 'kick-user', user });
  }

  function banUser(user: string) {
    if (!confirm(`Ban ${user} from this server?`)) return;
    chat.sendRaw({ type: 'ban-user', user });
  }

  function unbanUser(user: string) {
    chat.sendRaw({ type: 'unban-user', user });
  }

  function muteUser(user: string, durationSeconds?: number) {
    const payload: Record<string, unknown> = { type: 'mute-user', user };
    if (typeof durationSeconds === 'number') payload.durationSeconds = durationSeconds;
    chat.sendRaw(payload);
  }

  function unmuteUser(user: string) {
    chat.sendRaw({ type: 'unmute-user', user });
  }

  $: userRoleMenuItems = (() => {
    if (!userRoleMenuTarget) return [];
    const target = userRoleMenuTarget;
    const currentRole = $roles[target]?.role;
    const items: { label: string; action: () => void; danger?: boolean; icon?: string }[] = [];
    items.push({ label: 'Send Message', action: () => openDm(target) });
    if (currentUserIsOwner) {
      for (const role of ASSIGNABLE_ROLES) {
        if (currentRole?.toLowerCase() === role.toLowerCase()) continue;
        items.push({ label: `Set as ${role}`, action: () => assignRole(target, role) });
      }
      if (currentRole) {
        items.push({ label: 'Remove Role', action: () => removeRole(target), danger: true });
      }
    }
    if (canModerate(target)) {
      items.push({ label: 'Mute (10 min)', action: () => muteUser(target, 600) });
      items.push({ label: 'Mute (1 hour)', action: () => muteUser(target, 3600) });
      items.push({ label: 'Mute (until lifted)', action: () => muteUser(target) });
      items.push({ label: 'Unmute', action: () => unmuteUser(target) });
      if ($onlineUsers.includes(target)) {
        items.push({ label: 'Kick User', danger: true, action: () => kickUser(target) });
      }
      items.push({ label: 'Ban User', danger: true, action: () => banUser(target) });
      items.push({ label: 'Unban User', action: () => unbanUser(target) });
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
    // Pins live on the server; the resulting `pins` broadcast updates the store.
    if (isMessagePinned(msg)) {
      chat.sendRaw({ type: 'unpin-message', messageId: msg.id });
    } else {
      chat.sendRaw({ type: 'pin-message', messageId: msg.id });
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
            {:else if block.kind === 'unread'}
              <div class="unread-divider" role="separator" aria-label="New messages">
                <span>New</span>
              </div>
            {:else if block.kind === 'message'}
              <div
                class="message"
                data-message-id={typeof block.message.id === 'number' ? block.message.id : undefined}
                class:highlighted={highlightedMessageId === block.message.id}
              >
                {#if block.message.replyTo}
                  {@const reply = block.message.replyTo}
                  <button
                    type="button"
                    class="reply-quote"
                    on:click={() => focusMessage(reply.id)}
                    title={`Jump to ${reply.user}'s message`}
                  >
                    <span class="reply-quote-arrow" aria-hidden="true">↪</span>
                    <span class="reply-quote-user">{reply.user}</span>
                    <span class="reply-quote-text">{reply.text || 'Original message'}</span>
                  </button>
                {/if}
                <span class="username">{block.message.user}</span>
                <span class="timestamp">{block.message.time}</span>
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
                      <span class="attachment-icon" aria-hidden="true">
                        <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48"/></svg>
                      </span>
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
                      <button
                        type="button"
                        class="message-action"
                        on:click={() => addReactionPrompt(block.message.id as number)}
                        title="Add reaction"
                      >
                        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="10"/><path d="M8 14s1.5 2 4 2 4-2 4-2"/><line x1="9" y1="9" x2="9.01" y2="9"/><line x1="15" y1="9" x2="15.01" y2="9"/></svg>
                        <span class="sr-only">Add reaction</span>
                      </button>
                      <button
                        type="button"
                        class="message-action"
                        on:click={() => startReply(block.message)}
                        title="Reply"
                      >
                        <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polyline points="9 17 4 12 9 7"/><path d="M20 18v-2a4 4 0 0 0-4-4H4"/></svg>
                        <span class="sr-only">Reply</span>
                      </button>
                      {#if canEditMessage(block.message)}
                        <button
                          type="button"
                          class="message-action"
                          on:click={() => editChatMessage(block.message)}
                          title="Edit message"
                        >
                          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z"/></svg>
                          <span class="sr-only">Edit message</span>
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
                          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 17v5"/><path d="M9 10.76a2 2 0 0 1-1.11 1.79l-1.78.9A2 2 0 0 0 5 15.24V16a1 1 0 0 0 1 1h12a1 1 0 0 0 1-1v-.76a2 2 0 0 0-1.11-1.79l-1.78-.9A2 2 0 0 1 15 10.76V7a1 1 0 0 1 1-1 2 2 0 0 0 0-4H8a2 2 0 0 0 0 4 1 1 0 0 1 1 1z"/></svg>
                          <span class="sr-only">{isMessagePinned(block.message) ? 'Unpin message' : 'Pin message'}</span>
                        </button>
                      {/if}
                      {#if canDeleteMessage(block.message)}
                        <button
                          type="button"
                          class="message-action danger"
                          on:click={() => deleteChatMessage(block.message)}
                          title="Delete message"
                        >
                          <svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M10 11v6"/><path d="M14 11v6"/><path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6"/><path d="M3 6h18"/><path d="M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/></svg>
                          <span class="sr-only">Delete message</span>
                        </button>
                      {/if}
                    </div>
                  {/if}
                </div>
                {#if typeof block.message.id === 'number'}
                  {@const reactions = reactionEntries(block.message)}
                  {#if reactions.length > 0}
                    <div class="reactions">
                      {#each reactions as reaction (reaction.emoji)}
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
                      <button
                        class="reaction-chip add"
                        on:click={() => addReactionPrompt(block.message.id as number)}
                        title="Add reaction"
                      >
                        +
                      </button>
                    </div>
                  {/if}
                  {#if threadReplyCounts.get(block.message.id)}
                    {@const replyCount = threadReplyCounts.get(block.message.id) ?? 0}
                    <button
                      type="button"
                      class="thread-indicator"
                      on:click={() => openThread(block.message.id as number)}
                      title="Open thread"
                    >
                      <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>
                      {replyCount} {replyCount === 1 ? 'reply' : 'replies'}
                    </button>
                  {/if}
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
        {#if replyingTo}
          <div class="reply-bar">
            <span class="reply-bar-label">
              Replying to <strong>{replyingTo.user}</strong>
              <span class="reply-bar-preview">{searchResultPreview(replyingTo)}</span>
            </span>
            <button
              type="button"
              class="reply-bar-cancel"
              on:click={cancelReply}
              aria-label="Cancel reply"
            >
              ✕
            </button>
          </div>
        {/if}
        {#if typingLabel}
          <div class="typing-indicator" aria-live="polite">
            <span class="typing-dots" aria-hidden="true"><span></span><span></span><span></span></span>
            {typingLabel}
          </div>
        {/if}
        <textarea
          class:scrollable={inputScrollable}
          bind:value={message}
          bind:this={messageInput}
          rows="1"
          placeholder="Message"
          on:input={handleComposerInput}
          on:paste={handlePaste}
          on:keydown={(e) => {
            if (e.key === 'Enter' && !e.shiftKey) {
              e.preventDefault();
              send();
            } else if (e.key === 'Escape' && replyingTo) {
              cancelReply();
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
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48"/></svg>
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

      {#if threadRootId !== null}
        <aside class="thread-panel" aria-label="Thread">
          <header class="thread-header">
            <span>Thread</span>
            <button
              type="button"
              class="thread-close"
              on:click={closeThread}
              aria-label="Close thread"
            >
              ✕
            </button>
          </header>
          <div class="thread-messages">
            {#each threadMessages as tm (tm.id)}
              <div class="thread-message" class:root={tm.id === threadRootId}>
                <div class="thread-message-meta">
                  <span class="username">{tm.user}</span>
                  <span class="timestamp">{tm.time}</span>
                </div>
                <div class="thread-message-text">
                  {#if tm.text}
                    {@html renderMarkdown(tm.text)}
                  {:else if tm.image}
                    <img src={tm.image as string} alt="" />
                  {:else if tm.attachment}
                    <a href={tm.attachment.url} target="_blank" rel="noopener noreferrer">
                      {tm.attachment.name}
                    </a>
                  {/if}
                </div>
              </div>
            {:else}
              <p class="thread-empty">Loading thread…</p>
            {/each}
          </div>
          <form class="thread-input" on:submit|preventDefault={sendThreadReply}>
            <input
              type="text"
              bind:value={threadReplyText}
              placeholder="Reply in thread…"
              aria-label="Reply in thread"
            />
          </form>
        </aside>
      {/if}

      {#if $dmActivePeer}
        <aside class="thread-panel" aria-label="Direct messages">
          <header class="thread-header">
            <span class="thread-header-title">
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><rect x="2" y="4" width="20" height="16" rx="2"/><path d="m22 7-8.97 5.7a1.94 1.94 0 0 1-2.06 0L2 7"/></svg>
              {$dmActivePeer}
            </span>
            <button
              type="button"
              class="thread-close"
              on:click={closeDm}
              aria-label="Close direct messages"
            >
              ✕
            </button>
          </header>
          <div class="thread-messages">
            {#each dmMessages as dmsg (dmsg.id)}
              <div class="thread-message" class:root={dmsg.from === $session.user}>
                <div class="thread-message-meta">
                  <span class="username">{dmsg.from}</span>
                  <span class="timestamp">{dmsg.time}</span>
                </div>
                <div class="thread-message-text">
                  {#if dmsg.text}
                    {@html renderMarkdown(dmsg.text)}
                  {/if}
                </div>
              </div>
            {:else}
              <p class="thread-empty">No messages yet. Say hi!</p>
            {/each}
          </div>
          <form class="thread-input" on:submit|preventDefault={sendDmMessage}>
            <input
              type="text"
              bind:value={dmText}
              placeholder={`Message ${$dmActivePeer}…`}
              aria-label="Direct message"
            />
          </form>
        </aside>
      {/if}

      {#each $voice as peer (peer.id)}
        <audio autoplay use:stream={{ stream: peer.stream, userId: peer.id }}></audio>
      {/each}
    </div>
    <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
    <div class="resizer" role="separator" aria-label="Resize user list" on:mousedown={startRightResize}></div>
    <UserList
      {statusMap}
      onUserContextMenu={openUserRoleMenu}
      onOpenDm={openDm}
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

{#if $connection === 'connecting' || $connection === 'disconnected' || $connection === 'failed'}
  <div class="connection-overlay" class:connecting={$connection === 'connecting'} role="alert">
    <div class="connection-card">
      {#if $connection === 'connecting'}
        <div class="connection-spinner" aria-hidden="true"></div>
        <h2>Connecting…</h2>
        <p class="connection-detail">{$selectedServer ?? 'Unknown server'}</p>
      {:else}
        <h2>{$connection === 'failed' ? 'Could not connect' : 'Connection lost'}</h2>
        <p>
          {$connection === 'failed'
            ? 'The server is offline or unreachable. Check the address or try again later.'
            : 'The connection to the server was lost. It may have gone offline.'}
        </p>
        <p class="connection-detail">{$selectedServer ?? 'Unknown server'}</p>
        <div class="connection-actions">
          <button type="button" class="btn btn-primary" on:click={retryConnect}>Try again</button>
          <button type="button" class="btn" on:click={() => leaveToServers()}>
            Back to servers
          </button>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  /* App shell: three full-height panes separated by 1px borders. The
     sidebars sit on --color-bg, the chat pane on --color-surface. */
  .page {
    display: flex;
    height: 100vh;
    background: var(--color-bg);
    overflow: hidden;
  }

  /* .channels/.sidebar are the root elements of ChannelSidebar/UserList;
     :global escapes Svelte's per-component style scoping. */
  .page.focus :global(.channels),
  .page.focus :global(.sidebar),
  .page.focus .resizer {
    display: none;
  }

  .page.focus .chat {
    max-width: 60rem;
    margin: 0 auto;
    border-left: 1px solid var(--color-surface-outline);
    border-right: 1px solid var(--color-surface-outline);
  }

  .resizer {
    width: 5px;
    margin: 0 -2px;
    cursor: col-resize;
    position: relative;
    flex-shrink: 0;
    z-index: 5;
  }

  .resizer:hover {
    background: color-mix(in srgb, var(--color-primary) 35%, transparent);
  }

  .chat {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
    position: relative;
    background: var(--color-surface);
    border-left: 1px solid var(--color-surface-outline);
    border-right: 1px solid var(--color-surface-outline);
  }

  /* pointer-events: none keeps drag events flowing to .chat underneath. */
  .drop-overlay {
    position: absolute;
    inset: 0;
    z-index: 30;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 2px dashed var(--color-primary);
    background: color-mix(in srgb, var(--color-surface) 80%, transparent);
    pointer-events: none;
  }

  .drop-overlay span {
    padding: var(--space-2) var(--space-4);
    border-radius: var(--radius-md);
    background: var(--color-surface-elevated);
    border: 1px solid var(--color-surface-outline);
    color: var(--color-on-surface);
    font-weight: 600;
    font-size: var(--text-md);
  }

  .messages-shell {
    flex: 1;
    min-height: 0;
    display: flex;
  }

  .messages {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    padding: var(--space-4) 0;
  }

  .day-separator {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    margin: var(--space-4) var(--space-4) var(--space-2);
    color: var(--color-muted);
    font-size: var(--text-xs);
    font-weight: 600;
  }

  .day-separator::before,
  .day-separator::after {
    content: '';
    flex: 1;
    border-top: 1px solid var(--color-surface-outline);
  }

  .unread-divider {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    margin: var(--space-2) var(--space-4);
    color: var(--color-error);
    font-size: var(--text-xs);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }

  .unread-divider::before,
  .unread-divider::after {
    content: '';
    flex: 1;
    border-top: 1px solid color-mix(in srgb, var(--color-error) 55%, transparent);
  }

  /* Messages are flat rows; hover reveals a floating action toolbar. */
  .message {
    position: relative;
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    column-gap: var(--space-2);
    row-gap: var(--space-1);
    padding: var(--space-1) var(--space-4);
    margin: 1px 0;
    border-left: 2px solid transparent;
    /* Isolate layout/style recalculation per message so long histories stay cheap. */
    contain: layout style;
  }

  .message:hover {
    background: color-mix(in srgb, var(--color-surface-raised) 45%, transparent);
  }

  .message.highlighted {
    background: color-mix(in srgb, var(--color-primary) 10%, transparent);
    border-left-color: var(--color-primary);
  }

  .message .username {
    font-weight: 600;
    font-size: var(--text-md);
    color: var(--color-on-surface);
  }

  .message .timestamp {
    font-size: var(--text-xs);
    color: var(--color-muted);
    font-family: var(--font-mono);
  }

  .message .role {
    font-size: var(--text-xs);
    font-weight: 600;
    color: var(--color-muted);
  }

  .reply-quote {
    flex-basis: 100%;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    max-width: 100%;
    padding: var(--space-1) var(--space-2);
    border: none;
    border-left: 2px solid var(--color-outline-strong);
    border-radius: 0 var(--radius-xs) var(--radius-xs) 0;
    background: color-mix(in srgb, var(--color-surface-raised) 55%, transparent);
    color: var(--color-muted);
    font-size: var(--text-sm);
    cursor: pointer;
    text-align: left;
  }

  .reply-quote:hover {
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
  }

  .reply-quote-arrow {
    flex-shrink: 0;
    opacity: 0.7;
  }

  .reply-quote-user {
    flex-shrink: 0;
    font-weight: 600;
    color: var(--color-on-surface-variant);
  }

  .reply-quote-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .bot-badge {
    display: inline-flex;
    align-items: center;
    align-self: center;
    padding: 0 var(--space-1);
    border-radius: var(--radius-xs);
    font-size: 0.625rem;
    font-weight: 700;
    letter-spacing: 0.04em;
    line-height: 1rem;
    background: var(--color-primary-container);
    color: var(--color-primary);
  }

  .content-wrapper {
    flex-basis: 100%;
    min-width: 0;
  }

  .message .content {
    display: block;
    color: var(--color-on-surface-variant);
    font-size: var(--text-md);
    line-height: 1.55;
    overflow-wrap: anywhere;
  }

  .message .content :global(p) {
    margin: 0;
  }

  .edited-badge {
    margin-left: var(--space-1);
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .ephemeral-badge {
    display: inline-flex;
    align-items: center;
    margin-top: var(--space-2);
    padding: 0 var(--space-2);
    border-radius: var(--radius-pill);
    font-size: var(--text-xs);
    font-weight: 600;
    line-height: 1.25rem;
    background: color-mix(in srgb, var(--color-warning) 15%, transparent);
    color: var(--color-warning);
    width: fit-content;
  }

  /* Floating per-message toolbar, shown on hover or keyboard focus. */
  .message-actions {
    position: absolute;
    top: calc(-1 * var(--space-3));
    right: var(--space-4);
    display: inline-flex;
    align-items: center;
    gap: 0;
    padding: var(--space-1);
    border-radius: var(--radius-sm);
    border: 1px solid var(--color-surface-outline);
    background: var(--color-surface-elevated);
    box-shadow: var(--shadow-sm);
    opacity: 0;
    pointer-events: none;
    z-index: 2;
  }

  .message:hover .message-actions,
  .message:focus-within .message-actions {
    opacity: 1;
    pointer-events: auto;
  }

  .message-action {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.75rem;
    height: 1.75rem;
    padding: 0;
    border-radius: var(--radius-xs);
    border: none;
    background: transparent;
    color: var(--color-muted);
    cursor: pointer;
  }

  .message-action:hover,
  .message-action.active {
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
  }

  .message-action.active {
    color: var(--color-primary);
  }

  .message-action.danger:hover {
    background: color-mix(in srgb, var(--color-error) 14%, transparent);
    color: var(--color-error);
  }

  .link-previews {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }

  .message .content :global(code) {
    background: var(--color-surface-raised);
    padding: 0.125rem var(--space-1);
    border-radius: var(--radius-xs);
    font-family: var(--font-mono);
    font-size: 0.85em;
  }

  .message .content :global(pre) {
    background: var(--color-bg);
    border-radius: var(--radius-md);
    padding: var(--space-3);
    overflow-x: auto;
    border: 1px solid var(--color-surface-outline);
  }

  .message .content :global(pre code) {
    display: block;
    padding: 0;
    margin: 0;
    background: transparent;
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .message .content :global(.hljs) {
    color: var(--color-on-surface);
  }

  .message .content :global(.hljs-comment),
  .message .content :global(.hljs-quote) {
    color: var(--color-muted);
    font-style: italic;
  }

  .message .content :global(.hljs-keyword),
  .message .content :global(.hljs-selector-tag),
  .message .content :global(.hljs-subst) {
    color: color-mix(in srgb, var(--color-primary) 80%, var(--color-on-surface) 20%);
  }

  .message .content :global(.hljs-string),
  .message .content :global(.hljs-doctag),
  .message .content :global(.hljs-regexp) {
    color: color-mix(in srgb, var(--color-success) 75%, var(--color-on-surface) 25%);
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
    color: color-mix(in srgb, var(--color-error) 70%, var(--color-on-surface) 30%);
  }

  .message .content :global(.hljs-meta),
  .message .content :global(.hljs-meta .hljs-string) {
    color: color-mix(in srgb, var(--color-primary) 65%, var(--color-on-surface) 35%);
  }

  .message img {
    max-width: min(420px, 100%);
    border-radius: var(--radius-md);
    margin-top: var(--space-2);
    border: 1px solid var(--color-surface-outline);
  }

  .attachment-card {
    display: inline-flex;
    align-items: center;
    gap: var(--space-3);
    margin-top: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    border: 1px solid var(--color-surface-outline);
    background: var(--color-surface-elevated);
    color: var(--color-on-surface);
    text-decoration: none;
    max-width: min(420px, 100%);
  }

  .attachment-card:hover {
    border-color: var(--color-outline-strong);
    background: var(--color-surface-raised);
    text-decoration: none;
  }

  .attachment-icon {
    display: inline-flex;
    color: var(--color-primary);
    flex-shrink: 0;
  }

  .attachment-details {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .attachment-name {
    font-weight: 500;
    font-size: var(--text-md);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .attachment-size {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .reactions {
    flex-basis: 100%;
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin-top: var(--space-1);
  }

  .reaction-chip {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    border-radius: var(--radius-pill);
    padding: 0.125rem var(--space-2);
    font-size: var(--text-sm);
    line-height: 1.25rem;
    border: 1px solid var(--color-surface-outline);
    background: var(--color-surface-elevated);
    color: var(--color-on-surface-variant);
  }

  .reaction-chip:hover {
    border-color: var(--color-outline-strong);
  }

  .reaction-chip.active {
    background: var(--color-primary-container);
    border-color: color-mix(in srgb, var(--color-primary) 45%, transparent);
    color: var(--color-on-surface);
  }

  .reaction-chip.add {
    color: var(--color-muted);
    font-weight: 600;
  }

  .thread-indicator {
    justify-self: flex-start;
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
    padding: 0.125rem var(--space-2);
    border-radius: var(--radius-pill);
    border: 1px solid transparent;
    background: transparent;
    color: var(--color-primary);
    font-size: var(--text-sm);
    font-weight: 500;
    cursor: pointer;
  }

  .thread-indicator:hover {
    background: var(--color-primary-container);
  }

  /* Composer */
  .input-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: var(--space-2);
    align-items: end;
    padding: var(--space-3) var(--space-4) var(--space-4);
    border-top: 1px solid var(--color-surface-outline);
    background: var(--color-surface);
  }

  .command-feedback {
    grid-column: 1 / -1;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    font-size: var(--text-sm);
    font-weight: 500;
    border: 1px solid color-mix(in srgb, var(--color-success) 30%, transparent);
    background: color-mix(in srgb, var(--color-success) 10%, transparent);
    color: var(--color-success);
  }

  .command-feedback.error {
    border-color: color-mix(in srgb, var(--color-error) 35%, transparent);
    background: color-mix(in srgb, var(--color-error) 10%, transparent);
    color: var(--color-error);
  }

  .reply-bar {
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--color-surface-raised);
    font-size: var(--text-sm);
    color: var(--color-muted);
  }

  .reply-bar-label {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    overflow: hidden;
  }

  .reply-bar-label strong {
    color: var(--color-on-surface);
  }

  .reply-bar-preview {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .reply-bar-cancel {
    flex-shrink: 0;
    border: none;
    background: transparent;
    color: var(--color-muted);
    cursor: pointer;
    font-size: var(--text-sm);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-xs);
  }

  .reply-bar-cancel:hover {
    color: var(--color-on-surface);
    background: var(--color-surface-elevated);
  }

  .typing-indicator {
    grid-column: 1 / -1;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .typing-dots {
    display: inline-flex;
    gap: 0.1875rem;
  }

  .typing-dots span {
    width: 0.25rem;
    height: 0.25rem;
    border-radius: 50%;
    background: var(--color-muted);
    animation: typing-bounce 1.2s infinite ease-in-out;
  }

  .typing-dots span:nth-child(2) {
    animation-delay: 0.15s;
  }

  .typing-dots span:nth-child(3) {
    animation-delay: 0.3s;
  }

  @keyframes typing-bounce {
    0%,
    60%,
    100% {
      transform: translateY(0);
      opacity: 0.5;
    }
    30% {
      transform: translateY(-3px);
      opacity: 1;
    }
  }

  textarea {
    width: 100%;
    min-height: var(--control-height-lg);
    max-height: 360px;
    resize: none;
    overflow-y: hidden;
    overflow-x: hidden;
    border-radius: var(--radius-md);
    padding: var(--space-3) var(--space-4);
    line-height: 1.5;
  }

  textarea.scrollable {
    overflow-y: auto;
  }

  .controls {
    display: flex;
    align-items: flex-end;
    gap: var(--space-2);
  }

  .input-controls {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .preview-container {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-1);
    background: var(--color-surface-raised);
    padding: var(--space-2);
    border-radius: var(--radius-md);
    border: 1px dashed var(--color-outline-strong);
  }

  .preview-container img {
    max-width: 120px;
    max-height: 120px;
    border-radius: var(--radius-sm);
  }

  .preview-remove {
    background: transparent;
    color: var(--color-muted);
    border: none;
    display: inline-flex;
    padding: var(--space-1);
    border-radius: var(--radius-xs);
  }

  .preview-remove:hover {
    color: var(--color-error);
    background: color-mix(in srgb, var(--color-error) 12%, transparent);
  }

  .file-chip {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    max-width: 220px;
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--color-surface-elevated);
    font-size: var(--text-sm);
  }

  .file-chip svg {
    flex-shrink: 0;
    color: var(--color-primary);
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
    width: var(--control-height-lg);
    height: var(--control-height-lg);
    flex-shrink: 0;
    border-radius: var(--radius-md);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }

  .file-button {
    border: 1px solid var(--color-surface-outline);
    background: var(--color-surface-raised);
    color: var(--color-on-surface-variant);
  }

  .file-button:hover {
    border-color: var(--color-outline-strong);
    color: var(--color-on-surface);
  }

  .send {
    border: none;
    background: var(--color-primary);
    color: var(--color-on-primary);
  }

  .send:hover {
    background: color-mix(in srgb, var(--color-primary) 88%, var(--color-on-surface));
  }

  .file-button svg,
  .send svg {
    width: 1.25rem;
    height: 1.25rem;
  }

  /* Thread / DM side panel */
  .thread-panel {
    position: absolute;
    top: 0;
    right: 0;
    bottom: 0;
    z-index: 25;
    width: min(360px, 90%);
    display: flex;
    flex-direction: column;
    background: var(--color-surface-elevated);
    border-left: 1px solid var(--color-surface-outline);
    box-shadow: var(--shadow-md);
  }

  .thread-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--color-surface-outline);
    font-weight: 600;
    font-size: var(--text-md);
  }

  .thread-header-title {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .thread-header-title svg {
    color: var(--color-primary);
    flex-shrink: 0;
  }

  .thread-close {
    border: none;
    background: transparent;
    color: var(--color-muted);
    cursor: pointer;
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-xs);
  }

  .thread-close:hover {
    color: var(--color-on-surface);
    background: var(--color-surface-raised);
  }

  .thread-messages {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-3);
  }

  .thread-message {
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
    background: var(--color-surface-raised);
  }

  .thread-message.root {
    background: var(--color-primary-container);
  }

  .thread-message-meta {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
  }

  .thread-message-meta .username {
    font-weight: 600;
    color: var(--color-on-surface);
    font-size: var(--text-sm);
  }

  .thread-message-meta .timestamp {
    font-size: var(--text-xs);
    color: var(--color-muted);
    font-family: var(--font-mono);
  }

  .thread-message-text {
    color: var(--color-on-surface-variant);
    font-size: var(--text-sm);
    line-height: 1.55;
    overflow-wrap: anywhere;
  }

  .thread-message-text :global(p) {
    margin: 0;
  }

  .thread-message-text img {
    max-width: 100%;
    border-radius: var(--radius-sm);
    margin-top: var(--space-1);
  }

  .thread-empty {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-sm);
    text-align: center;
    padding: var(--space-4);
  }

  .thread-input {
    padding: var(--space-3);
    border-top: 1px solid var(--color-surface-outline);
  }

  .thread-input input {
    width: 100%;
    border-radius: var(--radius-md);
  }

  /* Connection overlay */
  .connection-overlay {
    position: fixed;
    inset: 0;
    z-index: var(--z-overlay);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-5);
    background: var(--color-overlay);
    backdrop-filter: blur(6px);
  }

  /* Delay the connecting state so fast connects never flash the overlay. */
  .connection-overlay.connecting {
    animation: connection-fade-in 0.2s ease 0.4s both;
  }

  @keyframes connection-fade-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  .connection-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-3);
    max-width: 420px;
    padding: var(--space-6);
    border-radius: var(--radius-lg);
    background: var(--color-surface-elevated);
    border: 1px solid var(--color-surface-outline);
    box-shadow: var(--shadow-lg);
    text-align: center;
  }

  .connection-card h2 {
    font-size: var(--text-xl);
  }

  .connection-card p {
    margin: 0;
    color: var(--color-muted);
    line-height: 1.5;
  }

  .connection-detail {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    word-break: break-all;
  }

  .connection-spinner {
    width: var(--space-6);
    height: var(--space-6);
    border-radius: 50%;
    border: 3px solid var(--color-surface-raised);
    border-top-color: var(--color-primary);
    animation: connection-spin 0.8s linear infinite;
  }

  @keyframes connection-spin {
    to {
      transform: rotate(360deg);
    }
  }

  .connection-actions {
    display: flex;
    gap: var(--space-3);
    margin-top: var(--space-2);
    flex-wrap: wrap;
    justify-content: center;
  }

  /* Responsive: stack panes on narrow windows. */
  @media (max-width: 1100px) {
    .page {
      flex-direction: column;
      height: auto;
      min-height: 100vh;
      overflow: visible;
    }

    .page :global(.channels),
    .page :global(.sidebar) {
      width: 100% !important;
      max-height: 40vh;
      order: 0;
      border-bottom: 1px solid var(--color-surface-outline);
    }

    .resizer {
      display: none;
    }

    .chat {
      order: 1;
      border-left: none;
      border-right: none;
      min-height: 60vh;
    }

    .page :global(.sidebar) {
      order: 2;
      border-bottom: none;
      border-top: 1px solid var(--color-surface-outline);
    }
  }

  @media (max-width: 640px) {
    .input-row {
      grid-template-columns: 1fr;
    }

    .controls {
      justify-content: flex-end;
    }
  }
</style>
