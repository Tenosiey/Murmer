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
  import MessageItem from '$lib/components/chat/MessageItem.svelte';
  import MessageComposer from '$lib/components/chat/MessageComposer.svelte';
  import ConversationPanel from '$lib/components/chat/ConversationPanel.svelte';
  import ConnectionOverlay from '$lib/components/chat/ConnectionOverlay.svelte';
  import { ping } from '$lib/stores/ping';
  import { channels } from '$lib/stores/channels';
  import { voiceChannels } from '$lib/stores/voiceChannels';
  import { categories } from '$lib/stores/categories';
  import type { CategoryInfo, ChannelInfo, ContextMenuItem } from '$lib/types';
  import { leftSidebarWidth, rightSidebarWidth } from '$lib/stores/layout';
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
  import type { Message, UserStatus, ScreenSharePeer } from '$lib/types';
  import {
    pingToStrength,
    buildMessageBlocks,
    describeDuration,
    type MessageBlock
  } from '$lib/chat/helpers';
  import { dialogs } from '$lib/stores/dialogs';
  import {
    hotkeys,
    eventToCombo,
    firesWhileTyping,
    isTextInputTarget,
    type HotkeyActionId
  } from '$lib/stores/hotkeys';
  import {
    setGlobalHotkeyActions,
    clearGlobalHotkeyActions
  } from '$lib/stores/globalHotkeys';
  import EmojiPicker from '$lib/components/EmojiPicker.svelte';
  import {
    MODERATOR_ROLES,
    MAX_TOPIC_LENGTH,
    MIN_EPHEMERAL_SECONDS,
    MAX_EPHEMERAL_SECONDS,
    VOICE_QUALITY_PRESETS,
    DEFAULT_VOICE_PRESET,
    DEFAULT_CHANNEL_NAME,
    roleRank
  } from '$lib/chat/constants';
  import ServerDashboardModal from '$lib/components/ServerDashboardModal.svelte';
  import UserStatsModal from '$lib/components/UserStatsModal.svelte';
  import WikiView from '$lib/components/wiki/WikiView.svelte';
  import { wikilinks } from '$lib/wiki/links';

  let serverStrength = 0;
  $: serverStrength = pingToStrength($ping);

  let message = '';
  let composer: MessageComposer;
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

  let wikiOpen = false;
  /** Page a wikilink asked to open; consumed by WikiView on mount. */
  let wikiInitialSlug: string | null = null;

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

  function clearPendingFile() {
    setPendingFile(null);
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

  function toggleReaction(messageId: number | undefined, emoji: string, users: string[]) {
    if (typeof messageId !== 'number') return;
    const current = $session.user;
    if (!current) return;
    const hasReaction = users.includes(current);
    chat.react(messageId, emoji, hasReaction ? 'remove' : 'add');
  }

  let emojiPickerOpen = false;
  let emojiPickerX = 0;
  let emojiPickerY = 0;
  let emojiPickerMessageId: number | null = null;

  function openEmojiPicker(messageId: number | undefined, event: MouseEvent) {
    if (typeof messageId !== 'number') return;
    emojiPickerMessageId = messageId;
    emojiPickerX = event.clientX;
    emojiPickerY = event.clientY;
    emojiPickerOpen = true;
  }

  function closeEmojiPicker() {
    emojiPickerOpen = false;
    emojiPickerMessageId = null;
  }

  function pickReaction(emoji: string) {
    if (emojiPickerMessageId === null) return;
    chat.react(emojiPickerMessageId, emoji, 'add');
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

  /* The server drops every connection into "general" and sends its history with
     the presence response, so the initial pick has to be "general" too — the
     channel list arrives sorted by name, which puts anything sorting ahead of
     it at index 0. The fallback only covers servers seeded without "general". */
  function defaultChannel(list: ChannelInfo[]): ChannelInfo {
    return list.find((c) => c.name === DEFAULT_CHANNEL_NAME) ?? list[0];
  }

  $: if ($channels.length && !$channels.some((c) => c.id === currentChatChannelId)) {
    currentChatChannelId = defaultChannel($channels).id;
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
    // Voice hotkeys also fire as OS-level global shortcuts while another
    // window has focus (Tauri shell only; a no-op in the plain browser).
    setGlobalHotkeyActions({
      toggleMic: toggleMicrophone,
      toggleDeafen: toggleOutput,
      toggleVoice: toggleVoiceChannel
    });
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
    clearGlobalHotkeyActions();
  });

  function sendText() {
    const trimmed = message.trim();
    if (trimmed === '') return;
    if (trimmed.startsWith('/')) {
      if (handleSlashCommand(trimmed)) {
        message = '';
        return;
      }
    }
    const replyTarget = typeof replyingTo?.id === 'number' ? replyingTo.id : undefined;
    chat.send($session.user ?? 'anon', message, replyTarget);
    replyingTo = null;
    message = '';
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
    wikiInitialSlug = null;
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

  function toggleWiki() {
    if (wikiOpen) {
      wikiOpen = false;
      wikiInitialSlug = null;
    } else {
      wikiOpen = true;
    }
  }

  /**
   * Open the wiki at a page, switching channels first for
   * `[[channel/page]]` links. Called from wikilinks in chat messages and
   * from cross-channel links inside the wiki view.
   */
  function openWikiPage(channelName: string | null, slug: string) {
    if (channelName && channelName !== currentChatChannelName) {
      const target = $channels.find((c) => c.name === channelName);
      if (!target) {
        void dialogs.alert({
          title: 'Wiki',
          message: `Channel "${channelName}" was not found on this server.`
        });
        return;
      }
      joinChannel(target.id);
    }
    wikiInitialSlug = slug;
    wikiOpen = true;
  }

  function startReply(msg: Message) {
    if (typeof msg.id !== 'number') return;
    replyingTo = msg;
    composer?.focusInput();
  }

  function cancelReply() {
    replyingTo = null;
  }

  function openThread(rootId: number) {
    threadRootId = rootId;
    chat.loadThread(rootId);
  }

  function closeThread() {
    threadRootId = null;
  }

  function sendThreadReply(text: string) {
    if (threadRootId === null) return;
    chat.send($session.user ?? 'anon', text, threadRootId);
  }

  function openDm(user: string) {
    if (user === $session.user) return;
    closeThread();
    dm.open(user);
    chat.loadDmHistory(user);
  }

  function closeDm() {
    dm.close();
  }

  function sendDmMessage(text: string) {
    const peer = $dmActivePeer;
    if (!peer) return;
    chat.sendDm(peer, text);
  }

  $: dmMessages = $dmActivePeer ? ($dmConversations[$dmActivePeer] ?? []) : [];

  function handleComposerInput() {
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
        await dialogs.alert({
          title: 'Join a voice channel first',
          message: 'You must be in a voice channel to view screen shares.'
        });
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
      dialogs.alert({
        title: 'Screen share unavailable',
        message: 'Could not open this screen share. The stream may have ended.'
      });
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

  async function createChannelPrompt(categoryId: number | null = null) {
    const name = await dialogs.prompt({
      title: 'Create text channel',
      label: 'Channel name',
      placeholder: 'e.g. general',
      confirmLabel: 'Create'
    });
    if (name) channels.create(name.trim(), categoryId);
  }

  async function selectVoicePreset(): Promise<{ quality: string; bitrate: number | null } | null> {
    const quality = await dialogs.select({
      title: 'Voice quality',
      options: VOICE_QUALITY_PRESETS.map((preset) => ({
        value: preset.quality,
        label: preset.label,
        description:
          preset.bitrate && preset.bitrate > 0
            ? `${Math.round(preset.bitrate / 1000)} kbps`
            : 'Uncompressed audio'
      })),
      initial: DEFAULT_VOICE_PRESET.quality,
      confirmLabel: 'Apply'
    });
    if (quality === null) return null;
    const preset = VOICE_QUALITY_PRESETS.find((p) => p.quality === quality) ?? DEFAULT_VOICE_PRESET;
    return { quality: preset.quality, bitrate: preset.bitrate };
  }

  async function createVoiceChannelPrompt(categoryId: number | null = null) {
    const name = await dialogs.prompt({
      title: 'Create voice channel',
      label: 'Channel name',
      placeholder: 'e.g. Lounge',
      confirmLabel: 'Next'
    });
    if (!name) return;
    const preset = await selectVoicePreset();
    if (!preset) return;
    voiceChannels.create(name.trim(), preset, categoryId);
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

  $: currentUserIsOwner = (() => {
    const user = $session.user;
    if (!user) return false;
    const info = $roles[user];
    return info?.role?.toLowerCase() === 'owner';
  })();

  // Requesters must strictly outrank their target for kick/ban/mute.
  $: currentUserModerationRank = roleRank(
    $session.user ? $roles[$session.user]?.role : undefined
  );

  function canModerate(target: string): boolean {
    if (target === $session.user) return false;
    return currentUserModerationRank > roleRank($roles[target]?.role);
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

  async function banUser(user: string) {
    const confirmed = await dialogs.confirm({
      title: `Ban ${user}?`,
      message: 'They will be disconnected and unable to rejoin until unbanned.',
      confirmLabel: 'Ban user',
      danger: true
    });
    if (!confirmed) return;
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
    const items: ContextMenuItem[] = [];
    items.push({ label: 'Send Message', action: () => openDm(target) });
    items.push({ label: 'View Stats', action: () => openUserStats(target) });
    if (currentUserIsOwner) {
      const roleItems: ContextMenuItem[] = ASSIGNABLE_ROLES.filter(
        (role) => currentRole?.toLowerCase() !== role.toLowerCase()
      ).map((role) => ({ label: role, action: () => assignRole(target, role) }));
      if (currentRole) {
        roleItems.push({ label: 'Remove Role', action: () => removeRole(target), danger: true });
      }
      if (roleItems.length) {
        items.push({ label: 'Set Role', children: roleItems });
      }
    }
    if (canModerate(target)) {
      items.push({
        label: 'Mute',
        children: [
          { label: '10 minutes', action: () => muteUser(target, 600) },
          { label: '1 hour', action: () => muteUser(target, 3600) },
          { label: 'Until lifted', action: () => muteUser(target) },
          { label: 'Unmute', action: () => unmuteUser(target) }
        ]
      });
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

  let serverDashboardOpen = false;

  function openServerDashboard() {
    serverDashboardOpen = true;
  }

  function closeServerDashboard() {
    serverDashboardOpen = false;
  }

  let statsUser: string | null = null;

  function openUserStats(user: string) {
    statsUser = user;
  }

  function closeUserStats() {
    statsUser = null;
  }

  async function editTopic() {
    const existing = $channelTopics[currentChatChannelId] ?? '';
    const input = await dialogs.prompt({
      title: 'Channel topic',
      message: 'Shown next to the channel name. Leave empty to clear the topic.',
      initial: existing,
      placeholder: 'What is this channel about?',
      maxLength: MAX_TOPIC_LENGTH,
      confirmLabel: 'Save',
      required: false
    });
    if (input === null) return;
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

  async function editChatMessage(msg: Message) {
    if (typeof msg.id !== 'number' || typeof msg.text !== 'string') return;
    const input = await dialogs.prompt({
      title: 'Edit message',
      initial: msg.text,
      multiline: true,
      confirmLabel: 'Save'
    });
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
    const confirmed = await dialogs.confirm({
      title: 'Delete message?',
      message: 'This removes the message for everyone. This cannot be undone.',
      confirmLabel: 'Delete',
      danger: true
    });
    if (!confirmed) return;
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

  /** Leave the current voice channel, or join the last/first one. */
  function toggleVoiceChannel() {
    if (inVoice) {
      leaveVoice();
    } else {
      const channels = $voiceChannels;
      if (channels.length) {
        joinVoiceChannel(currentVoiceChannelId ?? channels[0].id);
      }
    }
  }

  function handleGlobalShortcut(event: KeyboardEvent) {
    if (event.defaultPrevented) return;
    const combo = eventToCombo(event);
    if (!combo) return;

    const action = (Object.entries($hotkeys) as Array<[HotkeyActionId, string | null]>).find(
      ([, bound]) => bound === combo
    )?.[0];
    if (!action) return;

    // A binding without a real modifier (e.g. plain "M") must not fire while
    // the user is typing a message.
    if (isTextInputTarget(event.target) && !firesWhileTyping(combo)) return;

    event.preventDefault();
    switch (action) {
      case 'toggleMic':
        toggleMicrophone();
        break;
      case 'toggleDeafen':
        toggleOutput();
        break;
      case 'toggleVoice':
        toggleVoiceChannel();
        break;
      case 'openSearch':
        openSearch();
        break;
      case 'openSettings':
        openSettings();
        break;
      case 'openHelp':
        openHelp();
        break;
    }
  }

  async function createCategoryPrompt() {
    const name = await dialogs.prompt({
      title: 'Create category',
      label: 'Category name',
      placeholder: 'e.g. Projects',
      confirmLabel: 'Create'
    });
    if (name) categories.create(name.trim());
  }

  async function renameCategoryPrompt(id: number) {
    const cat = $categories.find((c) => c.id === id);
    const name = await dialogs.prompt({
      title: 'Rename category',
      label: 'Category name',
      initial: cat?.name ?? '',
      confirmLabel: 'Rename'
    });
    if (name) categories.rename(id, name.trim());
  }

  /** Builds the "Move to" submenu; empty when there is nowhere to move to. */
  function buildMoveToItems(channelId: number, voice: boolean): ContextMenuItem[] {
    const targets: ContextMenuItem[] = [];
    const currentCh = voice
      ? $voiceChannels.find((c) => c.id === channelId)
      : $channels.find((c) => c.id === channelId);
    const currentCatId = currentCh?.categoryId ?? null;

    if (currentCatId !== null) {
      targets.push({
        label: '(no category)',
        action: () => channels.move(channelId, null, voice)
      });
    }

    for (const cat of $categories) {
      if (cat.id !== currentCatId) {
        targets.push({
          label: cat.name,
          action: () => channels.move(channelId, cat.id, voice)
        });
      }
    }

    return targets.length ? [{ label: 'Move to', children: targets }] : [];
  }

  $: channelMenuItems = [
    {
      label: 'Create',
      children: [
        { label: 'Text Channel', action: () => createChannelPrompt() },
        { label: 'Voice Channel', action: () => createVoiceChannelPrompt() },
        { label: 'Category', action: createCategoryPrompt }
      ]
    },
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

  <div class="page">
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
        {wikiOpen}
        onToggleWiki={toggleWiki}
        showServerDashboard={currentUserModerationRank >= 1}
        onOpenServerDashboard={openServerDashboard}
        onLeaveServer={leaveServer}
        onLogout={logout}
      />
      <SettingsModal open={settingsOpen} close={closeSettings} />
      <ServerDashboardModal
        open={serverDashboardOpen}
        close={closeServerDashboard}
        rank={currentUserModerationRank}
      />
      <UserStatsModal open={statsUser !== null} user={statsUser} close={closeUserStats} />
      <HelpOverlay bind:this={helpOverlay} open={helpOpen} onClose={closeHelp} />
      <SearchOverlay
        bind:this={searchOverlay}
        open={searchOpen}
        onClose={closeSearch}
        onSearch={doSearch}
        onFocusResult={handleSearchResult}
        {now}
      />
      {#if wikiOpen}
        <!-- Edit affordances are shown to everyone; the server enforces the
             channel-management gate, same as channel/topic management. -->
        {#key currentChatChannelId}
          <WikiView
            channelId={currentChatChannelId}
            channelName={currentChatChannelName}
            canEdit={true}
            initialSlug={wikiInitialSlug}
            onCrossChannel={openWikiPage}
          />
        {/key}
      {:else}
      <PinnedBar
        entries={pinnedEntries}
        messages={channelMessages}
        onFocusMessage={focusMessage}
      />
      <div class="messages-shell">
        <div
          class="messages"
          bind:this={messagesContainer}
          on:scroll={onScroll}
          use:wikilinks={{
            channelName: currentChatChannelName,
            onNavigate: (nav) => openWikiPage(nav.channel, nav.slug)
          }}
        >
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
              <MessageItem
                message={block.message}
                links={block.links}
                continuation={block.continuation}
                {now}
                highlighted={highlightedMessageId === block.message.id}
                pinned={isMessagePinned(block.message)}
                replyCount={typeof block.message.id === 'number'
                  ? (threadReplyCounts.get(block.message.id) ?? 0)
                  : 0}
                canEdit={canEditMessage(block.message)}
                canDelete={canDeleteMessage(block.message)}
                canPin={canPinMessage(block.message)}
                onFocusMessage={focusMessage}
                onReply={startReply}
                onEdit={editChatMessage}
                onTogglePin={togglePinMessage}
                onDelete={deleteChatMessage}
                onOpenEmojiPicker={openEmojiPicker}
                onToggleReaction={toggleReaction}
                onOpenThread={openThread}
              />
            {/if}
          {:else}
            <div class="channel-empty">
              <h3>Welcome to #{currentChatChannelName}</h3>
              <p>This is the beginning of the channel. Say hi!</p>
            </div>
          {/each}
        </div>
      </div>
      <MessageComposer
        bind:this={composer}
        bind:value={message}
        {replyingTo}
        {typingLabel}
        {commandFeedback}
        {commandFeedbackType}
        {pendingFile}
        {previewUrl}
        onSend={send}
        onInput={handleComposerInput}
        onCancelReply={cancelReply}
        onFileSelected={setPendingFile}
      />
      {/if}

      {#if threadRootId !== null}
        <ConversationPanel
          kind="thread"
          title="Thread"
          messages={threadMessages}
          emptyText="Loading thread…"
          placeholder="Reply in thread…"
          onSend={sendThreadReply}
          onClose={closeThread}
          emphasize={(msg) => msg.id === threadRootId}
        />
      {/if}

      {#if $dmActivePeer}
        <ConversationPanel
          kind="dm"
          title={$dmActivePeer}
          messages={dmMessages}
          emptyText="No messages yet. Say hi!"
          placeholder={`Message ${$dmActivePeer}…`}
          onSend={sendDmMessage}
          onClose={closeDm}
          emphasize={(msg) => msg.from === $session.user}
        />
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

<EmojiPicker
  open={emojiPickerOpen}
  x={emojiPickerX}
  y={emojiPickerY}
  onPick={pickReaction}
  onClose={closeEmojiPicker}
/>

{#if viewingScreenShare}
  <ScreenShareViewer peer={viewingScreenShare} onClose={closeScreenShareViewer} />
{/if}

{#if $connection === 'connecting' || $connection === 'disconnected' || $connection === 'failed'}
  <ConnectionOverlay
    state={$connection}
    server={$selectedServer}
    onRetry={retryConnect}
    onBack={() => leaveToServers()}
  />
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

  .channel-empty {
    margin: auto;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-6);
    text-align: center;
  }

  .channel-empty h3 {
    font-size: var(--text-xl);
  }

  .channel-empty p {
    margin: 0;
    color: var(--color-muted);
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
</style>
