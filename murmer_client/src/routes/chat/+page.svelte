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
  import { voice, voiceStats } from '$lib/stores/voice';
  import { selectedServer, servers } from '$lib/stores/servers';
  import { onlineUsers } from '$lib/stores/online';
  import { allUsers, offlineUsers } from '$lib/stores/users';
  import { voiceUsers } from '$lib/stores/voiceUsers';
  import { volume, outputDeviceId, outputMuted, microphoneMuted, userVolumes, setUserVolume, voiceMode, voiceActivity, isPttActive } from '$lib/stores/settings';
  import { remoteSpeaking, setRemoteSpeaking } from '$lib/stores/voiceSpeaking';
  import { get } from 'svelte/store';
  import { goto } from '$app/navigation';
  import ConnectionBars from '$lib/components/ConnectionBars.svelte';
  import SettingsModal from '$lib/components/SettingsModal.svelte';
  import PingDot from '$lib/components/PingDot.svelte';
  import LinkPreview from '$lib/components/LinkPreview.svelte';
  import ContextMenu from '$lib/components/ContextMenu.svelte';
  import { ping } from '$lib/stores/ping';
  import { channels } from '$lib/stores/channels';
  import { voiceChannels } from '$lib/stores/voiceChannels';
  import { leftSidebarWidth, rightSidebarWidth, focusMode } from '$lib/stores/layout';
  import { channelTopics } from '$lib/stores/channelTopics';
  import { theme } from '$lib/stores/theme';
  import { statuses, STATUS_LABELS, STATUS_EMOJIS, USER_STATUS_VALUES } from '$lib/stores/status';
  import { pinned } from '$lib/stores/pins';
  import type { PinnedEntry } from '$lib/stores/pins';
  import { channelNotifications, type ChannelNotificationPreference } from '$lib/stores/channelNotifications';
  import { loadKeyPair, sign } from '$lib/keypair';
  import { renderMarkdown } from '$lib/markdown';
  import { extractLinks } from '$lib/link-preview';
  import type { Message, UserStatus, VoiceChannelInfo } from '$lib/types';
  function pingToStrength(ms: number): number {
    return ms === 0 ? 5 : ms < 50 ? 5 : ms < 100 ? 4 : ms < 200 ? 3 : ms < 400 ? 2 : 1;
  }

  let serverStrength = 0;
  $: serverStrength = pingToStrength($ping);

  let message = '';
  let fileInput: HTMLInputElement;
  let messageInput: HTMLTextAreaElement;
  let inputScrollable = false;
  let previewUrl: string | null = null;
  let menuOpen = false;
  let menuX = 0;
  let menuY = 0;
  const statusOptions: Array<{ value: UserStatus; label: string; emoji: string }> = USER_STATUS_VALUES.map((value) => ({
    value,
    label: STATUS_LABELS[value],
    emoji: STATUS_EMOJIS[value]
  }));
  let statusMenuOpen = false;
  let statusMenuButton: HTMLButtonElement | null = null;
  let statusMenuElement: HTMLDivElement | null = null;
  let statusMap: Record<string, UserStatus> = {};
  const MESSAGE_INPUT_MAX_HEIGHT = 360;

  const MODERATOR_ROLES = ['Admin', 'Mod', 'Owner'];
  const NOTIFICATION_OPTIONS: Array<{
    value: ChannelNotificationPreference;
    label: string;
    description: string;
    icon: string;
  }> = [
    { value: 'all', label: 'All messages', description: 'Send alerts for every new message', icon: '🔔' },
    { value: 'mentions', label: 'Mentions only', description: 'Only alert when you are mentioned', icon: '@' },
    { value: 'mute', label: 'Muted', description: 'Do not show notifications for this channel', icon: '🔕' }
  ];

  let notificationMenuOpen = false;
  let notificationMenuButton: HTMLButtonElement | null = null;
  let notificationMenuElement: HTMLDivElement | null = null;
  let currentNotificationPreference: ChannelNotificationPreference = 'all';
  let notificationMenuLabel = 'All messages';

  const PIN_PREVIEW_LIMIT = 120;

  let pinnedEntries: PinnedEntry[] = [];
  let highlightedMessageId: number | null = null;
  let pendingScrollToMessage: number | null = null;
  let highlightTimer: ReturnType<typeof setTimeout> | null = null;
  let currentUserCanModerate = false;

  const VOICE_QUALITY_PRESETS: Array<{ quality: string; bitrate: number | null; label: string }> = [
    { quality: 'low', bitrate: 32_000, label: 'Low' },
    { quality: 'standard', bitrate: 64_000, label: 'Standard' },
    { quality: 'high', bitrate: 96_000, label: 'High' },
    { quality: 'ultra', bitrate: 128_000, label: 'Ultra' },
    { quality: 'lossless', bitrate: null, label: 'Lossless' }
  ];
  const DEFAULT_VOICE_PRESET = VOICE_QUALITY_PRESETS[1];

  function formatVoiceQuality(info: VoiceChannelInfo): string {
    const preset = VOICE_QUALITY_PRESETS.find((p) => p.quality === info.quality);
    const bitrate = info.bitrate ?? preset?.bitrate ?? null;
    const label = preset ? preset.label : info.quality;
    return bitrate && bitrate > 0 ? `${label} (${Math.round(bitrate / 1000)} kbps)` : label;
  }

  function promptVoicePreset(): { quality: string; bitrate: number | null } {
    const input = prompt(
      'Voice quality (low, standard, high, ultra, lossless)',
      DEFAULT_VOICE_PRESET.quality
    );
    if (!input) return { quality: DEFAULT_VOICE_PRESET.quality, bitrate: DEFAULT_VOICE_PRESET.bitrate };
    const normalized = input.trim().toLowerCase();
    const preset = VOICE_QUALITY_PRESETS.find((p) => p.quality === normalized) ?? DEFAULT_VOICE_PRESET;
    return { quality: preset.quality, bitrate: preset.bitrate };
  }

  type MessageBlock =
    | { kind: 'separator'; label: string; key: string }
    | { kind: 'message'; message: Message; key: string; links: string[] };

  let channelMessages: Message[] = [];
  let messageBlocks: MessageBlock[] = [];

  function parseTimestampValue(timestamp: string | undefined): Date | null {
    if (!timestamp) return null;
    const parsed = Date.parse(timestamp);
    if (Number.isNaN(parsed)) return null;
    return new Date(parsed);
  }

  function dateKey(date: Date): string {
    return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
  }

  function formatDayHeading(date: Date): string {
    const today = new Date();
    const key = dateKey(date);
    const todayKey = dateKey(today);
    if (key === todayKey) return 'Today';
    const yesterday = new Date(today);
    yesterday.setDate(today.getDate() - 1);
    if (key === dateKey(yesterday)) return 'Yesterday';
    return date.toLocaleDateString(undefined, { year: 'numeric', month: 'long', day: 'numeric' });
  }

  function buildMessageBlocks(messages: Message[]): MessageBlock[] {
    const blocks: MessageBlock[] = [];
    let lastDateKey: string | null = null;

    for (let index = 0; index < messages.length; index += 1) {
      const message = messages[index];
      const timestamp = parseTimestampValue(message.timestamp);
      if (timestamp) {
        const currentKey = dateKey(timestamp);
        if (lastDateKey !== currentKey) {
          blocks.push({
            kind: 'separator',
            label: formatDayHeading(timestamp),
            key: `separator-${currentKey}-${message.id ?? index}`
          });
          lastDateKey = currentKey;
        }
      }

      const links = extractLinks(message.text);
      blocks.push({
        kind: 'message',
        message,
        key: `message-${message.id ?? `${index}-${message.time ?? ''}`}`,
        links
      });
    }

    return blocks;
  }

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
      const h = Math.min(messageInput.scrollHeight, MESSAGE_INPUT_MAX_HEIGHT);
      messageInput.style.height = h + 'px';
      inputScrollable = messageInput.scrollHeight > h;
    } else {
      inputScrollable = false;
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

  function ensureStatus(
    map: Record<string, UserStatus>,
    user: string,
    fallback: UserStatus = 'offline'
  ): UserStatus {
    return (map[user] ?? fallback) as UserStatus;
  }

  function toggleStatusMenu(event: MouseEvent) {
    event.stopPropagation();
    if (notificationMenuOpen) {
      notificationMenuOpen = false;
    }
    statusMenuOpen = !statusMenuOpen;
  }

  function selectStatus(value: UserStatus) {
    statuses.setSelf(value);
    statusMenuOpen = false;
  }

  function toggleNotificationMenu(event: MouseEvent) {
    event.stopPropagation();
    if (statusMenuOpen) {
      statusMenuOpen = false;
    }
    notificationMenuOpen = !notificationMenuOpen;
  }

  function selectNotificationPreference(value: ChannelNotificationPreference) {
    channelNotifications.setPreference(currentChatChannel, value);
    notificationMenuOpen = false;
  }

  function notificationButtonIcon(value: ChannelNotificationPreference): string {
    switch (value) {
      case 'mentions':
        return '@';
      case 'mute':
        return '🔕';
      default:
        return '🔔';
    }
  }

  function handleMenuOutside(event: MouseEvent) {
    const target = event.target as Node | null;
    if (statusMenuOpen) {
      if (statusMenuElement && target && statusMenuElement.contains(target)) return;
      if (statusMenuButton && target && statusMenuButton.contains(target)) return;
      statusMenuOpen = false;
    }
    if (notificationMenuOpen) {
      if (notificationMenuElement && target && notificationMenuElement.contains(target)) return;
      if (notificationMenuButton && target && notificationMenuButton.contains(target)) return;
      notificationMenuOpen = false;
    }
  }

  function handleStatusMenuKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      statusMenuOpen = false;
      event.stopPropagation();
      event.preventDefault();
    }
  }

  function handleNotificationMenuKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      notificationMenuOpen = false;
      event.stopPropagation();
      event.preventDefault();
    }
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

  $: currentUserStatus = $session.user
    ? ensureStatus(statusMap, $session.user, 'online')
    : 'offline';
  $: currentUserStatusLabel = STATUS_LABELS[currentUserStatus];
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
  let currentChatChannel = 'general';
  let currentVoiceChannel: string | null = null;
  let currentTopic = '';

  $: channelMessages = $chat.filter((m) => (m.channel ?? 'general') === currentChatChannel);
  $: messageBlocks = buildMessageBlocks(channelMessages);
  $: pinnedEntries = $pinned[currentChatChannel] ?? [];
  $: currentNotificationPreference = ($channelNotifications[currentChatChannel] ?? 'all') as ChannelNotificationPreference;
  $: notificationMenuLabel = (() => {
    const found = NOTIFICATION_OPTIONS.find((option) => option.value === currentNotificationPreference);
    return found ? found.label : 'All messages';
  })();

  $: if ($channels.length && !$channels.includes(currentChatChannel)) {
    currentChatChannel = $channels[0];
    chat.sendRaw({ type: 'join', channel: currentChatChannel });
  }

  $: currentTopic = $channelTopics[currentChatChannel] ?? '';


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
    if (currentVoiceChannel) {
      voice.leave(currentVoiceChannel);
    }
    ping.stop();
    roles.set({});
    if (highlightTimer) {
      clearTimeout(highlightTimer);
      highlightTimer = null;
    }
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
      const now = new Date();
      chat.sendRaw({
        type: 'chat',
        user: $session.user ?? 'anon',
        image: img,
        time: now.toLocaleTimeString(),
        timestamp: now.toISOString()
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
    statusMenuOpen = false;
    notificationMenuOpen = false;
    chat.clear();
    chat.sendRaw({ type: 'join', channel: ch });
    scrollBottom();
  }

  function joinVoice() {
    if ($session.user && currentVoiceChannel) {
      const info = $voiceChannels.find((vc) => vc.name === currentVoiceChannel);
      voice.join($session.user, currentVoiceChannel, info);
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
    const preset = promptVoicePreset();
    voiceChannels.create(name, preset);
    if ($session.user) {
      if (inVoice && currentVoiceChannel) {
        voice.leave(currentVoiceChannel);
      }
      currentVoiceChannel = name;
      voice.join($session.user, name, { name, quality: preset.quality, bitrate: preset.bitrate });
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
      const info = $voiceChannels.find((vc) => vc.name === ch);
      voice.join($session.user, ch, info);
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

  function editTopic() {
    const existing = $channelTopics[currentChatChannel] ?? '';
    const input = prompt('Set channel topic', existing);
    if (input === null) return;
    channelTopics.setTopic(currentChatChannel, input);
  }

  function canDeleteMessage(msg: Message): boolean {
    const current = $session.user;
    if (!current || typeof msg.id !== 'number') return false;
    if (msg.user === current) return true;
    return currentUserCanModerate;
  }

  function canPinMessage(msg: Message): boolean {
    return typeof msg.id === 'number';
  }

  function isMessagePinned(msg: Message): boolean {
    if (typeof msg.id !== 'number') return false;
    return pinned.isPinned(currentChatChannel, msg.id);
  }

  function togglePinMessage(msg: Message) {
    if (typeof msg.id !== 'number') return;
    if (isMessagePinned(msg)) {
      pinned.unpin(currentChatChannel, msg.id);
    } else {
      pinned.pin(currentChatChannel, msg);
    }
  }

  async function deleteChatMessage(msg: Message) {
    if (typeof msg.id !== 'number') return;
    const confirmation = await Promise.resolve(confirm('Delete this message?') as boolean | Promise<boolean>);
    if (!confirmation) return;
    chat.delete(msg.id);
  }

  function resolvePinnedMessage(entry: PinnedEntry): Message | undefined {
    return channelMessages.find((message) => message.id === entry.id);
  }

  function formatPinnedPreview(entry: PinnedEntry): string {
    const message = resolvePinnedMessage(entry);
    const base = message?.text ?? entry.text ?? '';
    const trimmed = base.trim();
    if (trimmed.length > 0) {
      return trimmed.length > PIN_PREVIEW_LIMIT ? `${trimmed.slice(0, PIN_PREVIEW_LIMIT)}…` : trimmed;
    }
    if (message?.image || entry.image) {
      return 'Image attachment';
    }
    return 'Message';
  }

  function pinnedAuthor(entry: PinnedEntry): string {
    const message = resolvePinnedMessage(entry);
    return message?.user ?? entry.user ?? 'Unknown';
  }

  function pinnedTimestamp(entry: PinnedEntry): string {
    const source = resolvePinnedMessage(entry)?.timestamp ?? entry.timestamp ?? entry.pinnedAt;
    if (!source) return '';
    const parsed = Date.parse(source);
    if (Number.isNaN(parsed)) return '';
    return new Date(parsed).toLocaleString();
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
    chat.loadHistory(currentChatChannel, messageId + 1, 200);
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
              currentVoiceChannel = channels[0].name;
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
    ...(menuVoiceChannel
      ? [
          ...VOICE_QUALITY_PRESETS.map((preset) => ({
            label:
              preset.bitrate && preset.bitrate > 0
                ? `Set Voice Quality: ${preset.label} (${Math.round(preset.bitrate / 1000)} kbps)`
                : `Set Voice Quality: ${preset.label}`,
            action: () =>
              voiceChannels.configure(menuVoiceChannel!, {
                quality: preset.quality,
                bitrate: preset.bitrate
              })
          })),
          { label: 'Delete Voice Channel', action: () => voiceChannels.remove(menuVoiceChannel!) }
        ]
      : [])
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

  const handleHistory = async () => {
    await tick();
    if (messagesContainer) {
      messagesContainer.scrollTop = messagesContainer.scrollHeight - prevHeight;
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
    const channelName = (event.channel as string | undefined) ?? currentChatChannel;
    if (typeof messageId !== 'number') return;
    pinned.removeMessage(channelName, messageId);
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
      const filteredLength = $chat.filter((m) => (m.channel ?? 'general') === currentChatChannel).length;
      if (filteredLength !== lastLength) {
        lastLength = filteredLength;
        if (!loadingHistory && !handledPending) {
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
    window.addEventListener('click', handleMenuOutside);
  });

  onDestroy(() => {
    window.removeEventListener('mousemove', handleMouseMove);
    window.removeEventListener('mouseup', stopResize);
    window.removeEventListener('keydown', handleGlobalShortcut);
    window.removeEventListener('click', handleMenuOutside);
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
      {#each $voiceChannels as ch (ch.name)}
        <div class="voice-group">
          <button on:click={() => joinVoiceChannel(ch.name)} on:contextmenu={(e) => openChannelMenu(e, ch.name, true)}>
            <span class="chan-icon">🔊</span>
            <span class="voice-channel-name">{ch.name}</span>
            <span class="voice-channel-quality">{formatVoiceQuality(ch)}</span>
          </button>
          {#if $voiceUsers[ch.name]?.length}
            <ul class="voice-user-list">
              {#each $voiceUsers[ch.name] as user}
                <li
                  on:contextmenu={(e) => user !== $session.user && openUserVolumeMenu(e, user)}
                  class:clickable={user !== $session.user}
                  class:talking={
                    user === $session.user
                      ? !$microphoneMuted && $voiceActivity
                      : Boolean($remoteSpeaking[user])
                  }
                >
                  <span
                    class="status voice"
                    class:talking={
                      user === $session.user
                        ? !$microphoneMuted && $voiceActivity
                        : Boolean($remoteSpeaking[user])
                    }
                  ></span>
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

      <div class="voice-controls-container">
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
      </div>
    </div>
    <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
    <div class="resizer" role="separator" aria-label="Resize channel list" on:mousedown={startLeftResize}></div>
    <div class="chat">
      <div class="header">
        <div class="title">
          <h1>{currentChatChannel}</h1>
          {#if currentTopic}
            <p class="topic" title={currentTopic}>{currentTopic}</p>
          {:else}
            <p class="topic empty">No topic set</p>
          {/if}
        </div>
        <div class="actions">
          <div class="user">{$session.user}</div>
          <div class="status-control">
            <button
              class="action-button status-button"
              bind:this={statusMenuButton}
              aria-haspopup="true"
              aria-expanded={statusMenuOpen}
              on:click={toggleStatusMenu}
              title={`Set status (${currentUserStatusLabel})`}
            >
              <span class={`status ${currentUserStatus}`}></span>
              <span class="status-button-label">{currentUserStatusLabel}</span>
              <svg
                width="12"
                height="12"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
                aria-hidden="true"
              >
                <polyline points="6 9 12 15 18 9"></polyline>
              </svg>
            </button>
            {#if statusMenuOpen}
              <div
                class="status-menu"
                bind:this={statusMenuElement}
                role="menu"
                tabindex="-1"
                on:click|stopPropagation
                on:keydown={handleStatusMenuKeydown}
              >
                {#each statusOptions as option}
                  <button
                    class:active={option.value === currentUserStatus}
                    on:click={() => selectStatus(option.value)}
                    role="menuitemradio"
                    aria-checked={option.value === currentUserStatus}
                  >
                    <span class={`status ${option.value}`}></span>
                    <span class="status-option-label">{option.label}</span>
                    <span class="status-option-emoji" aria-hidden="true">{option.emoji}</span>
                  </button>
                {/each}
              </div>
            {/if}
          </div>
          <div class="connection-info">
            <PingDot ping={$ping} />
            <ConnectionBars strength={serverStrength} />
          </div>
          <button
            class="action-button"
            on:click={() => theme.toggle()}
            title={$theme === 'dark' ? 'Switch to light theme' : 'Switch to dark theme'}
            aria-pressed={$theme === 'light'}
          >
            {#if $theme === 'dark'}
              <svg
                width="20"
                height="20"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
                aria-hidden="true"
              >
                <circle cx="12" cy="12" r="4" />
                <path d="M12 2v2" />
                <path d="M12 20v2" />
                <path d="m4.93 4.93 1.41 1.41" />
                <path d="m17.66 17.66 1.41 1.41" />
                <path d="M2 12h2" />
                <path d="M20 12h2" />
                <path d="m6.34 17.66-1.41 1.41" />
                <path d="m19.07 4.93-1.41 1.41" />
              </svg>
              <span class="sr-only">Switch to light theme</span>
            {:else}
              <svg
                width="20"
                height="20"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
                aria-hidden="true"
              >
                <path d="M20.985 12.486a9 9 0 1 1-9.473-9.472c.405-.022.617.46.402.803a6 6 0 0 0 8.268 8.268c.344-.215.825-.004.803.401" />
              </svg>
              <span class="sr-only">Switch to dark theme</span>
            {/if}
          </button>
          <button class="action-button" on:click={editTopic} title="Edit channel topic">
            <svg
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <path
                d="M21.174 6.812a1 1 0 0 0-3.986-3.987L3.842 16.174a2 2 0 0 0-.5.83l-1.321 4.352a.5.5 0 0 0 .623.622l4.353-1.32a2 2 0 0 0 .83-.497z"
              />
              <path d="m15 5 4 4" />
            </svg>
            <span class="sr-only">Edit channel topic</span>
          </button>
          <button
            class="action-button focus-toggle"
            class:focusActive={$focusMode}
            aria-pressed={$focusMode}
            on:click={toggleFocusMode}
            title={$focusMode ? 'Exit focus mode' : 'Enter focus mode'}
          >
            {#if $focusMode}
              <svg
                width="20"
                height="20"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
                aria-hidden="true"
              >
                <path d="M15 3h6v6" />
                <path d="m21 3-7 7" />
                <path d="m3 21 7-7" />
                <path d="M9 21H3v-6" />
              </svg>
              <span>Restore</span>
            {:else}
              <svg
                width="20"
                height="20"
                viewBox="0 0 24 24"
                fill="none"
                stroke="currentColor"
                stroke-width="1.8"
                stroke-linecap="round"
                stroke-linejoin="round"
                aria-hidden="true"
              >
                <circle cx="12" cy="12" r="3" />
                <path d="M3 7V5a2 2 0 0 1 2-2h2" />
                <path d="M17 3h2a2 2 0 0 1 2 2v2" />
                <path d="M21 17v2a2 2 0 0 1-2 2h-2" />
                <path d="M7 21H5a2 2 0 0 1-2-2v-2" />
              </svg>
              <span>Focus</span>
            {/if}
          </button>
          <div class="notification-control">
            <button
              class="action-button"
              bind:this={notificationMenuButton}
              aria-haspopup="true"
              aria-expanded={notificationMenuOpen}
              on:click={toggleNotificationMenu}
              title={`Channel notifications: ${notificationMenuLabel}`}
            >
              <span class="notification-icon">{notificationButtonIcon(currentNotificationPreference)}</span>
              <span class="sr-only">Configure channel notifications</span>
            </button>
            {#if notificationMenuOpen}
              <div
                class="notification-menu"
                bind:this={notificationMenuElement}
                role="menu"
                tabindex="-1"
                on:click|stopPropagation
                on:keydown={handleNotificationMenuKeydown}
              >
                {#each NOTIFICATION_OPTIONS as option}
                  <button
                    class:active={option.value === currentNotificationPreference}
                    on:click={() => selectNotificationPreference(option.value)}
                    role="menuitemradio"
                    aria-checked={option.value === currentNotificationPreference}
                  >
                    <span class="notification-option-icon" aria-hidden="true">{option.icon}</span>
                    <span class="notification-option-text">
                      <span class="label">{option.label}</span>
                      <span class="description">{option.description}</span>
                    </span>
                  </button>
                {/each}
              </div>
            {/if}
          </div>
          <button class="action-button" on:click={openSettings} title="Settings">
            <svg
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <path
                d="M9.671 4.136a2.34 2.34 0 0 1 4.659 0 2.34 2.34 0 0 0 3.319 1.915 2.34 2.34 0 0 1 2.33 4.033 2.34 2.34 0 0 0 0 3.831 2.34 2.34 0 0 1-2.33 4.033 2.34 2.34 0 0 0-3.319 1.915 2.34 2.34 0 0 1-4.659 0 2.34 2.34 0 0 0-3.32-1.915 2.34 2.34 0 0 1-2.33-4.033 2.34 2.34 0 0 0 0-3.831A2.34 2.34 0 0 1 6.35 6.051a2.34 2.34 0 0 0 3.319-1.915"
              />
              <circle cx="12" cy="12" r="3" />
            </svg>
            <span class="sr-only">Open settings</span>
          </button>
          <button class="action-button" on:click={leaveServer} title="Leave Server">
            <svg
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <path d="m16 17 5-5-5-5" />
              <path d="M21 12H9" />
              <path d="M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4" />
            </svg>
            <span class="sr-only">Leave server</span>
          </button>
          <button class="action-button danger" on:click={logout} title="Logout">
            <svg
              width="20"
              height="20"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.8"
              stroke-linecap="round"
              stroke-linejoin="round"
              aria-hidden="true"
            >
              <path d="M12 2v10" />
              <path d="M18.4 6.6a9 9 0 1 1-12.77.04" />
            </svg>
            <span class="sr-only">Sign out</span>
          </button>
        </div>
      </div>
      <SettingsModal open={settingsOpen} close={closeSettings} />
      {#if pinnedEntries.length > 0}
        <div class="pinned-bar" role="region" aria-label="Pinned messages">
          <div class="pinned-header">
            <span class="pinned-title">Pinned</span>
            <span class="pinned-count">{pinnedEntries.length}</span>
          </div>
          <ul class="pinned-list">
            {#each pinnedEntries as entry (entry.id)}
              <li class="pinned-item">
                <button class="pinned-preview" on:click={() => focusMessage(entry.id)}>
                  <span class="pinned-author">{pinnedAuthor(entry)}</span>
                  <span class="pinned-text">{formatPinnedPreview(entry)}</span>
                  {#if pinnedTimestamp(entry)}
                    <span class="pinned-timestamp">{pinnedTimestamp(entry)}</span>
                  {/if}
                </button>
                <button
                  class="pinned-remove"
                  on:click={() => pinned.unpin(currentChatChannel, entry.id)}
                  aria-label="Unpin message"
                >
                  ✕
                </button>
              </li>
            {/each}
          </ul>
        </div>
      {/if}
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
                  </span>
                  {#if typeof block.message.id === 'number' && (canPinMessage(block.message) || canDeleteMessage(block.message))}
                    <div class="message-actions">
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
        <textarea
          class:scrollable={inputScrollable}
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
            <span class={`status ${ensureStatus(statusMap, user, 'online')}`}></span>
            <span class="status-label">{STATUS_LABELS[ensureStatus(statusMap, user, 'online')]}</span>
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
            <span class={`status ${ensureStatus(statusMap, user)}`}></span>
            <span class="status-label">{STATUS_LABELS[ensureStatus(statusMap, user)]}</span>
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
    padding: clamp(1.25rem, 2.5vw, 1.75rem);
    gap: clamp(0.75rem, 2vw, 1rem);
    backdrop-filter: blur(0.5px);
  }

  .page.focus {
    padding-inline: clamp(1.5rem, 4vw, 3rem);
  }

  .page.focus .channels,
  .page.focus .sidebar,
  .page.focus .resizer {
    display: none;
  }

  .page.focus .chat {
    max-width: 1080px;
    margin: 0 auto;
  }

  .channels {
    width: clamp(220px, 18vw, 260px);
    background: color-mix(in srgb, var(--color-surface-elevated) 86%, transparent);
    border-radius: var(--radius-lg);
    border: 1px solid var(--color-surface-outline);
    box-shadow: var(--shadow-xs);
    padding: clamp(1rem, 2vw, 1.25rem);
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .channels .section {
    margin: 0.75rem 0 0.35rem 0;
    font-size: 0.72rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.16em;
    color: var(--color-muted);
  }

  .channels button {
    width: 100%;
    padding: 0.6rem 0.9rem;
    border: 1px solid transparent;
    background: transparent;
    color: var(--color-muted);
    text-align: left;
    border-radius: var(--radius-sm);
    font-weight: 600;
    letter-spacing: 0.01em;
    display: flex;
    align-items: center;
    gap: 0.55rem;
    position: relative;
  }

  .voice-channel-name {
    flex: 1;
  }

  .voice-channel-quality {
    margin-left: auto;
    font-size: 0.68rem;
    font-weight: 600;
    color: color-mix(in srgb, var(--color-muted) 85%, transparent);
  }

  .channels button:hover {
    background: color-mix(in srgb, var(--color-primary) 8%, transparent);
    color: var(--color-on-surface);
  }

  .channels button.active {
    background: color-mix(in srgb, var(--color-primary) 18%, transparent);
    color: var(--color-on-surface);
    border-color: color-mix(in srgb, var(--color-primary) 30%, transparent);
    box-shadow: inset 0 0 0 1px color-mix(in srgb, var(--color-primary) 24%, transparent);
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
    border-radius: 999px;
    background: color-mix(in srgb, var(--color-on-surface) 12%, transparent);
    transform: translateX(-50%);
  }

  .chat {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
    min-width: 0;
  }

  .header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: clamp(1rem, 2vw, 1.35rem) clamp(1rem, 3vw, 1.5rem);
    border-radius: var(--radius-lg);
    background: linear-gradient(
      135deg,
      color-mix(in srgb, var(--color-primary) 18%, var(--color-surface-raised)),
      color-mix(in srgb, var(--color-tertiary) 12%, var(--color-surface-raised))
    );
    border: 1px solid color-mix(in srgb, var(--color-primary) 20%, transparent);
    box-shadow: var(--shadow-sm);
    gap: 1rem;
    flex-wrap: wrap;
  }

  .title {
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    min-width: 0;
  }

  .title h1 {
    margin: 0;
    font-size: clamp(1.25rem, 2.5vw, 1.7rem);
    letter-spacing: -0.01em;
  }

  .topic {
    margin: 0;
    font-size: 0.92rem;
    color: color-mix(in srgb, var(--color-on-primary) 82%, transparent);
    max-width: min(40rem, 60vw);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .topic.empty {
    font-style: italic;
    opacity: 0.7;
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }

  .user {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.45rem 0.9rem;
    border-radius: 999px;
    background: color-mix(in srgb, var(--color-on-primary) 15%, transparent);
    color: var(--color-on-primary);
    font-weight: 600;
    letter-spacing: 0.01em;
  }

  .user::before {
    content: '🧑‍🚀';
  }

  .status-control {
    position: relative;
  }

  .status-button {
    width: auto;
    height: auto;
    min-width: 0;
    padding: 0.4rem 0.85rem;
    gap: 0.45rem;
    font-weight: 600;
    font-size: 0.85rem;
    justify-content: flex-start;
    white-space: nowrap;
  }

  .status-button svg {
    margin-left: 0.35rem;
  }

  .status-button-label {
    text-transform: capitalize;
  }

  .status-menu {
    position: absolute;
    top: calc(100% + 0.4rem);
    right: 0;
    min-width: 12rem;
    background: color-mix(in srgb, var(--color-surface-elevated) 95%, transparent);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-lg);
    padding: 0.35rem;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    z-index: 60;
  }

  .status-menu button {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    border: none;
    background: transparent;
    color: inherit;
    border-radius: var(--radius-sm);
    padding: 0.45rem 0.6rem;
    text-align: left;
    font-size: 0.9rem;
    cursor: pointer;
  }

  .status-menu button:hover,
  .status-menu button:focus-visible,
  .status-menu button.active {
    background: color-mix(in srgb, var(--color-primary) 12%, transparent);
    outline: none;
  }

  .status-menu .status {
    width: 0.6rem;
    height: 0.6rem;
  }

  .status-menu button.active {
    font-weight: 600;
  }

  .status-option-label {
    flex: 1;
    text-transform: capitalize;
  }

  .status-option-emoji {
    font-size: 1rem;
  }

  .connection-info {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    padding: 0.4rem 0.8rem;
    border-radius: 999px;
    background: color-mix(in srgb, var(--color-on-primary) 12%, transparent);
  }

  .action-button {
    min-width: 2.5rem;
    width: auto;
    min-height: 2.5rem;
    height: auto;
    border-radius: 0.85rem;
    border: 1px solid color-mix(in srgb, var(--color-outline-strong) 70%, transparent);
    background: color-mix(in srgb, var(--color-surface-elevated) 82%, transparent);
    color: color-mix(in srgb, var(--color-on-surface) 92%, var(--color-muted) 8%);
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
    padding: 0.55rem;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.04);
  }

  .action-button svg {
    width: 1.1rem;
    height: 1.1rem;
  }

  .action-button:hover {
    border-color: color-mix(in srgb, var(--color-outline-strong) 90%, transparent);
    transform: translateY(-1px);
    box-shadow: var(--shadow-xs);
  }

  .action-button.danger {
    color: var(--color-error);
    border-color: color-mix(in srgb, var(--color-error) 40%, transparent);
    background: color-mix(in srgb, var(--color-error) 12%, transparent);
  }

  .action-button.danger:hover {
    border-color: color-mix(in srgb, var(--color-error) 55%, transparent);
    background: color-mix(in srgb, var(--color-error) 18%, transparent);
  }

  .action-button.focus-toggle {
    padding-inline: 0.9rem;
    width: auto;
    font-size: 0.9rem;
  }

  .action-button.focus-toggle svg {
    width: 1.05rem;
    height: 1.05rem;
  }

  .action-button.focus-toggle span {
    font-weight: 600;
    font-size: 0.85rem;
  }

  .action-button.focus-toggle.focusActive {
    background: color-mix(in srgb, var(--color-secondary) 20%, var(--color-surface-elevated) 80%);
    color: var(--color-on-surface);
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
    font-size: 0.75rem;
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
    border-radius: 999px;
    background: color-mix(in srgb, var(--color-surface-elevated) 78%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-primary) 18%, transparent);
    color: var(--color-muted);
    font-weight: 600;
  }

  .message {
    display: grid;
    grid-template-columns: auto auto 1fr;
    gap: 0.75rem;
    align-items: start;
    padding: 0.85rem 1rem;
    border-radius: var(--radius-md);
    background: color-mix(in srgb, var(--color-primary) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-primary) 12%, transparent);
    transition: transform var(--transition), box-shadow var(--transition);
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
    font-size: 0.72rem;
    color: var(--color-muted);
    font-family: 'JetBrains Mono', 'Fira Code', monospace;
    opacity: 0.7;
  }

  .message .username {
    font-weight: 600;
    color: var(--color-on-surface);
  }

  .message .role {
    font-size: 0.75rem;
    font-weight: 600;
    align-self: center;
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
    font-size: 0.85rem;
    transition: background var(--transition), border-color var(--transition), transform var(--transition);
  }

  .message-action:hover,
  .message-action.active {
    background: color-mix(in srgb, var(--color-primary) 18%, transparent);
    border-color: color-mix(in srgb, var(--color-primary) 36%, transparent);
    transform: translateY(-1px);
  }

  .message-action.danger {
    color: color-mix(in srgb, #ef4444 80%, var(--color-on-surface));
  }

  .message-action.danger:hover {
    background: color-mix(in srgb, #ef4444 18%, transparent);
    border-color: color-mix(in srgb, #ef4444 32%, transparent);
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

  .notification-control {
    position: relative;
  }

  .notification-icon {
    font-size: 1.1rem;
    line-height: 1;
  }

  .notification-menu {
    position: absolute;
    top: calc(100% + 0.4rem);
    right: 0;
    min-width: 16rem;
    padding: 0.5rem;
    border-radius: var(--radius-md);
    border: 1px solid color-mix(in srgb, var(--color-primary) 16%, transparent);
    background: color-mix(in srgb, var(--color-surface-elevated) 95%, transparent);
    box-shadow: var(--shadow-lg);
    display: flex;
    flex-direction: column;
    gap: 0.35rem;
    z-index: 80;
  }

  .notification-menu button {
    display: flex;
    gap: 0.65rem;
    align-items: center;
    padding: 0.5rem 0.65rem;
    border-radius: var(--radius-sm);
    border: none;
    background: transparent;
    color: var(--color-on-surface);
    cursor: pointer;
    text-align: left;
    transition: background var(--transition), transform var(--transition);
  }

  .notification-menu button:hover,
  .notification-menu button.active {
    background: color-mix(in srgb, var(--color-primary) 15%, transparent);
    transform: translateY(-1px);
  }

  .notification-option-icon {
    font-size: 1rem;
  }

  .notification-option-text {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
  }

  .notification-option-text .label {
    font-weight: 600;
  }

  .notification-option-text .description {
    font-size: 0.82rem;
    color: color-mix(in srgb, var(--color-on-surface) 70%, transparent);
  }

  .pinned-bar {
    display: flex;
    flex-direction: column;
    gap: 0.6rem;
    margin-bottom: 1rem;
    padding: 0.85rem 1rem;
    border-radius: var(--radius-lg);
    border: 1px solid color-mix(in srgb, var(--color-primary) 18%, transparent);
    background: color-mix(in srgb, var(--color-surface-elevated) 90%, transparent);
  }

  .pinned-header {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-weight: 600;
    color: var(--color-on-surface);
  }

  .pinned-title {
    font-size: 0.95rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
  }

  .pinned-count {
    font-size: 0.75rem;
    padding: 0.1rem 0.6rem;
    border-radius: 999px;
    background: color-mix(in srgb, var(--color-primary) 16%, transparent);
    color: var(--color-on-surface);
  }

  .pinned-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .pinned-item {
    display: flex;
    gap: 0.5rem;
    align-items: stretch;
  }

  .pinned-preview {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    text-align: left;
    border-radius: var(--radius-md);
    border: 1px solid color-mix(in srgb, var(--color-primary) 16%, transparent);
    background: color-mix(in srgb, var(--color-surface-elevated) 88%, transparent);
    padding: 0.6rem 0.75rem;
    color: var(--color-on-surface);
    cursor: pointer;
    transition: background var(--transition), border-color var(--transition), transform var(--transition);
  }

  .pinned-preview:hover {
    background: color-mix(in srgb, var(--color-primary) 14%, transparent);
    border-color: color-mix(in srgb, var(--color-primary) 28%, transparent);
    transform: translateY(-1px);
  }

  .pinned-author {
    font-weight: 600;
    font-size: 0.9rem;
  }

  .pinned-text {
    font-size: 0.88rem;
    color: color-mix(in srgb, var(--color-on-surface) 88%, transparent);
    word-break: break-word;
  }

  .pinned-timestamp {
    font-size: 0.75rem;
    color: var(--color-muted);
  }

  .pinned-remove {
    border: none;
    border-radius: 999px;
    background: color-mix(in srgb, var(--color-primary) 12%, transparent);
    color: var(--color-on-surface);
    width: 30px;
    height: 30px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    transition: background var(--transition), transform var(--transition);
  }

  .pinned-remove:hover {
    background: color-mix(in srgb, #ef4444 22%, transparent);
    transform: translateY(-1px);
  }

  .pinned-remove:focus-visible {
    outline: 2px solid color-mix(in srgb, var(--color-primary) 40%, transparent);
    outline-offset: 2px;
  }

  .reactions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
    margin-top: 0.35rem;
  }

  .reaction-chip {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
    border-radius: 999px;
    padding: 0.3rem 0.65rem;
    font-size: 0.82rem;
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
    font-size: 0.78rem;
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

  .voice-controls-container {
    margin-top: auto;
    padding-top: 1rem;
    border-top: 1px solid var(--color-surface-outline);
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .voice-controls-panel {
    border-radius: var(--radius-lg);
    padding: 1rem;
    background: color-mix(in srgb, var(--color-surface-raised) 86%, transparent);
    border: 1px solid var(--color-surface-outline);
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }

  .voice-controls-header {
    font-size: 0.85rem;
    font-weight: 600;
    color: var(--color-muted);
    letter-spacing: 0.08em;
    text-transform: uppercase;
  }

  .voice-controls-buttons {
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
  }

  .voice-control-btn {
    display: inline-flex;
    align-items: center;
    gap: 0.5rem;
    padding: 0.7rem 0.85rem;
    border-radius: var(--radius-sm);
    border: 1px solid color-mix(in srgb, var(--color-primary) 16%, transparent);
    background: color-mix(in srgb, var(--color-primary) 8%, transparent);
    color: var(--color-on-surface);
    font-weight: 600;
    width: 100%;
  }

  .voice-control-btn:hover {
    border-color: color-mix(in srgb, var(--color-primary) 32%, transparent);
  }

  .voice-control-btn.leave {
    border-color: color-mix(in srgb, var(--color-error) 45%, transparent);
    color: var(--color-error);
    background: color-mix(in srgb, var(--color-error) 12%, transparent);
  }

  .voice-control-btn.leave:hover {
    border-color: color-mix(in srgb, var(--color-error) 60%, transparent);
  }

  .voice-control-btn.mute.muted {
    background: color-mix(in srgb, var(--color-error) 15%, transparent);
    border-color: color-mix(in srgb, var(--color-error) 45%, transparent);
    color: var(--color-error);
  }

  .voice-control-btn.mute.active,
  .voice-control-btn.mute.ptt-active {
    background: color-mix(in srgb, var(--color-success) 18%, transparent);
    border-color: color-mix(in srgb, var(--color-success) 40%, transparent);
    color: var(--color-success);
  }

  .sidebar {
    width: clamp(240px, 18vw, 280px);
    background: color-mix(in srgb, var(--color-surface-elevated) 84%, transparent);
    border-radius: var(--radius-lg);
    border: 1px solid var(--color-surface-outline);
    box-shadow: var(--shadow-xs);
    padding: clamp(1rem, 2vw, 1.3rem);
    display: flex;
    flex-direction: column;
    gap: 0.85rem;
    min-width: 0;
  }

  .sidebar h2 {
    margin: 0;
    font-size: 1.25rem;
  }

  .sidebar h3 {
    margin: 0;
    font-size: 0.85rem;
    text-transform: uppercase;
    letter-spacing: 0.12em;
    color: var(--color-muted);
  }

  .sidebar ul {
    list-style: none;
    padding: 0;
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }

  .sidebar li {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    padding: 0.4rem 0.55rem;
    border-radius: var(--radius-sm);
  }

  .sidebar li:hover {
    background: color-mix(in srgb, var(--color-primary) 8%, transparent);
  }

  .status {
    width: 0.75rem;
    height: 0.75rem;
    border-radius: 50%;
    flex-shrink: 0;
  }

  .status.online {
    background: var(--color-success);
  }

  .status.away {
    background: #fbbf24;
  }

  .status.busy {
    background: #ef4444;
  }

  .status.offline {
    background: color-mix(in srgb, var(--color-muted) 40%, transparent);
  }

  .status-label {
    font-size: 0.75rem;
    color: var(--color-muted);
    text-transform: capitalize;
    min-width: 3.5rem;
  }

  .sidebar .status-label {
    text-transform: capitalize;
  }

  .volume-menu-overlay {
    position: fixed;
    inset: 0;
    background: var(--color-overlay);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 40;
  }

  .volume-menu {
    position: absolute;
    width: min(320px, 90vw);
    background: color-mix(in srgb, var(--color-surface-elevated) 88%, transparent);
    border-radius: var(--radius-lg);
    border: 1px solid var(--color-surface-outline);
    box-shadow: var(--shadow-md);
    padding: 1.25rem;
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }

  .volume-menu-header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: 0.5rem;
  }

  .volume-menu-user {
    font-weight: 600;
  }

  .volume-menu-title {
    font-size: 0.85rem;
    color: var(--color-muted);
  }

  .volume-menu-content {
    display: grid;
    gap: 1.1rem;
  }

  .volume-control-row {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .volume-icon {
    font-size: 1.2rem;
  }

  .volume-menu-slider {
    flex: 1;
    -webkit-appearance: none;
    appearance: none;
    height: 0.45rem;
    border-radius: 999px;
    background: color-mix(in srgb, var(--color-surface-raised) 88%, transparent);
    border: 1px solid color-mix(in srgb, var(--color-primary) 18%, transparent);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.12);
    outline: none;
    transition: border-color var(--transition), box-shadow var(--transition);
  }

  .volume-menu-slider:focus {
    border-color: color-mix(in srgb, var(--color-secondary) 32%, transparent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--color-secondary) 18%, transparent);
  }

  .volume-menu-slider::-webkit-slider-runnable-track {
    height: 0.45rem;
    border-radius: 999px;
    background: linear-gradient(
      90deg,
      color-mix(in srgb, var(--color-primary) 45%, var(--color-surface-elevated) 55%) 0%,
      color-mix(in srgb, var(--color-primary) 18%, var(--color-surface-elevated) 82%) 100%
    );
  }

  .volume-menu-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--color-surface);
    border: 2px solid color-mix(in srgb, var(--color-secondary) 60%, transparent);
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.15);
    margin-top: -7px;
  }

  .volume-menu-slider::-moz-range-track {
    height: 0.45rem;
    border-radius: 999px;
    background: linear-gradient(
      90deg,
      color-mix(in srgb, var(--color-primary) 45%, var(--color-surface-elevated) 55%) 0%,
      color-mix(in srgb, var(--color-primary) 18%, var(--color-surface-elevated) 82%) 100%
    );
    border: none;
  }

  .volume-menu-slider::-moz-range-progress {
    height: 0.45rem;
    border-radius: 999px;
    background: linear-gradient(
      90deg,
      color-mix(in srgb, var(--color-primary) 55%, var(--color-surface-elevated) 45%) 0%,
      color-mix(in srgb, var(--color-primary) 24%, var(--color-surface-elevated) 76%) 100%
    );
  }

  .volume-menu-slider::-moz-range-thumb {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--color-surface);
    border: 2px solid color-mix(in srgb, var(--color-secondary) 60%, transparent);
    box-shadow: 0 2px 4px rgba(0, 0, 0, 0.15);
  }

  .volume-percentage {
    font-size: 0.82rem;
    color: var(--color-muted);
  }

  .volume-presets {
    display: flex;
    gap: 0.5rem;
  }

  .preset-btn {
    flex: 1;
    padding: 0.55rem 0.75rem;
    border-radius: var(--radius-sm);
    border: 1px solid color-mix(in srgb, var(--color-primary) 18%, transparent);
    background: color-mix(in srgb, var(--color-primary) 10%, transparent);
    color: var(--color-secondary);
  }

  .preset-btn:hover {
    border-color: color-mix(in srgb, var(--color-primary) 28%, transparent);
  }

  .sr-only {
    border: 0;
    clip: rect(0 0 0 0);
    height: 1px;
    margin: -1px;
    overflow: hidden;
    padding: 0;
    position: absolute;
    width: 1px;
  }

  @media (max-width: 1280px) {
    .page {
      flex-direction: column;
      height: auto;
      min-height: 100vh;
    }

    .channels,
    .sidebar {
      width: 100%;
      order: 0;
    }

    .resizer {
      display: none;
    }

    .chat {
      order: 1;
    }

    .sidebar {
      order: 2;
    }
  }

  @media (max-width: 768px) {
    .page {
      padding: clamp(1rem, 4vw, 1.5rem);
    }

    .header {
      flex-direction: column;
      align-items: stretch;
    }

    .actions {
      justify-content: flex-start;
    }

    .input-row {
      grid-template-columns: 1fr;
    }

    .controls {
      justify-content: flex-end;
    }
  }
</style>

