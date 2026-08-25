<!--
  Primary chat surface. Handles WebSocket lifecycle, message rendering, voice
  channel state and peripheral UI such as sidebars and context menus. The
  module coordinates many Svelte stores to keep the interface reactive.
-->
<script lang="ts">
  import { onMount, onDestroy, tick } from 'svelte';
  import { chat } from '$lib/stores/chat';
  import { roles, userRoleIds } from '$lib/stores/roles';
  import { roleDefinitions } from '$lib/stores/roleDefinitions';
  import { channelOverrides } from '$lib/stores/channelOverrides';
  import { canSpeak, resetVoicePermissions } from '$lib/stores/voicePermissions';
  import { can, myTopPosition, myPermissions } from '$lib/stores/permissions';
  import { PERMISSIONS, hasPermission, computeTopPosition } from '$lib/chat/permissions';
  import { session } from '$lib/stores/session';
  import { uploadForm, uploadErrorMessage } from '$lib/upload';
  import { displayNames, profiles } from '$lib/stores/profiles';
  import { voice, voiceVideo } from '$lib/stores/voice';
  import { selectedServer, servers } from '$lib/stores/servers';
  import { onlineUsers } from '$lib/stores/online';
  import { offlineUsers } from '$lib/stores/users';
  import { volume, outputDeviceId, outputMuted, microphoneMuted, userVolumes } from '$lib/stores/settings';
  import { setSpeaking, SPEAKING_RMS_THRESHOLD } from '$lib/stores/voiceSpeaking';
  import { getAudioContext, resumeAudioContext } from '$lib/voice/audioContext';
  import { subscribeTick } from '$lib/voice/ticker';
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
  import type { CategoryInfo, ChannelInfo, ContextMenuItem, ForwardInfo } from '$lib/types';
  import { forwardOptions, forwardedDmText, parseForwardTarget } from '$lib/chat/forward';
  import {
    parseWhen,
    splitWhen,
    scheduleBoundsError,
    reminderTextError,
    describeWhen
  } from '$lib/chat/schedule';
  import { leftSidebarWidth, rightSidebarWidth } from '$lib/stores/layout';
  import { channelTopics } from '$lib/stores/channelTopics';
  import { statuses, STATUS_LABELS, USER_STATUS_VALUES } from '$lib/stores/status';
  import { pinned } from '$lib/stores/pins';
  import type { PinnedEntry } from '$lib/stores/pins';
  import { scheduledAttention } from '$lib/stores/scheduled';
  import { typing } from '$lib/stores/typing';
  import { unread } from '$lib/stores/unread';
  import { threadData } from '$lib/stores/thread';
  import { dm } from '$lib/stores/dm';
  import { channelDraft, dmDraft, drafts, threadDraft } from '$lib/stores/drafts';
  import { peerKeys } from '$lib/stores/peerKeys';
  import { channelKeys } from '$lib/stores/channelKeys';
  import { dmFingerprint } from '$lib/dm-crypto';
  import {
    screenSharePeers,
    watchedScreenShares,
    openScreenShare,
    closeScreenShare,
    closeAllScreenShares,
    leaveScreenShareAsViewer,
    stopScreenShare,
    localScreenShareStream,
    screenSharePreview,
    toggleScreenSharePreview
  } from '$lib/stores/screenShare';
  import ScreenShareLayer from '$lib/components/ScreenShareLayer.svelte';
  import {
    activeWebcams,
    localCameraStream,
    stopCamera
  } from '$lib/stores/webcam';
  import WebcamStage from '$lib/components/WebcamStage.svelte';
  import { loadKeyPair, sign } from '$lib/keypair';
  import { httpBaseFromWs } from '$lib/server-url';
  import { connection, connectionError } from '$lib/stores/connection';
  import {
    uploadConfig,
    describeUploadRejection,
    formatUploadSize
  } from '$lib/stores/uploadConfig';
  import { voiceDefaults } from '$lib/stores/voiceDefaults';
  import { slowModeWait } from '$lib/stores/chatSettings';
  import { describeServerError, isFatalConnectionError } from '$lib/errors';
  import type { Message, UserStatus, WatchedScreenShare, WebcamTile } from '$lib/types';
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
    MAX_TOPIC_LENGTH,
    MIN_EPHEMERAL_SECONDS,
    MAX_EPHEMERAL_SECONDS,
    VOICE_QUALITY_PRESETS,
    DEFAULT_VOICE_PRESET,
    DEFAULT_CHANNEL_NAME,
    MAX_NICKNAME_LENGTH,
    MAX_REMINDER_TEXT_LENGTH
  } from '$lib/chat/constants';
  import ServerDashboardModal from '$lib/components/ServerDashboardModal.svelte';
  import ChannelPermissionsModal from '$lib/components/ChannelPermissionsModal.svelte';
  import UserStatsModal from '$lib/components/UserStatsModal.svelte';
  import SchedulePanel from '$lib/components/SchedulePanel.svelte';
  import UserProfileModal from '$lib/components/UserProfileModal.svelte';
  import WikiView from '$lib/components/wiki/WikiView.svelte';
  import { wikilinks } from '$lib/wiki/links';


  let message = $state('');
  let composer: MessageComposer | undefined = $state();
  let previewUrl: string | null = $state(null);
  let pendingFile: File | null = $state(null);
  let dragDepth = $state(0);
  let menuOpen = $state(false);
  let menuX = $state(0);
  let menuY = $state(0);

  let highlightedMessageId: number | null = $state(null);
  let pendingScrollToMessage: number | null = null;
  let highlightTimer: ReturnType<typeof setTimeout> | null = null;

  let commandFeedback: string | null = $state(null);
  let commandFeedbackType: 'info' | 'error' = $state('info');
  let feedbackTimer: ReturnType<typeof setTimeout> | null = null;

  let searchOpen = $state(false);
  let searchOverlay: SearchOverlay | undefined = $state();

  let wikiOpen = $state(false);
  let wikiView: WikiView | undefined = $state();
  /** Page a wikilink asked to open; consumed by WikiView on mount. */
  let wikiInitialSlug: string | null = $state(null);

  let helpOpen = $state(false);
  let helpOverlay: HelpOverlay | undefined = $state();

  let remindersOpen = $state(false);

  let now = $state(Date.now());
  let expiryTicker: number | null = null;

  let replyingTo: Message | null = $state(null);
  let threadRootId: number | null = $state(null);
  const dmConversations = dm.conversations;
  const dmActivePeer = dm.activePeer;
  const peerKeyConflicts = peerKeys.conflicts;
  /* Last-read message id captured when entering the channel; the "New"
     divider stays anchored there until the user switches channels. */
  let unreadMarkerAfterId = $state(0);

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

  function openReminders() {
    clearCommandFeedback();
    remindersOpen = true;
  }

  function closeReminders() {
    remindersOpen = false;
  }

  /**
   * Ask for a "when" and a note, then set a reminder anchored to a message.
   *
   * The note is never pre-filled from the message: in an encrypted channel the
   * server would then be holding a plaintext copy of something it is not
   * supposed to be able to read, and a rule that only sometimes applies is a
   * rule somebody eventually forgets. What the user types is what is stored.
   */
  async function remindAboutMessage(msg: Message) {
    const messageId = typeof msg.id === 'number' ? msg.id : undefined;
    const when = await dialogs.prompt({
      title: 'Remind me about this',
      label: 'When',
      placeholder: '15m, 2h, 3d, or 17:30',
      initial: '1h'
    });
    if (when === null) return;
    const preview = parseWhen(when);
    if (!preview) {
      void dialogs.alert({
        title: 'Remind me about this',
        message: `“${when}” is not a time. Try 15m, 2h, 3d or 17:30.`
      });
      return;
    }
    const previewBounds = scheduleBoundsError(preview);
    if (previewBounds) {
      void dialogs.alert({ title: 'Remind me about this', message: previewBounds });
      return;
    }
    const note = await dialogs.prompt({
      title: `Reminder ${describeWhen(preview.toISOString())}`,
      label: 'Note',
      maxLength: MAX_REMINDER_TEXT_LENGTH,
      placeholder: 'What is this about?'
    });
    if (note === null) return;
    const invalid = reminderTextError(note);
    if (invalid) {
      void dialogs.alert({ title: 'Remind me about this', message: invalid });
      return;
    }
    // Re-read the "when" now rather than reusing what it meant two dialogs
    // ago. A "45s" typed while the note prompt was still open would otherwise
    // be sent as a time already in the past, and the server would answer with
    // a message about bounds rather than about anything the user did. Both
    // readings are what was meant: a duration counts from finishing, an
    // absolute time is the same instant either way.
    const at = parseWhen(when);
    const bounds = at ? scheduleBoundsError(at) : 'That is not a time.';
    if (!at || bounds) {
      void dialogs.alert({ title: 'Remind me about this', message: bounds ?? '' });
      return;
    }
    chat.setReminder(note.trim(), at.toISOString(), messageId);
    setCommandFeedback(`Reminder set for ${describeWhen(at.toISOString())}.`);
  }

  /** Jump to the message a reminder was set on, switching channels if needed. */
  function openReminderTarget(channelId: number, messageId: number) {
    if (!$channels.some((channel) => channel.id === channelId)) return;
    joinChannel(channelId);
    focusMessage(messageId);
  }

  function setPendingFile(file: File | null) {
    // The server enforces its upload policy on /upload; checking here as well
    // saves a doomed round trip and names the actual reason. Covers the picker,
    // drag & drop and paste alike.
    if (file) {
      const rejection = describeUploadRejection(file);
      if (rejection) {
        setCommandFeedback(rejection, 'error');
        return;
      }
    }
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

  let emojiPickerOpen = $state(false);
  let emojiPickerX = $state(0);
  let emojiPickerY = $state(0);
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



  let inVoice = $state(false);
  let settingsOpen = $state(false);
  let currentChatChannelId: number = $state(0);
  let initialChannelSet = $state(false);
  let currentVoiceChannelId: number | null = $state(null);




  /* The server drops every connection into "general" and sends its history with
     the presence response, so the initial pick has to be "general" too — the
     channel list arrives sorted by name, which puts anything sorting ahead of
     it at index 0. The fallback only covers servers seeded without "general". */
  function defaultChannel(list: ChannelInfo[]): ChannelInfo {
    return list.find((c) => c.name === DEFAULT_CHANNEL_NAME) ?? list[0];
  }


  function latestMessageId(messages: Message[]): number | null {
    let max: number | null = null;
    for (const m of messages) {
      if (typeof m.id === 'number' && (max === null || m.id > max)) max = m.id;
    }
    return max;
  }






  function stream(node: HTMLAudioElement, data: { stream: MediaStream, userId: string }) {
    let currentUserId = data.userId;

    let analyser: AnalyserNode | null = null;
    let sourceNode: MediaStreamAudioSourceNode | null = null;
    let gainNode: GainNode | null = null;
    let stopTicks: (() => void) | null = null;
    let buffer: Uint8Array<ArrayBuffer> | null = null;

    // The per-user volume is applied by a gain node rather than the audio
    // element, because `HTMLMediaElement.volume` is clamped to 1 and quiet
    // members need to be boosted beyond 100%. The element still carries the
    // global volume and the output mute, and keeps `setSinkId` working.
    // If the graph could not be built we fall back to the element alone,
    // where any boost is clamped back down to 100%.
    const updateVolume = () => {
      const userVol = $userVolumes[currentUserId] ?? 1.0;
      if ($outputMuted) {
        node.volume = 0;
      } else {
        // Anything outside [0, 1] throws on assignment, so clamp rather than
        // trusting the stored values.
        const elementVol = $volume * (gainNode ? 1 : Math.min(userVol, 1));
        node.volume = Math.max(0, Math.min(1, elementVol));
      }
      if (gainNode) gainNode.gain.value = userVol;
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

    const disconnectNode = (audioNode: AudioNode | null, label: string) => {
      if (!audioNode) return;
      try {
        audioNode.disconnect();
      } catch (err) {
        if (import.meta.env.DEV) console.warn(`Failed to disconnect ${label}`, err);
      }
    };

    const teardownAudio = () => {
      if (stopTicks) {
        stopTicks();
        stopTicks = null;
      }
      disconnectNode(sourceNode, 'source node');
      sourceNode = null;
      disconnectNode(analyser, 'analyser');
      analyser = null;
      disconnectNode(gainNode, 'gain node');
      gainNode = null;
      buffer = null;
      setSpeaking(currentUserId, false);
    };

    const setupAudio = (stream: MediaStream | null | undefined) => {
      teardownAudio();
      node.srcObject = stream ?? null;
      if (!stream) return;

      try {
        // The context is shared with every other graph in the app: browsers
        // cap concurrent contexts at a handful, and one per peer used to
        // exhaust that budget in a busy channel — after which this whole
        // block threw and the per-user boost silently stopped working.
        const audioContext = getAudioContext();
        if (!audioContext) throw new Error('no audio context');
        resumeAudioContext();

        sourceNode = audioContext.createMediaStreamSource(stream);
        analyser = audioContext.createAnalyser();
        analyser.fftSize = 512;
        buffer = new Uint8Array(new ArrayBuffer(analyser.fftSize)) as Uint8Array<ArrayBuffer>;

        sourceNode.connect(analyser);

        // source -> gain -> destination stream, which the element then plays.
        // Playing the gained stream back through the element (instead of
        // sending it to the context destination) keeps `setSinkId` output
        // device selection and the global volume/mute working as before.
        gainNode = audioContext.createGain();
        const destination = audioContext.createMediaStreamDestination();
        sourceNode.connect(gainNode);
        gainNode.connect(destination);
        node.srcObject = destination.stream;
        updateVolume();

        stopTicks = subscribeTick(() => {
          if (!analyser || !buffer) return;
          analyser.getByteTimeDomainData(buffer);
          let sum = 0;
          for (let i = 0; i < buffer.length; i++) {
            const value = (buffer[i] - 128) / 128;
            sum += value * value;
          }
          const rms = Math.sqrt(sum / buffer.length);
          const speaking = rms > SPEAKING_RMS_THRESHOLD;
          setSpeaking(currentUserId, speaking);
        });
      } catch (error) {
        if (import.meta.env.DEV) {
          console.warn('Failed to build the remote audio graph', error);
        }
        // Never let a broken graph silence a peer: drop it and play the
        // stream straight from the element (no boost beyond 100%).
        teardownAudio();
        node.srcObject = stream;
        updateVolume();
      }
    };

    setupAudio(data.stream);

    return {
      update(newData: { stream: MediaStream, userId: string }) {
        if (currentUserId !== newData.userId) {
          setSpeaking(currentUserId, false);
          currentUserId = newData.userId;
        }
        setupAudio(newData.stream);
        updateVolume();
      },
      destroy() {
        unsubVol();
        unsubMute();
        unsubUserVol();
        unsubOut();
        teardownAudio();
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
        chat.join(currentChatChannelId);
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
    // The join was refused after the microphone was already open — drop the
    // half-open session rather than leave the UI showing a channel we are
    // not actually in.
    if (code === 'voice-channel-full') leaveVoice();
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

  // A `warn` rule lets the message through and tells the sender privately.
  // It carries the rule's name, never its pattern — what the server filters
  // is not something everyone gets to read.
  const handleAutomodWarning = (msg: Message) => {
    const name = typeof msg.rule === 'string' ? msg.rule.trim() : '';
    const rule = name ? `the “${name}” rule` : 'an auto-moderation rule';
    setCommandFeedback(`Your message was flagged by ${rule}.`, 'error');
  };
  chat.on('automod-warning', handleAutomodWarning);

  const handleUserUnbanned = (msg: Message) => {
    if (typeof msg.user !== 'string') return;
    setCommandFeedback(`${msg.user} has been unbanned.`);
  };
  chat.on('user-unbanned', handleUserUnbanned);

  // Danger Zone actions rearrange the app under everyone at once: without a
  // word, the history and half the channels simply vanish mid-sentence.
  const handleMessagesPurged = (msg: Message) => {
    const by = typeof msg.by === 'string' && msg.by ? ` by ${msg.by}` : '';
    setCommandFeedback(`Every message on this server was deleted${by}.`, 'error');
  };
  chat.on('messages-purged', handleMessagesPurged);

  const handleServerReset = (msg: Message) => {
    const by = typeof msg.by === 'string' && msg.by ? ` by ${msg.by}` : '';
    // The channel this client was viewing may be gone. The server re-sends
    // the channel lists, and the effect that watches them drops back to
    // `general` and rejoins, so all that is left here is saying why.
    setCommandFeedback(`This server was reset${by}.`, 'error');
  };
  chat.on('server-reset', handleServerReset);

  onMount(() => {
    if (!get(session).user) {
      goto('/login');
      return;
    }
    userRoleIds.reset();
    roleDefinitions.reset();
    channelOverrides.reset();
    profiles.reset();
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
    // Leaving the server: park what is in the composer alongside the drafts
    // of the other channels, which the store already holds per server URL.
    drafts.park(channelDraft(currentChatChannelId), message);
    chat.off('history', handleHistory);
    chat.off('message-deleted', handleMessageDeleted);
    chat.off('error', handleServerError);
    chat.off('force-disconnect', handleForceDisconnect);
    chat.off('user-muted', handleUserMuted);
    chat.off('user-unmuted', handleUserUnmuted);
    chat.off('automod-warning', handleAutomodWarning);
    chat.off('user-unbanned', handleUserUnbanned);
    chat.off('messages-purged', handleMessagesPurged);
    chat.off('server-reset', handleServerReset);
    chat.disconnect();
    if (currentVoiceChannelId !== null) {
      voice.leave(currentVoiceChannelId);
    }
    ping.stop();
    userRoleIds.reset();
    roleDefinitions.reset();
    channelOverrides.reset();
    profiles.reset();
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
    // The quoted snippet is passed along because an encrypted channel gives
    // the server no plaintext to build one from; it travels sealed instead.
    const error = chat.send($session.user ?? 'anon', message, replyTarget, replyingTo?.text);
    if (error) {
      setCommandFeedback(error, 'error');
      return;
    }
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
    const base = httpBaseFromWs(selected);
    if (import.meta.env.DEV) console.log('Uploading file to', base + '/upload', file);
    try {
      const res = await fetch(base + '/upload', { method: 'POST', body: uploadForm(file) });
      if (import.meta.env.DEV) console.log('Upload response status:', res.status);
      if (res.status === 413) {
        setCommandFeedback(
          `File is too large to upload (limit: ${formatUploadSize($uploadConfig.maxBytes)}).`,
          'error'
        );
        return;
      }
      const uploadError = uploadErrorMessage(res.status);
      if (uploadError) {
        setCommandFeedback(uploadError, 'error');
        return;
      }
      if (!res.ok) {
        throw new Error(`upload failed with status ${res.status}`);
      }
      const data = await res.json();
      if (import.meta.env.DEV) console.log('Upload response data:', data);
      const url = data.url as string;
      const absolute = url.startsWith('http') ? url : base + url;
      const sendError =
        data.kind === 'image' || file.type.startsWith('image/')
          ? chat.sendUpload($session.user ?? 'anon', { image: absolute })
          : chat.sendUpload($session.user ?? 'anon', {
              attachment: {
                url: absolute,
                name: typeof data.name === 'string' ? data.name : file.name,
                size: typeof data.size === 'number' ? data.size : file.size
              }
            });
      if (sendError) setCommandFeedback(sendError, 'error');
    } catch (e) {
      console.error('upload failed', e);
      setCommandFeedback('File upload failed.', 'error');
    } finally {
      clearPendingFile();
    }
  }

  async function send() {
    // The server rejects sends without SEND_MESSAGES; mirror that here so
    // hotkeys and slash commands can't bypass the disabled composer.
    if (!get(can)(PERMISSIONS.SEND_MESSAGES)) return;
    const hasMessage = message.trim() !== '';
    if (!pendingFile && !hasMessage) return;
    // Slow mode is enforced server-side, which would bounce the message back
    // as an error *after* clearing the composer. Naming the wait up front
    // keeps what the user typed. Members who can manage messages are exempt
    // there, so they are exempt here too.
    if (!get(can)(PERMISSIONS.MANAGE_MESSAGES)) {
      const wait = slowModeWait();
      if (wait > 0) {
        setCommandFeedback(`Slow mode is on — ${wait}s before your next message.`, 'error');
        return;
      }
    }
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
        const meError = chat.send(currentUser ?? 'anon', `_${rest}_`);
        if (meError) setCommandFeedback(meError, 'error');
        return true;
      }
      case 'shrug': {
        // Backslash-escaped so markdown doesn't italicize the face.
        const shrug = '¯\\\\\\_(ツ)\\_/¯';
        const text = rest ? `${rest} ${shrug}` : shrug;
        const shrugError = chat.send(currentUser ?? 'anon', text);
        if (shrugError) setCommandFeedback(shrugError, 'error');
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
        const ephemeralError = chat.sendEphemeral(
          currentUser,
          contentText,
          expires.toISOString()
        );
        if (ephemeralError) {
          setCommandFeedback(ephemeralError, 'error');
          return true;
        }
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
      case 'reminders': {
        openReminders();
        return true;
      }
      case 'remind':
      case 'remindme': {
        const { when, text } = splitWhen(rest);
        if (!when || !text) {
          setCommandFeedback('Usage: /remind <when> <note> — e.g. /remind 15m stretch', 'error');
          return true;
        }
        const at = parseWhen(when);
        if (!at) {
          setCommandFeedback(`“${when}” is not a time. Try 15m, 2h, 3d or 17:30.`, 'error');
          return true;
        }
        const bounds = scheduleBoundsError(at);
        if (bounds) {
          setCommandFeedback(bounds, 'error');
          return true;
        }
        const invalid = reminderTextError(text);
        if (invalid) {
          setCommandFeedback(invalid, 'error');
          return true;
        }
        chat.setReminder(text, at.toISOString());
        setCommandFeedback(`Reminder set for ${describeWhen(at.toISOString())}.`);
        return true;
      }
      case 'schedule': {
        const { when, text } = splitWhen(rest);
        if (!when || !text) {
          setCommandFeedback(
            'Usage: /schedule <when> <message> — e.g. /schedule 2h notes are up',
            'error'
          );
          return true;
        }
        const at = parseWhen(when);
        if (!at) {
          setCommandFeedback(`“${when}” is not a time. Try 15m, 2h, 3d or 17:30.`, 'error');
          return true;
        }
        const bounds = scheduleBoundsError(at);
        if (bounds) {
          setCommandFeedback(bounds, 'error');
          return true;
        }
        // Sealed here for an encrypted channel, which is why this goes through
        // the chat store rather than a raw frame.
        const scheduleError = chat.scheduleMessage(
          currentChatChannelId,
          text,
          at.toISOString()
        );
        if (scheduleError) {
          setCommandFeedback(scheduleError, 'error');
          return true;
        }
        setCommandFeedback(`Message queued for ${describeWhen(at.toISOString())}.`);
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

  function doSearch(query: string) {
    return chat.search(currentChatChannelId, query, 50);
  }

  /**
   * Open a wiki page hit. Results always come from the current channel, so
   * the only question is whether the wiki view is already mounted: it
   * captures its page on mount, so an open one is navigated through itself.
   */
  function handleSearchPage(slug: string) {
    if (wikiOpen) {
      void wikiView?.openPage(slug);
      return;
    }
    openWikiPage(null, slug);
  }

  function joinChannel(id: number) {
    if (id === currentChatChannelId) return;
    // Park the half-typed sentence under the channel being left and restore
    // whatever was parked for the one being entered. One composer serves
    // every channel, so without this the text follows the user across.
    drafts.park(channelDraft(currentChatChannelId), message);
    wikiInitialSlug = null;
    currentChatChannelId = id;
    message = drafts.take(channelDraft(id));
    unreadMarkerAfterId = unread.getLastRead(id);
    unread.setActive(id);
    replyingTo = null;
    closeThread();
    loadingHistory = false;
    chat.clear();
    chat.join(id);
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

  /**
   * Forward a message to another channel or into a direct message.
   *
   * The two destinations take different routes and have to: a channel forward
   * is a server-side copy, so only the ids are sent and the attribution is the
   * server's; a DM is end-to-end encrypted, so the copy is composed and sealed
   * here instead. See `src/lib/chat/forward.ts`.
   */
  async function forwardMessage(msg: Message) {
    const messageId = typeof msg.id === 'number' ? msg.id : null;
    if (messageId === null) return;

    const me = $session.user;
    // Conversations already open first — they are who a forward usually goes
    // to — then everyone else this server knows, online or not.
    const peers = [...Object.keys($dmConversations), ...$onlineUsers, ...$offlineUsers].filter(
      (peer, index, all) => peer !== me && all.indexOf(peer) === index
    );
    const options = forwardOptions({
      channels: $channels,
      currentChannelId: currentChatChannelId,
      peers,
      displayName: $displayNames
    });
    if (options.length === 0) {
      void dialogs.alert({
        title: 'Forward message',
        message: 'There is nowhere else on this server to forward this to.'
      });
      return;
    }

    const target = parseForwardTarget(
      await dialogs.select({
        title: 'Forward message',
        message: 'The copy keeps the original author and says where it came from.',
        options,
        confirmLabel: 'Forward'
      })
    );
    if (!target) return;

    if (target.kind === 'channel') {
      const error = chat.forward(messageId, target.channelId);
      if (error) {
        setCommandFeedback(error, 'error');
        return;
      }
      const name = $channels.find((channel) => channel.id === target.channelId)?.name ?? '';
      setCommandFeedback(`Forwarded to #${name}.`);
      return;
    }

    const error = await chat.sendDm(
      target.user,
      forwardedDmText(msg, currentChatChannelName, $displayNames)
    );
    if (error) {
      void dialogs.alert({ title: 'Message not sent', message: error });
      return;
    }
    setCommandFeedback(`Forwarded to ${$displayNames(target.user)}.`);
  }

  /** Jump to the original of a forwarded message, switching channels first. */
  function focusForwardedSource(origin: ForwardInfo) {
    if (!$channels.some((channel) => channel.id === origin.channelId)) return;
    joinChannel(origin.channelId);
    focusMessage(origin.id);
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
    const root = $chat.find((m) => m.id === threadRootId);
    const error = chat.send($session.user ?? 'anon', text, threadRootId, root?.text);
    if (error) setCommandFeedback(error, 'error');
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
    void chat.sendDm(peer, text).then((error) => {
      if (error) void dialogs.alert({ title: 'Message not sent', message: error });
    });
  }

  /** Accept a DM peer's changed identity key after the user confirmed it. */
  function trustDmKey() {
    const peer = $dmActivePeer;
    if (peer) peerKeys.trust(peer);
  }

  /** Show the conversation's key fingerprint for out-of-band comparison. */
  function verifyDmKeys() {
    const peer = $dmActivePeer;
    if (!peer) return;
    // Verify the key actually in use: the unconfirmed new key if there is
    // a conflict, the pinned one otherwise.
    const peerKey = $peerKeyConflicts[peer] ?? peerKeys.pinned(peer);
    if (!peerKey) {
      void dialogs.alert({
        title: 'No key yet',
        message: `No encryption key is known for ${peer} on this server.`
      });
      return;
    }
    void dialogs.alert({
      title: `Verify keys with ${peer}`,
      message:
        `Fingerprint: ${dmFingerprint(loadKeyPair().publicKey, peerKey)} — ` +
        `compare it with ${peer} over another channel (in person, a call, …). ` +
        `It must match exactly on both ends.`
    });
  }


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
    resetVoicePermissions();
    // Neither a screen share nor a camera can outlive the voice session —
    // stop our own captures and close every share we were watching.
    stopScreenShare();
    leaveScreenShareAsViewer();
    void stopCamera();
  }

  /**
   * Toggle a share on the stage. Watching several at once is the point, so
   * this only ever adds or removes the one share that was clicked; a second
   * click on a share we already watch closes it and leaves the rest running.
   */
  async function handleViewScreenShare(userId: string) {
    try {
      if (!$session.user || currentVoiceChannelId === null) {
        await dialogs.alert({
          title: 'Join a voice channel first',
          message: 'You must be in a voice channel to view screen shares.'
        });
        return;
      }

      // Our own share needs no peer connection — show/hide the local preview.
      if (userId === $session.user) {
        toggleScreenSharePreview();
        return;
      }

      if ($watchedScreenShares.includes(userId)) {
        closeScreenShare(userId);
        return;
      }

      await openScreenShare(userId, $session.user, currentVoiceChannelId);
    } catch (error) {
      closeScreenShare(userId);
      console.error('Failed to view screen share:', error);
      dialogs.alert({
        title: 'Screen share unavailable',
        message: 'Could not open this screen share. The stream may have ended.'
      });
    }
  }

  function leaveServer() {
    chat.disconnect();
    if (currentVoiceChannelId !== null) {
      voice.leave(currentVoiceChannelId);
    }
    selectedServer.set(null);
    goto('/servers');
  }

  async function createChannelPrompt(categoryId: number | null = null, isPrivate = false) {
    const name = await dialogs.prompt({
      title: isPrivate ? 'Create private text channel' : 'Create text channel',
      label: 'Channel name',
      placeholder: 'e.g. general',
      confirmLabel: 'Create'
    });
    if (name) channels.create(name.trim(), categoryId, isPrivate);
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
      // The server's configured default, so a channel created without a
      // thought still lands on what the operator wanted.
      initial: $voiceDefaults.quality,
      confirmLabel: 'Apply'
    });
    if (quality === null) return null;
    const preset = VOICE_QUALITY_PRESETS.find((p) => p.quality === quality) ?? DEFAULT_VOICE_PRESET;
    return { quality: preset.quality, bitrate: preset.bitrate };
  }

  async function createVoiceChannelPrompt(categoryId: number | null = null, isPrivate = false) {
    const name = await dialogs.prompt({
      title: isPrivate ? 'Create private voice channel' : 'Create voice channel',
      label: 'Channel name',
      placeholder: 'e.g. Lounge',
      confirmLabel: 'Next'
    });
    if (!name) return;
    const preset = await selectVoicePreset();
    if (!preset) return;
    voiceChannels.create(name.trim(), preset, categoryId, isPrivate);
  }

  async function joinVoiceChannel(id: number) {
    if (!$session.user) return;
    if (inVoice && currentVoiceChannelId !== null) {
      // Switching channels ends the old voice session and with it any
      // running screen share or camera (both are bound to the old channel).
      stopScreenShare();
      void stopCamera();
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

  let menuChannelId: number | null = $state(null);
  let menuVoiceChannelId: number | null = $state(null);
  let menuCategoryId: number | null = $state(null);
  let volumeMenuOpen = $state(false);
  let volumeMenuX = $state(0);
  let volumeMenuY = $state(0);
  let volumeMenuUser: string | null = $state(null);

  function closeVolumeMenu() {
    volumeMenuOpen = false;
    volumeMenuUser = null;
  }

  let userRoleMenuOpen = $state(false);
  let userRoleMenuX = $state(0);
  let userRoleMenuY = $state(0);
  let userRoleMenuTarget: string | null = $state(null);

  /** Highest hierarchy position of a target user (Infinity for admins). */
  function targetTopPosition(target: string): number {
    return computeTopPosition($roleDefinitions, $userRoleIds[target] ?? []);
  }

  /** Whether the current user strictly outranks `target`. */
  function outranks(target: string): boolean {
    if (target === $session.user) return false;
    return $myTopPosition > targetTopPosition(target);
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

  /** Replace a user's assigned roles (server validates the hierarchy). */
  function setUserRoles(user: string, roleIds: number[]) {
    chat.sendRaw({ type: 'set-user-roles', user, roleIds });
  }

  /** Toggle a single role on a user, preserving their other assignments. */
  function toggleUserRole(user: string, roleId: number) {
    const current = $userRoleIds[user] ?? [];
    const next = current.includes(roleId)
      ? current.filter((id) => id !== roleId)
      : [...current, roleId];
    setUserRoles(user, next);
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

  /** Set or clear another member's nickname on this server. */
  async function changeNicknamePrompt(user: string) {
    const current = $profiles[user]?.nickname ?? '';
    const nickname = await dialogs.prompt({
      title: `Nickname for ${user}`,
      message: 'Shown instead of their display name on this server. Leave empty to clear it.',
      label: 'Nickname',
      initial: current,
      maxLength: MAX_NICKNAME_LENGTH,
      confirmLabel: 'Save',
      required: false
    });
    // `null` is a cancelled dialog; an empty string is a deliberate clear.
    if (nickname === null) return;
    profiles.setNickname(user, nickname.trim());
  }


  function openChannelMenu(event: MouseEvent, channelId?: number, voice?: boolean) {
    if (!$can(PERMISSIONS.MANAGE_CHANNELS)) return;
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
    if (!$can(PERMISSIONS.MANAGE_CHANNELS)) return;
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

  let serverDashboardOpen = $state(false);

  function openServerDashboard() {
    serverDashboardOpen = true;
  }

  function closeServerDashboard() {
    serverDashboardOpen = false;
  }

  // Per-channel permissions editor (private channels).
  let channelPermsOpen = $state(false);
  let channelPermsId: number | null = $state(null);
  let channelPermsVoice = $state(false);
  let channelPermsName = $state('');

  function openChannelPermissions(id: number, voice: boolean, name: string) {
    channelPermsId = id;
    channelPermsVoice = voice;
    channelPermsName = name;
    channelPermsOpen = true;
  }

  function closeChannelPermissions() {
    channelPermsOpen = false;
  }

  let statsUser: string | null = $state(null);

  function openUserStats(user: string) {
    statsUser = user;
  }

  function closeUserStats() {
    statsUser = null;
  }

  /** The member whose profile is open, or null. Works for the own profile
      too, where the modal doubles as the profile editor. */
  let profileUser: string | null = $state(null);

  function openProfile(user: string) {
    profileUser = user;
  }

  function closeProfile() {
    profileUser = null;
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
    return $can(PERMISSIONS.MANAGE_MESSAGES);
  }

  function canEditMessage(msg: Message): boolean {
    const current = $session.user;
    if (!current || typeof msg.id !== 'number') return false;
    if (typeof msg.text !== 'string' || msg.text.trim() === '') return false;
    // A forward's words are the original author's; the server refuses to let
    // the forwarder rewrite them under their own attribution.
    if (msg.forwardedFrom) return false;
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
    const error = chat.edit(msg.id, input);
    if (error) setCommandFeedback(error, 'error');
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
    // Listen-only channels (no Talk permission) can never unmute.
    if (!get(canSpeak)) {
      microphoneMuted.set(true);
      return;
    }
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

  async function renameChannelPrompt(id: number) {
    const ch = $channels.find((c) => c.id === id);
    const name = await dialogs.prompt({
      title: 'Rename channel',
      label: 'Channel name',
      initial: ch?.name ?? '',
      confirmLabel: 'Rename'
    });
    if (name) channels.rename(id, name.trim());
  }

  async function renameVoiceChannelPrompt(id: number) {
    const ch = $voiceChannels.find((c) => c.id === id);
    const name = await dialogs.prompt({
      title: 'Rename voice channel',
      label: 'Channel name',
      initial: ch?.name ?? '',
      confirmLabel: 'Rename'
    });
    if (name) voiceChannels.rename(id, name.trim());
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


  let messagesContainer: HTMLDivElement | undefined = $state();
  async function scrollBottom() {
    await tick();
    if (messagesContainer) {
      setScrollTop(messagesContainer.scrollHeight);
    }
  }
  let lastLength = 0;
  let loadingHistory = $state(false);
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

  /* Post-render scroll maintenance: honour a pending scroll-to-message and
     stick to the bottom when new messages arrive in the current channel. */
  $effect(() => {
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
  let serverStrength = $derived(pingToStrength($ping));
  let dragActive = $derived(dragDepth > 0);
  let statusMap: Record<string, UserStatus> = $derived.by(() => {
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
  });
  // Whether the current user can reach the server dashboard at all: any
  // management capability qualifies.
  let currentUserCanManage = $derived(
    $can(PERMISSIONS.MANAGE_MESSAGES) ||
      $can(PERMISSIONS.MANAGE_CHANNELS) ||
      $can(PERMISSIONS.MANAGE_WIKI) ||
      $can(PERMISSIONS.MANAGE_ROLES) ||
      $can(PERMISSIONS.MANAGE_EMOJIS) ||
      $can(PERMISSIONS.MANAGE_SERVER) ||
      $can(PERMISSIONS.KICK_MEMBERS) ||
      $can(PERMISSIONS.BAN_MEMBERS) ||
      $can(PERMISSIONS.MUTE_MEMBERS)
  );
  $effect(() => {
    if ($channels.length && !$channels.some((c) => c.id === currentChatChannelId)) {
      // Same swap as joinChannel: this fires on the first channel list and
      // whenever the channel being viewed is deleted underneath the user.
      drafts.park(channelDraft(currentChatChannelId), message);
      currentChatChannelId = defaultChannel($channels).id;
      message = drafts.take(channelDraft(currentChatChannelId));
      unreadMarkerAfterId = unread.getLastRead(currentChatChannelId);
      unread.setActive(currentChatChannelId);
      loadingHistory = false;
      // On the first pass the server already placed us in the default
      // channel, so only record it; a join here would double the history.
      chat.join(currentChatChannelId, initialChannelSet);
      initialChannelSet = true;
    }
  });
  let currentChatChannelName = $derived($channels.find(c => c.id === currentChatChannelId)?.name ?? '');
  let currentChannelEncrypted = $derived(
    $channels.find((c) => c.id === currentChatChannelId)?.e2ee === true
  );
  // The channel is encrypted but no key of ours has arrived: either nobody has
  // opened one yet, or no member has been online to wrap it for us. Composing
  // is blocked rather than silently failing at send time.
  let currentChannelKeyPending = $derived(
    currentChannelEncrypted &&
      !($channelKeys[currentChatChannelId]?.keys?.[$channelKeys[currentChatChannelId]?.epoch ?? -1])
  );
  // One window per share being watched, plus our own capture while the
  // self-preview is on. The peer is looked up on every change rather than
  // captured once: it is absent while the connection comes up, and the manager
  // republishes it when the share changes (audio arriving alongside the video,
  // say).
  let screenShareTiles = $derived.by<WatchedScreenShare[]>(() => {
    const tiles: WatchedScreenShare[] = $watchedScreenShares.map((userId) => ({
      key: `peer:${userId}`,
      userId,
      peer: $screenSharePeers.find((p) => p.userId === userId) ?? null,
      isSelf: false,
      onClose: () => closeScreenShare(userId)
    }));
    if ($localScreenShareStream && $screenSharePreview) {
      const stream = $localScreenShareStream;
      tiles.push({
        key: 'self',
        userId: $session.user ?? 'You',
        peer: { userId: $session.user ?? 'You', stream, hasAudio: stream.getAudioTracks().length > 0 },
        isSelf: true,
        // Closing the preview window is the same thing as switching the
        // preview off — nothing renders the capture a second time then.
        onClose: () => screenSharePreview.set(false)
      });
    }
    return tiles;
  });
  // Every camera on in the voice channel we are in: our own preview first,
  // then each peer whose camera the server announced. A peer with no stream
  // yet is still listed — the tile says "Connecting…" rather than appearing a
  // second later, which is the same reason a watched share gets a placeholder.
  let webcamTiles = $derived.by<WebcamTile[]>(() => {
    if (!inVoice || currentVoiceChannelId === null) return [];
    const tiles: WebcamTile[] = [];
    if ($localCameraStream) {
      tiles.push({
        key: 'self',
        userId: $session.user ?? '',
        stream: $localCameraStream,
        isSelf: true
      });
    }
    for (const user of $activeWebcams[currentVoiceChannelId] ?? []) {
      if (user === $session.user) continue;
      tiles.push({ key: `peer:${user}`, userId: user, stream: $voiceVideo[user] ?? null, isSelf: false });
    }
    return tiles;
  });
  let channelMessages = $derived($chat.filter((m) => m.channelId === currentChatChannelId));
  let messageBlocks = $derived(buildMessageBlocks(channelMessages, {
    unreadAfterId: unreadMarkerAfterId,
    currentUser: $session.user
  }));
  let pinnedEntries = $derived($pinned[currentChatChannelId] ?? []);
  // Everything rendered in the active channel counts as read.
  $effect(() => {
    const latest = latestMessageId(channelMessages);
    if (latest !== null) unread.markRead(currentChatChannelId, latest);
  });
  let typingLabel = $derived.by(() => {
    const users = Object.entries($typing[currentChatChannelId] ?? {})
      .filter(([user, expiry]) => user !== $session.user && expiry > now)
      .map(([user]) => $displayNames(user));
    if (users.length === 0) return null;
    if (users.length === 1) return `${users[0]} is typing…`;
    if (users.length === 2) return `${users[0]} and ${users[1]} are typing…`;
    return 'Several people are typing…';
  });
  let threadReplyCounts = $derived.by(() => {
    const map = new Map<number, number>();
    for (const m of channelMessages) {
      if (typeof m.threadId === 'number') {
        map.set(m.threadId, (map.get(m.threadId) ?? 0) + 1);
      }
    }
    return map;
  });
  /* The panel merges the server's thread snapshot with live messages from the
     store, so replies arriving while the thread is open show up immediately. */
  let threadMessages = $derived.by(() => {
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
  });
  let currentTopic = $derived($channelTopics[currentChatChannelId] ?? '');
  let dmMessages = $derived($dmActivePeer ? ($dmConversations[$dmActivePeer] ?? []) : []);
  // Roles the current user may grant: below their own position and no more
  // powerful than themselves (the server enforces the same bounds).
  let assignableRoles = $derived(
    $roleDefinitions.filter(
      (def) =>
        !def.isDefault &&
        def.position < $myTopPosition &&
        ($can(PERMISSIONS.ADMINISTRATOR) || (def.permissions & ~$myPermissions) === 0)
    )
  );
  let userRoleMenuItems = $derived((() => {
    if (!userRoleMenuTarget) return [];
    const target = userRoleMenuTarget;
    const items: ContextMenuItem[] = [];
    items.push({ label: 'View Profile', action: () => openProfile(target) });
    items.push({ label: 'Send Message', action: () => openDm(target) });
    items.push({ label: 'View Stats', action: () => openUserStats(target) });
    // Role assignment: a checklist of grantable roles, shown only to managers
    // who outrank the target.
    if ($can(PERMISSIONS.MANAGE_ROLES) && outranks(target) && assignableRoles.length) {
      const assigned = new Set($userRoleIds[target] ?? []);
      const roleItems: ContextMenuItem[] = assignableRoles.map((def) => ({
        label: assigned.has(def.id) ? `${def.name} (assigned)` : def.name,
        action: () => toggleUserRole(target, def.id)
      }));
      items.push({ label: 'Roles', children: roleItems });
    }
    if (outranks(target)) {
      if ($can(PERMISSIONS.MUTE_MEMBERS)) {
        items.push({
          label: 'Mute',
          children: [
            { label: '10 minutes', action: () => muteUser(target, 600) },
            { label: '1 hour', action: () => muteUser(target, 3600) },
            { label: 'Until lifted', action: () => muteUser(target) },
            { label: 'Unmute', action: () => unmuteUser(target) }
          ]
        });
      }
      if ($can(PERMISSIONS.MANAGE_NICKNAMES)) {
        items.push({ label: 'Change Nickname', action: () => changeNicknamePrompt(target) });
      }
      if ($can(PERMISSIONS.KICK_MEMBERS) && $onlineUsers.includes(target)) {
        items.push({ label: 'Kick User', danger: true, action: () => kickUser(target) });
      }
      if ($can(PERMISSIONS.BAN_MEMBERS)) {
        items.push({ label: 'Ban User', danger: true, action: () => banUser(target) });
        items.push({ label: 'Unban User', action: () => unbanUser(target) });
      }
    }
    return items;
  })());
  let channelMenuItems = $derived(!$can(PERMISSIONS.MANAGE_CHANNELS) ? [] : [
    {
      label: 'Create',
      children: [
        { label: 'Text Channel', action: () => createChannelPrompt() },
        { label: 'Voice Channel', action: () => createVoiceChannelPrompt() },
        { label: 'Private Text Channel', action: () => createChannelPrompt(null, true) },
        { label: 'Private Voice Channel', action: () => createVoiceChannelPrompt(null, true) },
        { label: 'Category', action: createCategoryPrompt }
      ]
    },
    ...(menuChannelId != null
      ? [
          {
            label: 'Edit Permissions',
            action: () =>
              openChannelPermissions(
                menuChannelId!,
                false,
                $channels.find((c) => c.id === menuChannelId)?.name ?? ''
              )
          },
          { label: 'Rename Channel', action: () => renameChannelPrompt(menuChannelId!) },
          ...buildMoveToItems(menuChannelId, false),
          { label: 'Delete Channel', action: () => channels.remove(menuChannelId!), danger: true }
        ]
      : []),
    ...(menuVoiceChannelId != null
      ? [
          {
            label: 'Edit Permissions',
            action: () =>
              openChannelPermissions(
                menuVoiceChannelId!,
                true,
                $voiceChannels.find((c) => c.id === menuVoiceChannelId)?.name ?? ''
              )
          },
          {
            label: 'Set Voice Quality',
            children: VOICE_QUALITY_PRESETS.map((preset) => ({
              label:
                preset.bitrate && preset.bitrate > 0
                  ? `${preset.label} (${Math.round(preset.bitrate / 1000)} kbps)`
                  : preset.label,
              action: () =>
                voiceChannels.configure(menuVoiceChannelId!, {
                  quality: preset.quality,
                  bitrate: preset.bitrate
                })
            }))
          },
          { label: 'Rename Voice Channel', action: () => renameVoiceChannelPrompt(menuVoiceChannelId!) },
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
  ]);
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
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="resizer" role="separator" aria-label="Resize channel list" onmousedown={startLeftResize}></div>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div
      class="chat"
      ondragenter={handleDragEnter}
      ondragover={handleDragOver}
      ondragleave={handleDragLeave}
      ondrop={handleDrop}
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
        encrypted={currentChannelEncrypted}
        {serverStrength}
        {statusMap}
        onEditTopic={editTopic}
        onOpenSearch={() => openSearch()}
        onOpenReminders={openReminders}
        reminderAttention={$scheduledAttention}
        onOpenSettings={openSettings}
        {wikiOpen}
        onToggleWiki={toggleWiki}
        showServerDashboard={currentUserCanManage}
        onOpenServerDashboard={openServerDashboard}
        onLeaveServer={leaveServer}
        onLogout={logout}
        onOpenProfile={() => $session.user && openProfile($session.user)}
      />
      <SettingsModal open={settingsOpen} close={closeSettings} />
      <ServerDashboardModal
        open={serverDashboardOpen}
        close={closeServerDashboard}
        permissions={$myPermissions}
      />
      <ChannelPermissionsModal
        open={channelPermsOpen}
        close={closeChannelPermissions}
        channelId={channelPermsId}
        voice={channelPermsVoice}
        channelName={channelPermsName}
      />
      <UserStatsModal open={statsUser !== null} user={statsUser} close={closeUserStats} />
      <SchedulePanel
        open={remindersOpen}
        close={closeReminders}
        onOpenMessage={openReminderTarget}
      />
      <UserProfileModal
        open={profileUser !== null}
        user={profileUser}
        close={closeProfile}
        onOpenDm={openDm}
      />
      <HelpOverlay bind:this={helpOverlay} open={helpOpen} onClose={closeHelp} />
      <SearchOverlay
        bind:this={searchOverlay}
        open={searchOpen}
        onClose={closeSearch}
        onSearch={doSearch}
        onFocusResult={handleSearchResult}
        onOpenPage={handleSearchPage}
        encrypted={currentChannelEncrypted}
        {now}
      />
      {#if wikiOpen}
        <!-- Edit affordances follow the MANAGE_WIKI permission; the server
             enforces the same gate on every wiki mutation. -->
        {#key currentChatChannelId}
          <WikiView
            bind:this={wikiView}
            channelId={currentChatChannelId}
            channelName={currentChatChannelName}
            canEdit={$can(PERMISSIONS.MANAGE_WIKI)}
            initialSlug={wikiInitialSlug}
            onCrossChannel={openWikiPage}
          />
        {/key}
      {:else}
      {#if webcamTiles.length > 0}
        <WebcamStage tiles={webcamTiles} />
      {/if}
      <PinnedBar
        entries={pinnedEntries}
        messages={channelMessages}
        onFocusMessage={focusMessage}
      />
      <div class="messages-shell">
        <div
          class="messages"
          bind:this={messagesContainer}
          onscroll={onScroll}
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
                onFocusForwarded={focusForwardedSource}
                onReply={startReply}
                onForward={forwardMessage}
                onRemind={remindAboutMessage}
                onEdit={editChatMessage}
                onTogglePin={togglePinMessage}
                onDelete={deleteChatMessage}
                onOpenEmojiPicker={openEmojiPicker}
                onToggleReaction={toggleReaction}
                onOpenThread={openThread}
                onOpenProfile={openProfile}
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
        canSend={$can(PERMISSIONS.SEND_MESSAGES)}
        encrypted={currentChannelEncrypted}
        keyPending={currentChannelEncrypted && currentChannelKeyPending}
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
          draftKey={threadDraft(threadRootId)}
          emphasize={(msg) => msg.id === threadRootId}
        />
      {/if}

      {#if $dmActivePeer}
        <ConversationPanel
          kind="dm"
          title={$displayNames($dmActivePeer)}
          messages={dmMessages}
          emptyText="No messages yet. Say hi!"
          placeholder={`Message ${$displayNames($dmActivePeer)}…`}
          onSend={sendDmMessage}
          onClose={closeDm}
          draftKey={dmDraft($dmActivePeer)}
          emphasize={(msg) => msg.from === $session.user}
          keyWarning={$dmActivePeer in $peerKeyConflicts}
          onTrustKey={trustDmKey}
          onVerify={verifyDmKeys}
        />
      {/if}

      {#each $voice as peer (peer.id)}
        <audio autoplay use:stream={{ stream: peer.stream, userId: peer.id }}></audio>
      {/each}
    </div>
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <div class="resizer" role="separator" aria-label="Resize user list" onmousedown={startRightResize}></div>
    <UserList
      {statusMap}
      onUserContextMenu={openUserRoleMenu}
      onOpenProfile={openProfile}
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

<!-- Every share being watched, each in its own floating window over the app;
     our own capture joins them as a corner preview while sharing. -->
{#if screenShareTiles.length > 0}
  <ScreenShareLayer tiles={screenShareTiles} onCloseAll={closeAllScreenShares} />
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
