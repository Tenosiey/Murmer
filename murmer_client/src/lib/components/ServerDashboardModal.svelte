<!--
  Server management dashboard for moderators and above. Separate from the
  user-facing SettingsModal: everything in here is server-wide state, gated
  per tab by the permission it controls.

  Every control here is cosmetic twice over: the server re-checks the
  permission behind each frame, and it enforces the settings themselves
  (slow mode, the message cap, the profanity filter, the auto-moderation
  rules, the upload policy). What
  a tab renders is the server's own answer — the identity, chat, upload,
  voice and screen-share frames it broadcasts — never local state that could
  drift from it.
-->
<script lang="ts">
  import { onMount, onDestroy, untrack } from 'svelte';
  import { chat } from '$lib/stores/chat';
  import { selectedServer } from '$lib/stores/servers';
  import { customEmojis, customEmojiList } from '$lib/stores/customEmojis';
  import { dialogs } from '$lib/stores/dialogs';
  import { describeServerError } from '$lib/errors';
  import { httpBaseFromWs } from '$lib/server-url';
  import { uploadForm, uploadErrorMessage } from '$lib/upload';
  import { onlineUsers } from '$lib/stores/online';
  import { displayNames } from '$lib/stores/profiles';
  import {
    EMOJI_NAME_RE,
    MAX_EMOJI_FILE_BYTES,
    MAX_SERVER_NAME_LENGTH,
    MAX_SERVER_DESCRIPTION_LENGTH,
    MAX_WELCOME_MESSAGE_LENGTH,
    MAX_SERVER_ICON_BYTES,
    MAX_ROLE_ICON_BYTES,
    MIN_UPLOAD_MAX_BYTES,
    MAX_UPLOAD_MAX_BYTES,
    UPLOAD_CATEGORIES,
    MAX_MESSAGE_LENGTH,
    MIN_CONFIGURABLE_MESSAGE_LENGTH,
    MAX_SLOW_MODE_SECONDS,
    MAX_PROFANITY_WORDS,
    MAX_PROFANITY_WORD_LEN,
    AUTOMOD_ACTIONS,
    AUTOMOD_KINDS,
    DEFAULT_AUTOMOD_MUTE_SECONDS,
    MAX_AUTOMOD_NAME_LEN,
    MAX_AUTOMOD_PATTERN_LEN,
    MAX_AUTOMOD_RULES,
    MAX_MUTE_SECONDS,
    MIN_MUTE_SECONDS,
    VOICE_QUALITY_PRESETS,
    MAX_VOICE_BITRATE
  } from '$lib/chat/constants';
  import {
    chatSettings,
    requestChatSettings,
    setChatSettings
  } from '$lib/stores/chatSettings';
  import {
    automodRules,
    clampMuteSeconds,
    requestAutomodRules,
    setAutomodRules,
    type AutomodRule
  } from '$lib/stores/automod';
  import { bans } from '$lib/stores/bans';
  import { auditLog } from '$lib/stores/auditLog';
  import {
    auditActionLabel,
    auditTargetIsMember,
    AUDIT_ACTOR_ADMIN_TOKEN
  } from '$lib/chat/audit';
  import { invites, inviteSpent, type InviteEntry } from '$lib/stores/invites';
  import { servers } from '$lib/stores/servers';
  import { createInviteLink } from '$lib/invite';
  import { isWebClient } from '$lib/platform';
  import { voiceDefaults, setVoiceDefaults } from '$lib/stores/voiceDefaults';
  import { storageUsage, formatBytes } from '$lib/stores/storageUsage';
  import {
    serverMetrics,
    formatUptime,
    formatMs,
    METRICS_POLL_INTERVAL_MS
  } from '$lib/stores/serverMetrics';
  import { uploadConfig, setUploadConfig } from '$lib/stores/uploadConfig';
  import { stats, statsConfig } from '$lib/stores/stats';
  import { serverIdentity } from '$lib/stores/serverIdentity';
  import {
    screenShareServerMaxBitrate,
    setServerScreenShareMaxBitrate
  } from '$lib/stores/screenShare';
  import { roleDefinitions } from '$lib/stores/roleDefinitions';
  import { myTopPosition } from '$lib/stores/permissions';
  import {
    PERMISSIONS,
    hasPermission,
    PERMISSION_GROUPS
  } from '$lib/chat/permissions';
  import type { Message, RoleDef } from '$lib/types';

  interface Props {
    open: boolean;
    close: () => void;
    /** The viewer's effective permission mask; gates which tabs are visible. */
    permissions: number;
  }

  let { open, close, permissions }: Props = $props();

  // Each server-wide topic lives on its own tab, gated by the permission it
  // controls so a role only sees what it can act on.
  const TABS = [
    { id: 'overview', label: 'Overview', perm: PERMISSIONS.MANAGE_SERVER },
    { id: 'health', label: 'Health', perm: PERMISSIONS.MANAGE_SERVER },
    { id: 'emojis', label: 'Emojis', perm: PERMISSIONS.MANAGE_EMOJIS },
    { id: 'moderation', label: 'Moderation', perm: PERMISSIONS.BAN_MEMBERS },
    { id: 'invites', label: 'Invites', perm: PERMISSIONS.CREATE_INVITES },
    { id: 'audit', label: 'Audit Log', perm: PERMISSIONS.VIEW_AUDIT_LOG },
    { id: 'stats', label: 'Stats', perm: PERMISSIONS.MANAGE_SERVER },
    { id: 'uploads', label: 'Files & Uploads', perm: PERMISSIONS.MANAGE_SERVER },
    { id: 'voice', label: 'Voice', perm: PERMISSIONS.MANAGE_SERVER },
    { id: 'screenshare', label: 'Screen Share', perm: PERMISSIONS.MANAGE_SERVER },
    { id: 'roles', label: 'Roles', perm: PERMISSIONS.MANAGE_ROLES },
    { id: 'danger', label: 'Danger Zone', perm: PERMISSIONS.ADMINISTRATOR }
  ] as const;
  let activeTab: (typeof TABS)[number]['id'] = $state('emojis');

  let visibleTabs = $derived(TABS.filter((tab) => hasPermission(permissions, tab.perm)));
  // If the active tab disappears (e.g. the viewer's role changed), fall back
  // to the first visible one.
  $effect(() => {
    if (visibleTabs.length > 0 && !visibleTabs.some((tab) => tab.id === activeTab)) {
      activeTab = visibleTabs[0].id;
    }
  });

  let httpBase = $derived($selectedServer ? httpBaseFromWs($selectedServer) : '');

  // ── Server identity (Overview tab) ─────────────────────────────────────────
  let identityName = $state('');
  let identityDescription = $state('');
  let identityWelcome = $state('');
  let identityFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);
  /** Set after sending a save; cleared (with feedback) once the broadcast
      confirms the change or an error frame arrives. */
  let identitySavePending = $state(false);
  let iconFileInput: HTMLInputElement | null = $state(null);
  let iconUploading = $state(false);

  // (Re-)fill the form whenever the dashboard opens; while it is open,
  // incoming identity broadcasts must not clobber what the user is typing.
  $effect(() => {
    if (open) {
      untrack(() => {
        identityName = $serverIdentity?.name ?? '';
        identityDescription = $serverIdentity?.description ?? '';
        identityWelcome = $serverIdentity?.welcomeMessage ?? '';
        identityFeedback = null;
        identitySavePending = false;
      });
    }
  });

  let identityDirty = $derived.by(() => {
    const current = $serverIdentity;
    return (
      identityName.trim() !== (current?.name ?? '') ||
      identityDescription.trim() !== (current?.description ?? '') ||
      identityWelcome.trim() !== (current?.welcomeMessage ?? '')
    );
  });

  // A broadcast matching the submitted values confirms the save.
  $effect(() => {
    if (identitySavePending && !identityDirty) {
      identitySavePending = false;
      identityFeedback = { text: 'Changes saved.', kind: 'info' };
    }
  });

  function saveIdentity() {
    const current = $serverIdentity;
    const fields: { name?: string; description?: string; welcomeMessage?: string } = {};
    const name = identityName.trim();
    const description = identityDescription.trim();
    const welcome = identityWelcome.trim();
    if (name !== (current?.name ?? '')) fields.name = name;
    if (description !== (current?.description ?? '')) fields.description = description;
    if (welcome !== (current?.welcomeMessage ?? '')) fields.welcomeMessage = welcome;
    if (Object.keys(fields).length === 0) return;
    identityFeedback = null;
    identitySavePending = true;
    // Role-checked server-side; the confirmation arrives as a broadcast
    // server-identity frame which updates the store.
    serverIdentity.save(fields);
  }

  async function uploadIcon(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0] ?? null;
    input.value = '';
    if (!file || !httpBase) return;
    identityFeedback = null;
    if (file.size > MAX_SERVER_ICON_BYTES) {
      identityFeedback = { text: 'Server icons must be 1 MB or smaller.', kind: 'error' };
      return;
    }
    iconUploading = true;
    try {
      const res = await fetch(httpBase + '/upload', { method: 'POST', body: uploadForm(file) });
      const uploadError = uploadErrorMessage(res.status, 'image');
      if (uploadError) {
        identityFeedback = { text: uploadError, kind: 'error' };
        return;
      }
      if (!res.ok) throw new Error(`upload failed with status ${res.status}`);
      const data = await res.json();
      if (typeof data.url !== 'string') throw new Error('upload response missing url');
      // Registration is role-checked server-side, like emoji registration.
      serverIdentity.save({ icon: data.url });
    } catch (e) {
      console.error('server icon upload failed', e);
      identityFeedback = { text: 'Icon upload failed. Please try again.', kind: 'error' };
    } finally {
      iconUploading = false;
    }
  }

  function removeIcon() {
    identityFeedback = null;
    serverIdentity.save({ icon: null });
  }

  // ── Screen share bitrate cap (Screen Share tab) ───────────────────────────
  let screenShareCapMbps = $state(0);
  let screenShareFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);
  /** Set after sending a save; cleared once the broadcast confirms the
      change or an error frame arrives. */
  let screenShareSavePending = $state(false);

  // (Re-)fill the field whenever the dashboard opens; while it is open,
  // incoming broadcasts must not clobber what the user is typing.
  $effect(() => {
    if (open) {
      untrack(() => {
        screenShareCapMbps = ($screenShareServerMaxBitrate ?? 0) / 1_000_000;
        screenShareFeedback = null;
        screenShareSavePending = false;
      });
    }
  });

  let screenShareCapDirty = $derived(
    Math.round((Number.isFinite(screenShareCapMbps) ? screenShareCapMbps : 0) * 1_000_000) !==
      ($screenShareServerMaxBitrate ?? 0)
  );

  // A broadcast matching the submitted value confirms the save.
  $effect(() => {
    if (screenShareSavePending && !screenShareCapDirty) {
      screenShareSavePending = false;
      screenShareFeedback = { text: 'Changes saved.', kind: 'info' };
    }
  });

  function saveScreenShareCap() {
    const mbps = Number.isFinite(screenShareCapMbps) ? screenShareCapMbps : NaN;
    if (Number.isNaN(mbps) || mbps < 0 || mbps > 100 || (mbps > 0 && mbps < 0.1)) {
      screenShareFeedback = {
        text: 'Enter a value between 0.1 and 100 Mbps, or 0 for no limit.',
        kind: 'error'
      };
      return;
    }
    screenShareFeedback = null;
    screenShareSavePending = true;
    // Role-checked server-side; the confirmation arrives as a broadcast
    // screenshare-config frame which updates the store.
    setServerScreenShareMaxBitrate(mbps > 0 ? Math.round(mbps * 1_000_000) : null);
  }

  // ── Upload policy (Files & Uploads tab) ────────────────────────────────────
  const BYTES_PER_MB = 1024 * 1024;
  let uploadMaxMb = $state(0);
  let uploadCategories: string[] = $state([]);
  let uploadFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);
  /** Set after sending a save; cleared once the broadcast confirms the
      change or an error frame arrives. */
  let uploadSavePending = $state(false);

  // (Re-)fill the form whenever the dashboard opens; while it is open,
  // incoming broadcasts must not clobber what the user is editing.
  $effect(() => {
    if (open) {
      untrack(() => {
        uploadMaxMb = $uploadConfig.maxBytes / BYTES_PER_MB;
        uploadCategories = [...$uploadConfig.categories];
        uploadFeedback = null;
        uploadSavePending = false;
      });
    }
  });

  let uploadMaxBytesDraft = $derived(
    Math.round((Number.isFinite(uploadMaxMb) ? uploadMaxMb : 0) * BYTES_PER_MB)
  );
  let uploadDirty = $derived(
    uploadMaxBytesDraft !== $uploadConfig.maxBytes ||
      uploadCategories.length !== $uploadConfig.categories.length ||
      !$uploadConfig.categories.every((id) => uploadCategories.includes(id))
  );

  // A broadcast matching the submitted values confirms the save.
  $effect(() => {
    if (uploadSavePending && !uploadDirty) {
      uploadSavePending = false;
      uploadFeedback = { text: 'Changes saved.', kind: 'info' };
    }
  });

  function toggleUploadCategory(id: string) {
    uploadCategories = uploadCategories.includes(id)
      ? uploadCategories.filter((entry) => entry !== id)
      : [...uploadCategories, id];
  }

  function saveUploadConfig() {
    if (
      uploadMaxBytesDraft < MIN_UPLOAD_MAX_BYTES ||
      uploadMaxBytesDraft > MAX_UPLOAD_MAX_BYTES
    ) {
      uploadFeedback = {
        text: `Enter a size between ${MIN_UPLOAD_MAX_BYTES / 1024} KB and ${
          MAX_UPLOAD_MAX_BYTES / BYTES_PER_MB
        } MB.`,
        kind: 'error'
      };
      return;
    }
    uploadFeedback = null;
    uploadSavePending = true;
    // Role-checked server-side; the confirmation arrives as a broadcast
    // upload-config frame which updates the store.
    setUploadConfig(uploadMaxBytesDraft, uploadCategories);
  }

  // ── Chat policy (Moderation tab) ───────────────────────────────────────────
  // Slow mode, the message length cap and the profanity filter. Editing needs
  // MANAGE_SERVER, which the Moderation tab itself does not (it opens for
  // BAN_MEMBERS so moderators can work the ban list), so the settings block
  // is gated separately.
  let canManageServer = $derived(hasPermission(permissions, PERMISSIONS.MANAGE_SERVER));

  let slowModeSeconds = $state(0);
  let maxMessageLength = $state(MAX_MESSAGE_LENGTH);
  let profanityFilter = $state(false);
  /** The word list as edited: one word per line. */
  let profanityWords = $state('');
  let chatFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);
  let chatSavePending = $state(false);

  /** Split the editor's text into normalized words the way the server does. */
  function parseWordList(raw: string): string[] {
    const words: string[] = [];
    for (const entry of raw.split(/[\n,]/)) {
      const word = entry.trim().toLowerCase();
      if (!word || word.length > MAX_PROFANITY_WORD_LEN || /\s/.test(word)) continue;
      if (!words.includes(word)) words.push(word);
      if (words.length === MAX_PROFANITY_WORDS) break;
    }
    return words;
  }

  let chatWordsDraft = $derived(parseWordList(profanityWords));

  // (Re-)fill the form from the server's answer. `words === null` means the
  // list has not been disclosed yet, so the editor stays empty and disabled
  // rather than offering to save an empty list over a real one.
  function loadChatSettings() {
    const current = $chatSettings;
    slowModeSeconds = current.slowModeSeconds;
    maxMessageLength = current.maxMessageLength;
    profanityFilter = current.profanityFilter;
    profanityWords = (current.words ?? []).join('\n');
    chatFeedback = null;
    chatSavePending = false;
  }

  /** Whether the manager-only word list has been taken into the editor. */
  let wordsLoaded = $state(false);

  // Ask for the full policy (word list included) whenever the tab is opened,
  // and fill the form with what is already known.
  $effect(() => {
    if (open && activeTab === 'moderation') {
      untrack(() => {
        if (canManageServer) {
          requestChatSettings();
          requestAutomodRules();
        }
        bans.refresh();
        wordsLoaded = false;
        loadChatSettings();
        automodLoaded = false;
        automodDraft = [];
        automodFeedback = null;
        automodSavePending = false;
      });
    }
  });

  // The word list only travels in the answer to that request, which lands
  // after the form was filled — take it once, then leave the editor alone so
  // a later broadcast cannot clobber what the user is typing.
  $effect(() => {
    const words = $chatSettings.words;
    if (!open || activeTab !== 'moderation' || words === null) return;
    untrack(() => {
      if (wordsLoaded) return;
      wordsLoaded = true;
      profanityWords = words.join('\n');
    });
  });

  // A broadcast matching the submitted values confirms the save.
  $effect(() => {
    if (chatSavePending && !chatDirty) {
      chatSavePending = false;
      chatFeedback = { text: 'Changes saved.', kind: 'info' };
    }
  });

  let chatDirty = $derived.by(() => {
    const current = $chatSettings;
    const words = current.words ?? [];
    return (
      slowModeSeconds !== current.slowModeSeconds ||
      maxMessageLength !== current.maxMessageLength ||
      profanityFilter !== current.profanityFilter ||
      chatWordsDraft.length !== words.length ||
      chatWordsDraft.some((word, index) => word !== words[index])
    );
  });

  function saveChatSettings() {
    const seconds = Number.isFinite(slowModeSeconds) ? Math.round(slowModeSeconds) : NaN;
    if (Number.isNaN(seconds) || seconds < 0 || seconds > MAX_SLOW_MODE_SECONDS) {
      chatFeedback = {
        text: `Slow mode must be between 0 and ${MAX_SLOW_MODE_SECONDS} seconds.`,
        kind: 'error'
      };
      return;
    }
    const length = Number.isFinite(maxMessageLength) ? Math.round(maxMessageLength) : NaN;
    if (
      Number.isNaN(length) ||
      length < MIN_CONFIGURABLE_MESSAGE_LENGTH ||
      length > MAX_MESSAGE_LENGTH
    ) {
      chatFeedback = {
        text: `The message limit must be between ${MIN_CONFIGURABLE_MESSAGE_LENGTH} and ${MAX_MESSAGE_LENGTH} characters.`,
        kind: 'error'
      };
      return;
    }
    chatFeedback = null;
    chatSavePending = true;
    setChatSettings({
      slowModeSeconds: seconds,
      maxMessageLength: length,
      profanityFilter,
      words: chatWordsDraft
    });
  }

  /** Render a slow mode interval the way people talk about it. */
  function describeInterval(seconds: number): string {
    if (seconds <= 0) return 'off';
    if (seconds < 60) return `${seconds}s`;
    if (seconds < 3600) {
      const minutes = Math.round(seconds / 60);
      return `${minutes} minute${minutes === 1 ? '' : 's'}`;
    }
    const hours = seconds / 3600;
    return `${Number.isInteger(hours) ? hours : hours.toFixed(1)} hour${hours === 1 ? '' : 's'}`;
  }

  function formatBanDate(value: string | null): string {
    if (!value) return 'unknown date';
    const parsed = new Date(value);
    return Number.isNaN(parsed.getTime()) ? 'unknown date' : parsed.toLocaleDateString();
  }

  async function liftBan(user: string) {
    const ok = await dialogs.confirm({
      title: `Unban ${user}?`,
      message: 'They will be able to join this server again.',
      confirmLabel: 'Unban'
    });
    if (!ok) return;
    bans.unban(user);
  }

  // ── Audit log (Audit Log tab) ─────────────────────────────────────────
  // A record rather than live state: it is fetched when the tab opens and on
  // demand, never kept in step with events. Nothing here is editable — the
  // entries are written by the server as the actions happen.
  let auditFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);

  $effect(() => {
    if (open && activeTab === 'audit') {
      untrack(() => {
        auditFeedback = null;
        auditLog.refresh();
      });
    }
  });

  // ── Invites (Invites tab) ──────────────────────────────────────────────────

  /** Lifetimes offered by the picker, in seconds. 0 is "never expires". */
  const INVITE_EXPIRY_OPTIONS = [
    { seconds: 30 * 60, label: '30 minutes' },
    { seconds: 6 * 60 * 60, label: '6 hours' },
    { seconds: 24 * 60 * 60, label: '1 day' },
    { seconds: 7 * 24 * 60 * 60, label: '7 days' },
    { seconds: 30 * 24 * 60 * 60, label: '30 days' },
    { seconds: 0, label: 'Never' }
  ];
  /** Use limits offered by the picker. 0 is "no limit". */
  const INVITE_USE_OPTIONS = [1, 5, 10, 25, 50, 100, 0];

  let inviteExpiry = $state(24 * 60 * 60);
  let inviteMaxUses = $state(1);
  let inviteFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);
  /** Which code's link was last copied, for the button's transient label. */
  let copiedInvite: string | null = $state(null);
  let inviteCopyTimeout: ReturnType<typeof setTimeout> | null = null;

  $effect(() => {
    if (open && activeTab === 'invites') {
      untrack(() => {
        inviteFeedback = null;
        invites.refresh();
      });
    }
  });

  function refreshAuditLog() {
    auditFeedback = null;
    auditLog.refresh();
  }

  /** Full date and time: an audit entry's value is knowing exactly when. */
  function formatAuditDate(value: string | null): string {
    if (!value) return 'unknown time';
    const parsed = new Date(value);
    return Number.isNaN(parsed.getTime()) ? 'unknown time' : parsed.toLocaleString();
  }

  /**
   * How to label whoever acted. The `/role` endpoint's sentinel is not an
   * account, so it must skip the nickname lookup — that lookup would leave it
   * alone today, but a member named after it is exactly the confusion the
   * sentinel's parentheses exist to prevent.
   */
  function auditActorLabel(actor: string): string {
    if (!actor) return 'unknown';
    if (actor === AUDIT_ACTOR_ADMIN_TOKEN) return actor;
    return $displayNames(actor);
  }

  /**
   * A target is only run through the nickname lookup when the action says it
   * is a member. A role or a channel that shares a name with somebody would
   * otherwise be relabelled as that person.
   */
  function auditTargetLabel(action: string, target: string): string {
    return auditTargetIsMember(action) ? $displayNames(target) : target;
  }

  // ── Health (Health tab) ───────────────────────────────────────────────
  // The only live readout in the dashboard, so the only one that polls. The
  // counters are cumulative; two samples are what turn them into a rate, so
  // the first refresh after opening the tab still shows averages since the
  // server started rather than nothing at all.
  $effect(() => {
    if (!open || activeTab !== 'health') return;
    serverMetrics.refresh();
    const timer = setInterval(() => serverMetrics.refresh(), METRICS_POLL_INTERVAL_MS);
    return () => clearInterval(timer);
  });

  /** Thousands separators: these counters reach seven figures on a busy day. */
  function formatCount(value: number): string {
    return value.toLocaleString();
  }

  // ── Auto-moderation rules (Moderation tab) ───────────────────────────
  // The general form of the word list above: a pattern plus what to do about
  // it. Same gate as the rest of the chat policy (MANAGE_SERVER), and the
  // same shape — the draft below is local, the server's answer is the truth,
  // and a rule the server refuses must never end up looking saved.
  let automodDraft: AutomodRule[] = $state([]);
  let automodFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);
  let automodSavePending = $state(false);
  /** Whether the manager-only rule list has been taken into the editor. */
  let automodLoaded = $state(false);

  // The rules only travel in the answer to `get-automod-rules`, which lands
  // after the tab opened — take them once, then leave the editor alone so a
  // later answer cannot clobber a half-written rule.
  $effect(() => {
    const rules = $automodRules;
    if (!open || activeTab !== 'moderation' || rules === null) return;
    untrack(() => {
      if (automodLoaded) return;
      automodLoaded = true;
      automodDraft = rules.map((rule) => ({ ...rule }));
    });
  });

  let automodDirty = $derived.by(() => {
    const current = $automodRules;
    if (current === null) return false;
    if (current.length !== automodDraft.length) return true;
    return automodDraft.some((rule, index) => {
      const stored = current[index];
      return (
        rule.name.trim() !== stored.name ||
        rule.pattern.trim() !== stored.pattern ||
        rule.kind !== stored.kind ||
        rule.action !== stored.action ||
        rule.enabled !== stored.enabled ||
        clampMuteSeconds(rule.muteSeconds) !== stored.muteSeconds
      );
    });
  });

  // The server's answer matching the draft confirms the save.
  $effect(() => {
    if (automodSavePending && !automodDirty) {
      automodSavePending = false;
      automodFeedback = { text: 'Rules saved.', kind: 'info' };
    }
  });

  function addAutomodRule() {
    if (automodDraft.length >= MAX_AUTOMOD_RULES) return;
    automodDraft = [
      ...automodDraft,
      {
        name: '',
        pattern: '',
        kind: 'word',
        action: 'delete',
        muteSeconds: DEFAULT_AUTOMOD_MUTE_SECONDS,
        enabled: true
      }
    ];
    automodFeedback = null;
  }

  function removeAutomodRule(index: number) {
    automodDraft = automodDraft.filter((_, position) => position !== index);
    automodFeedback = null;
  }

  /**
   * Why the draft cannot be saved, or null. Cosmetic — the server checks all
   * of it again, including whether a regular expression compiles, which only
   * it can answer.
   */
  function describeAutomodProblem(rules: AutomodRule[]): string | null {
    for (const [index, rule] of rules.entries()) {
      const position = index + 1;
      const pattern = rule.pattern.trim();
      if (!pattern) return `Rule ${position} has no pattern.`;
      if (pattern.length > MAX_AUTOMOD_PATTERN_LEN) {
        return `Rule ${position} has a pattern longer than ${MAX_AUTOMOD_PATTERN_LEN} characters.`;
      }
      if (rule.name.trim().length > MAX_AUTOMOD_NAME_LEN) {
        return `Rule ${position} has a name longer than ${MAX_AUTOMOD_NAME_LEN} characters.`;
      }
      // A whole-word pattern is compared against single words, so one with a
      // space in it would be saved and then never match anything.
      if (rule.kind === 'word' && /\s/.test(pattern)) {
        return `Rule ${position} matches a whole word, so its pattern cannot contain spaces.`;
      }
    }
    return null;
  }

  function saveAutomodRules() {
    const problem = describeAutomodProblem(automodDraft);
    if (problem) {
      automodFeedback = { text: problem, kind: 'error' };
      return;
    }
    automodFeedback = null;
    automodSavePending = true;
    // Role-checked server-side; the confirmation is the automod-rules answer.
    setAutomodRules(
      automodDraft.map((rule) => ({
        ...rule,
        name: rule.name.trim(),
        pattern: rule.pattern.trim(),
        muteSeconds: clampMuteSeconds(rule.muteSeconds)
      }))
    );
  }

  /** Render a mute duration the way people talk about it. */
  function describeMute(seconds: number): string {
    const clamped = clampMuteSeconds(seconds);
    if (clamped < 60) return `${clamped}s`;
    if (clamped < 3600) return `${Math.round(clamped / 60)} minutes`;
    if (clamped < 86_400) {
      const hours = clamped / 3600;
      return `${Number.isInteger(hours) ? hours : hours.toFixed(1)} hours`;
    }
    const days = clamped / 86_400;
    return `${Number.isInteger(days) ? days : days.toFixed(1)} days`;
  }

  function describeInviteUses(invite: InviteEntry): string {
    return invite.maxUses > 0 ? `${invite.uses}/${invite.maxUses} uses` : `${invite.uses} uses`;
  }

  function describeInviteExpiry(invite: InviteEntry): string {
    if (!invite.expiresAt) return 'never expires';
    const parsed = new Date(invite.expiresAt);
    if (Number.isNaN(parsed.getTime())) return 'unknown expiry';
    return `${parsed <= new Date() ? 'expired' : 'expires'} ${parsed.toLocaleString()}`;
  }

  /**
   * The shareable link for a code. It points at the web client we are running
   * in, or — in the desktop app, which has no origin of its own — at the
   * server, which is where a server started with `WEB_CLIENT_DIR` serves the
   * client from.
   */
  function inviteLink(code: string): string | null {
    const url = $selectedServer;
    if (!url) return null;
    const entry = servers.get(url) ?? { url, name: url };
    return createInviteLink(entry, isWebClient ? location.origin : undefined, code);
  }

  async function copyInviteLink(code: string) {
    const link = inviteLink(code);
    if (!link) return;
    try {
      await navigator.clipboard.writeText(link);
      copiedInvite = code;
      if (inviteCopyTimeout) clearTimeout(inviteCopyTimeout);
      inviteCopyTimeout = setTimeout(() => {
        if (copiedInvite === code) copiedInvite = null;
        inviteCopyTimeout = null;
      }, 2000);
    } catch (err) {
      inviteFeedback = { text: 'Could not copy the link. Copy the code instead.', kind: 'error' };
      if (import.meta.env.DEV) {
        console.error('Failed to copy invite link', err);
      }
    }
  }

  function createInvite() {
    inviteFeedback = null;
    invites.create(inviteExpiry, inviteMaxUses);
  }

  async function revokeInvite(code: string) {
    const ok = await dialogs.confirm({
      title: 'Revoke this invite?',
      message:
        'The link stops working immediately. Members who already joined through it keep their access — remove one of those with a ban.',
      confirmLabel: 'Revoke'
    });
    if (!ok) return;
    inviteFeedback = null;
    invites.revoke(code);
  }

  // ── Voice defaults (Voice tab) ─────────────────────────────────────────────
  let voiceQuality = $state('');
  let voiceBitrateKbps = $state(0);
  let voiceFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);
  let voiceSavePending = $state(false);

  // (Re-)fill the form whenever the dashboard opens; while it is open,
  // incoming broadcasts must not clobber what the user is editing.
  $effect(() => {
    if (open) {
      untrack(() => {
        voiceQuality = $voiceDefaults.quality;
        voiceBitrateKbps = ($voiceDefaults.bitrate ?? 0) / 1000;
        voiceFeedback = null;
        voiceSavePending = false;
      });
    }
  });

  let voiceBitrateDraft = $derived(
    Math.round((Number.isFinite(voiceBitrateKbps) ? voiceBitrateKbps : 0) * 1000)
  );
  let voiceDirty = $derived(
    voiceQuality !== $voiceDefaults.quality ||
      (voiceBitrateDraft > 0 ? voiceBitrateDraft : null) !== $voiceDefaults.bitrate
  );

  // A broadcast matching the submitted values confirms the save.
  $effect(() => {
    if (voiceSavePending && !voiceDirty) {
      voiceSavePending = false;
      voiceFeedback = { text: 'Changes saved.', kind: 'info' };
    }
  });

  /** Picking a preset fills in its bitrate; 0 kbps means "lossless". */
  function pickVoicePreset(event: Event) {
    const quality = (event.currentTarget as HTMLSelectElement).value;
    voiceQuality = quality;
    const preset = VOICE_QUALITY_PRESETS.find((entry) => entry.quality === quality);
    if (preset) voiceBitrateKbps = (preset.bitrate ?? 0) / 1000;
  }

  function saveVoiceDefaults() {
    if (!voiceQuality.trim()) {
      voiceFeedback = { text: 'Pick a quality preset.', kind: 'error' };
      return;
    }
    if (voiceBitrateDraft < 0 || voiceBitrateDraft > MAX_VOICE_BITRATE) {
      voiceFeedback = {
        text: `Enter a bitrate up to ${MAX_VOICE_BITRATE / 1000} kbps, or 0 for uncompressed.`,
        kind: 'error'
      };
      return;
    }
    voiceFeedback = null;
    voiceSavePending = true;
    // Role-checked server-side; the confirmation arrives as a broadcast
    // voice-defaults frame which updates the store.
    setVoiceDefaults(voiceQuality.trim(), voiceBitrateDraft > 0 ? voiceBitrateDraft : null);
  }

  // ── Storage usage (Files & Uploads tab) ───────────────────────────────────
  // Measured on request rather than tracked, so it is asked for when the tab
  // is opened and by the refresh button.
  $effect(() => {
    if (open && activeTab === 'uploads') {
      untrack(() => storageUsage.refresh());
    }
  });

  const CATEGORY_LABELS: Record<string, string> = {
    ...Object.fromEntries(UPLOAD_CATEGORIES.map((category) => [category.id, category.label])),
    // Files whose extension left the safe-list still occupy disk.
    other: 'Other'
  };

  // ── Danger Zone ────────────────────────────────────────────────────────────
  let dangerFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);

  /**
   * Both Danger Zone actions are irreversible, so the phrase typed here is
   * also what the server requires in the frame — a stray click cannot wipe a
   * server, and neither can a frame replayed without it.
   */
  async function confirmDestructive(
    title: string,
    message: string,
    phrase: string
  ): Promise<boolean> {
    const typed = await dialogs.prompt({
      title,
      message: `${message}\n\nType ${phrase} to confirm.`,
      label: 'Confirmation',
      placeholder: phrase,
      confirmLabel: 'Continue'
    });
    if (typed === null) return false;
    if (typed.trim() !== phrase) {
      dangerFeedback = { text: 'That did not match — nothing was changed.', kind: 'error' };
      return false;
    }
    return true;
  }

  async function purgeMessages() {
    dangerFeedback = null;
    const confirmed = await confirmDestructive(
      'Purge all messages',
      'Every message, pin and reaction on this server is deleted for everyone. This cannot be undone.',
      'PURGE'
    );
    if (!confirmed) return;
    chat.sendRaw({ type: 'purge-all-messages', confirm: 'PURGE' });
    dangerFeedback = { text: 'Purge requested.', kind: 'info' };
  }

  async function resetServer() {
    dangerFeedback = null;
    const confirmed = await confirmDestructive(
      'Reset server',
      'Every channel except general, plus all categories, custom roles, wiki pages and messages are deleted. Members, bans and emojis are kept. This cannot be undone.',
      'RESET'
    );
    if (!confirmed) return;
    chat.sendRaw({ type: 'reset-server', confirm: 'RESET' });
    dangerFeedback = { text: 'Reset requested.', kind: 'info' };
  }

  // ── Custom emoji management ────────────────────────────────────────────────
  let emojiName = $state('');
  let emojiFile: File | null = $state(null);
  let emojiFileInput: HTMLInputElement | null = $state(null);
  let uploading = $state(false);
  let emojiFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);

  let normalizedEmojiName = $derived(emojiName.trim().toLowerCase());
  let emojiNameValid = $derived(EMOJI_NAME_RE.test(normalizedEmojiName));
  let emojiNameTaken = $derived(emojiNameValid && normalizedEmojiName in $customEmojis);
  let emojiFileTooLarge = $derived.by(() => emojiFile !== null && emojiFile.size > MAX_EMOJI_FILE_BYTES);
  let canUploadEmoji =
    $derived(!uploading && emojiNameValid && !emojiNameTaken && emojiFile !== null && !emojiFileTooLarge);

  const EMOJI_ERROR_CODES = new Set([
    'emoji-permission-denied',
    'invalid-emoji-name',
    'invalid-emoji-url',
    'emoji-name-taken',
    'emoji-limit-reached',
    'emoji-update-failed',
    'emoji-not-found'
  ]);

  const IDENTITY_ERROR_CODES = new Set([
    'identity-permission-denied',
    'invalid-server-name',
    'invalid-server-description',
    'invalid-welcome-message',
    'invalid-server-icon',
    'identity-update-failed'
  ]);

  const SCREENSHARE_ERROR_CODES = new Set([
    'screenshare-permission-denied',
    'invalid-screenshare-bitrate',
    'screenshare-update-failed'
  ]);

  const UPLOAD_ERROR_CODES = new Set([
    'upload-permission-denied',
    'invalid-upload-config',
    'upload-config-update-failed'
  ]);

  const CHAT_SETTINGS_ERROR_CODES = new Set([
    'chat-settings-permission-denied',
    'invalid-chat-settings',
    'chat-settings-update-failed'
  ]);

  // `automod-blocked` is deliberately absent: that one answers a chat message,
  // not anything this dashboard sent.
  const AUTOMOD_ERROR_CODES = new Set([
    'automod-permission-denied',
    'invalid-automod-rules',
    'invalid-automod-pattern',
    'automod-update-failed'
  ]);

  const MODERATION_ERROR_CODES = new Set([
    'moderation-permission-denied',
    'moderation-target-not-found',
    'moderation-target-protected',
    'moderation-failed'
  ]);

  const VOICE_DEFAULTS_ERROR_CODES = new Set([
    'voice-defaults-permission-denied',
    'voice-defaults-update-failed',
    'invalid-voice-quality',
    'invalid-voice-bitrate'
  ]);

  const AUDIT_ERROR_CODES = new Set(['audit-log-permission-denied', 'audit-log-failed']);

  const MAINTENANCE_ERROR_CODES = new Set([
    'maintenance-permission-denied',
    'maintenance-not-confirmed',
    'maintenance-failed'
  ]);

  const INVITE_ERROR_CODES = new Set([
    'invite-permission-denied',
    'invalid-invite-options',
    'invite-limit-reached',
    'invite-not-found',
    'invite-update-failed'
  ]);

  const ROLE_ERROR_CODES = new Set([
    'role-permission-denied',
    'role-target-not-found',
    'role-update-failed',
    'role-not-found',
    'role-name-taken',
    'role-protected',
    'role-limit-reached',
    'invalid-role-name',
    'invalid-role-color',
    'invalid-role-icon',
    'invalid-role-permissions'
  ]);

  function handleServerError(msg: Message) {
    const code = (msg as any).message;
    if (!open || typeof code !== 'string') return;
    if (EMOJI_ERROR_CODES.has(code)) {
      emojiFeedback = { text: describeServerError(code), kind: 'error' };
    } else if (IDENTITY_ERROR_CODES.has(code)) {
      identitySavePending = false;
      identityFeedback = { text: describeServerError(code), kind: 'error' };
    } else if (SCREENSHARE_ERROR_CODES.has(code)) {
      screenShareSavePending = false;
      screenShareFeedback = { text: describeServerError(code), kind: 'error' };
    } else if (UPLOAD_ERROR_CODES.has(code)) {
      uploadSavePending = false;
      uploadFeedback = { text: describeServerError(code), kind: 'error' };
    } else if (AUTOMOD_ERROR_CODES.has(code)) {
      automodSavePending = false;
      automodFeedback = { text: describeServerError(code), kind: 'error' };
    } else if (CHAT_SETTINGS_ERROR_CODES.has(code) || MODERATION_ERROR_CODES.has(code)) {
      chatSavePending = false;
      chatFeedback = { text: describeServerError(code), kind: 'error' };
    } else if (VOICE_DEFAULTS_ERROR_CODES.has(code)) {
      voiceSavePending = false;
      voiceFeedback = { text: describeServerError(code), kind: 'error' };
    } else if (AUDIT_ERROR_CODES.has(code)) {
      auditFeedback = { text: describeServerError(code), kind: 'error' };
    } else if (MAINTENANCE_ERROR_CODES.has(code)) {
      dangerFeedback = { text: describeServerError(code), kind: 'error' };
    } else if (INVITE_ERROR_CODES.has(code)) {
      inviteFeedback = { text: describeServerError(code), kind: 'error' };
    } else if (ROLE_ERROR_CODES.has(code)) {
      roleErrorFeedback(code);
    }
  }

  onMount(() => chat.on('error', handleServerError));
  onDestroy(() => {
    chat.off('error', handleServerError);
    if (inviteCopyTimeout) clearTimeout(inviteCopyTimeout);
  });

  function handleEmojiFileChange(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    emojiFile = input.files?.[0] ?? null;
    emojiFeedback = null;
  }

  async function uploadEmoji() {
    if (!canUploadEmoji || !emojiFile || !httpBase) return;
    uploading = true;
    emojiFeedback = null;
    const name = normalizedEmojiName;
    try {
      const res = await fetch(httpBase + '/upload', {
        method: 'POST',
        body: uploadForm(emojiFile)
      });
      const uploadError = uploadErrorMessage(res.status, 'image');
      if (uploadError) {
        emojiFeedback = { text: uploadError, kind: 'error' };
        return;
      }
      if (!res.ok) throw new Error(`upload failed with status ${res.status}`);
      const data = await res.json();
      if (typeof data.url !== 'string') throw new Error('upload response missing url');
      // Registration is role-checked server-side; success arrives as an
      // updated emoji-list broadcast, errors as an error frame handled above.
      chat.sendRaw({ type: 'add-emoji', name, url: data.url });
      emojiName = '';
      emojiFile = null;
      if (emojiFileInput) emojiFileInput.value = '';
    } catch (e) {
      console.error('emoji upload failed', e);
      emojiFeedback = { text: 'Emoji upload failed. Please try again.', kind: 'error' };
    } finally {
      uploading = false;
    }
  }

  async function deleteEmoji(name: string) {
    const ok = await dialogs.confirm({
      title: 'Delete emoji',
      message: `Remove :${name}: from this server? Existing reactions will show the shortcode as text.`,
      confirmLabel: 'Delete',
      danger: true
    });
    if (!ok) return;
    chat.sendRaw({ type: 'remove-emoji', name });
  }

  function toggleServerStats(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    // Role-checked server-side; the confirmation arrives as a broadcast
    // stats-config frame which updates the store (and this checkbox).
    stats.setServerEnabled(input.checked);
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      close();
    }
  }

  function handleOverlayKeydown(event: KeyboardEvent) {
    // Keyboard activation of the focused backdrop only; keystrokes inside
    // the modal content (inputs, buttons) bubble here and must not close it.
    if (event.target !== event.currentTarget) return;
    if (event.key === 'Enter' || event.key === ' ') {
      close();
    }
  }

  // ── Roles (Roles tab) ──────────────────────────────────────────────────────
  // Roles are listed highest-power first (Owner on top, @everyone at the
  // bottom). Editing is bounded by the viewer's own position; the server
  // enforces the same rules, so these gates are cosmetic.
  let selectedRoleId: number | null = $state(null);
  let draftName = $state('');
  let draftColor = $state('');
  /** Role icon URL (`/files/<key>`), empty when the role has none. */
  let draftIcon = $state('');
  let draftPermissions = $state(0);
  let roleFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);
  let roleIconInput: HTMLInputElement | null = $state(null);
  let roleIconUploading = $state(false);

  let rolesHighToLow = $derived([...$roleDefinitions].sort((a, b) => b.position - a.position));
  let selectedRole = $derived(
    $roleDefinitions.find((r) => r.id === selectedRoleId) ?? null
  );
  // Custom (reorderable) roles, highest-power first.
  let customRolesHighToLow = $derived(
    rolesHighToLow.filter((r) => !r.isDefault && !r.isOwner)
  );

  function canManageRole(role: RoleDef): boolean {
    return $myTopPosition > role.position;
  }
  let nameEditable = $derived(!!selectedRole && canManageRole(selectedRole) && !selectedRole.isDefault);
  let permsEditable = $derived(!!selectedRole && canManageRole(selectedRole) && !selectedRole.isOwner);
  let deletable = $derived(
    !!selectedRole && canManageRole(selectedRole) && !selectedRole.isDefault && !selectedRole.isOwner
  );

  // Load the selected role into the editable draft whenever the selection (or
  // the underlying definition) changes.
  $effect(() => {
    const role = selectedRole;
    if (role) {
      untrack(() => {
        draftName = role.name;
        draftColor = role.color ?? '';
        draftIcon = role.icon ?? '';
        draftPermissions = role.permissions;
      });
    }
  });

  // Default to the highest role the viewer can see when the tab opens.
  $effect(() => {
    if (activeTab === 'roles' && selectedRoleId === null && rolesHighToLow.length) {
      untrack(() => {
        selectedRoleId = rolesHighToLow[0].id;
      });
    }
  });

  function selectRole(id: number) {
    selectedRoleId = id;
    roleFeedback = null;
  }

  function togglePermission(flag: number) {
    if (!permsEditable) return;
    draftPermissions ^= flag;
  }

  function isPermissionOn(flag: number): boolean {
    if ((draftPermissions & PERMISSIONS.ADMINISTRATOR) !== 0) return true;
    return (draftPermissions & flag) === flag;
  }

  function saveRole() {
    if (!selectedRole) return;
    const color = draftColor.trim();
    const icon = draftIcon.trim();
    chat.sendRaw({
      type: 'update-role',
      id: selectedRole.id,
      name: draftName.trim(),
      color: color === '' ? null : color,
      icon: icon === '' ? null : icon,
      permissions: draftPermissions >>> 0
    });
    roleFeedback = { text: 'Changes sent.', kind: 'info' };
  }

  /**
   * Upload an image and stage it as this role's icon. Like the emoji and
   * server-icon flows the file goes through `/upload` first; the URL is only
   * registered when the role is saved, and the server re-validates it.
   */
  async function uploadRoleIcon(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0] ?? null;
    input.value = '';
    if (!file || !httpBase) return;
    roleFeedback = null;
    if (file.size > MAX_ROLE_ICON_BYTES) {
      roleFeedback = { text: 'Role icons must be 512 KB or smaller.', kind: 'error' };
      return;
    }
    roleIconUploading = true;
    try {
      const res = await fetch(httpBase + '/upload', { method: 'POST', body: uploadForm(file) });
      const uploadError = uploadErrorMessage(res.status, 'image');
      if (uploadError) {
        roleFeedback = { text: uploadError, kind: 'error' };
        return;
      }
      if (!res.ok) throw new Error(`upload failed with status ${res.status}`);
      const data = await res.json();
      if (typeof data.url !== 'string') throw new Error('upload response missing url');
      draftIcon = data.url;
      roleFeedback = { text: 'Icon ready — save to apply it.', kind: 'info' };
    } catch (e) {
      console.error('role icon upload failed', e);
      roleFeedback = { text: 'Icon upload failed. Please try again.', kind: 'error' };
    } finally {
      roleIconUploading = false;
    }
  }

  // Picking a custom emoji just reuses its uploaded image, so no second copy
  // of the file is created.
  function pickEmojiIcon(event: Event) {
    const select = event.currentTarget as HTMLSelectElement;
    const url = select.value;
    select.value = '';
    if (!url) return;
    draftIcon = url;
    roleFeedback = { text: 'Icon ready — save to apply it.', kind: 'info' };
  }

  async function createRole() {
    const name = await dialogs.prompt({
      title: 'Create role',
      message: 'Name the new role. You can set its permissions after creating it.',
      placeholder: 'e.g. Dude',
      confirmLabel: 'Create'
    });
    if (name === null) return;
    const trimmed = name.trim();
    if (!trimmed) return;
    // New roles start with the baseline view + send permissions.
    chat.sendRaw({
      type: 'create-role',
      name: trimmed,
      permissions: (PERMISSIONS.VIEW_CHANNELS | PERMISSIONS.SEND_MESSAGES) >>> 0
    });
  }

  async function deleteRole() {
    if (!selectedRole || !deletable) return;
    const confirmed = await dialogs.confirm({
      title: `Delete “${selectedRole.name}”?`,
      message: 'Members lose this role. This cannot be undone.',
      confirmLabel: 'Delete role',
      danger: true
    });
    if (!confirmed) return;
    chat.sendRaw({ type: 'delete-role', id: selectedRole.id });
    selectedRoleId = null;
  }

  // Move a custom role up (more power) or down; sends the new order to the
  // server, which re-derives positions.
  function moveRole(role: RoleDef, direction: -1 | 1) {
    const order = customRolesHighToLow.map((r) => r.id);
    const index = order.indexOf(role.id);
    const target = index + direction;
    if (index < 0 || target < 0 || target >= order.length) return;
    [order[index], order[target]] = [order[target], order[index]];
    chat.sendRaw({ type: 'reorder-roles', orderedIds: order });
  }

  function roleErrorFeedback(code: string) {
    roleFeedback = { text: describeServerError(code), kind: 'error' };
  }
</script>

{#if open}
  <div class="modal-overlay" onclick={close} onkeydown={handleOverlayKeydown} role="dialog" aria-modal="true" aria-labelledby="server-dashboard-title" tabindex="-1">
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="modal-content" onclick={(event) => event.stopPropagation()} onkeydown={handleKeydown} role="document" tabindex="0">
      <div class="modal-header">
        <h2 id="server-dashboard-title">Server Dashboard</h2>
        <button class="icon-btn close-btn" onclick={close} aria-label="Close server dashboard">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>

      <div class="settings-layout">
        <nav class="settings-tabs" aria-label="Server dashboard sections">
          {#each visibleTabs as tab}
            <button
              class="tab-btn"
              class:selected={activeTab === tab.id}
              aria-pressed={activeTab === tab.id}
              onclick={() => (activeTab = tab.id)}
            >{tab.label}</button>
          {/each}
        </nav>

        <div class="modal-body">
        {#if activeTab === 'overview'}
          <div class="settings-section">
            <h3 class="section-title">Server Identity</h3>
            <div class="setting-group">
              <span class="setting-label">Server name</span>
              <input
                type="text"
                bind:value={identityName}
                placeholder="My Murmer Server"
                maxlength={MAX_SERVER_NAME_LENGTH}
                disabled={$serverIdentity === null}
              />
              <div class="setting-description">The name shown to members of this server.</div>
            </div>
            <div class="setting-group">
              <span class="setting-label">Description</span>
              <textarea
                rows="2"
                bind:value={identityDescription}
                placeholder="What this server is about…"
                maxlength={MAX_SERVER_DESCRIPTION_LENGTH}
                disabled={$serverIdentity === null}
              ></textarea>
              <div class="setting-description">A short description shown in the server list.</div>
            </div>
            <div class="setting-group">
              <span class="setting-label">Welcome message</span>
              <input
                type="text"
                bind:value={identityWelcome}
                placeholder="Welcome to the server!"
                maxlength={MAX_WELCOME_MESSAGE_LENGTH}
                disabled={$serverIdentity === null}
              />
              <div class="setting-description">
                Shown to new members the first time they connect. Leave empty to disable.
              </div>
            </div>
            <div class="setting-group">
              <div>
                <button
                  class="btn btn-primary"
                  onclick={saveIdentity}
                  disabled={!identityDirty || $serverIdentity === null}
                >Save changes</button>
              </div>
              {#if $serverIdentity === null}
                <div class="setting-description">Waiting for the server…</div>
              {/if}
              {#if identityFeedback}
                <div class="identity-feedback" class:error={identityFeedback.kind === 'error'}>
                  {identityFeedback.text}
                </div>
              {/if}
            </div>
            <div class="setting-group">
              <span class="setting-label">Server icon</span>
              <div class="icon-row">
                {#if $serverIdentity?.icon}
                  <img
                    class="icon-preview"
                    src={httpBase + $serverIdentity.icon}
                    alt="Server icon"
                    width="48"
                    height="48"
                  />
                {/if}
                <input
                  bind:this={iconFileInput}
                  type="file"
                  accept="image/png,image/jpeg,image/gif,image/webp"
                  class="sr-only"
                  onchange={uploadIcon}
                />
                <button
                  class="btn"
                  onclick={() => iconFileInput?.click()}
                  disabled={iconUploading || $serverIdentity === null}
                >
                  {iconUploading
                    ? 'Uploading…'
                    : $serverIdentity?.icon
                      ? 'Replace icon…'
                      : 'Upload icon…'}
                </button>
                {#if $serverIdentity?.icon}
                  <button class="btn btn-danger" onclick={removeIcon} disabled={iconUploading}>
                    Remove
                  </button>
                {/if}
              </div>
              <div class="setting-description">
                Shown in the server list of every member. Images up to 1 MB (PNG, JPEG, GIF or WebP).
              </div>
            </div>
          </div>

          <div class="settings-section">
            <h3 class="section-title">Online now ({$onlineUsers.length})</h3>
            <div class="setting-group">
              <div class="setting-description">
                Members connected to this server right now. Right-click a member in the sidebar
                to moderate them or change their roles.
              </div>
              {#if $onlineUsers.length === 0}
                <div class="setting-description">Nobody is connected.</div>
              {:else}
                <ul class="online-list">
                  {#each [...$onlineUsers].sort((a, b) => a.localeCompare(b)) as user (user)}
                    <li class="online-row">
                      <span class="online-dot" aria-hidden="true"></span>
                      <span class="online-name">{$displayNames(user)}</span>
                      {#if $displayNames(user) !== user}
                        <span class="online-account">{user}</span>
                      {/if}
                    </li>
                  {/each}
                </ul>
              {/if}
            </div>
          </div>
        {/if}

        {#if activeTab === 'health'}
          <div class="settings-section">
            <h3 class="section-title">Health</h3>
            <div class="setting-group">
              <div class="setting-description">
                What this server is doing right now. The counters are kept in memory and
                start again from zero at every restart, so they describe the current run
                and nothing before it. Rates are measured across refreshes, which happen
                every {METRICS_POLL_INTERVAL_MS / 1000} seconds while this tab is open.
              </div>
              {#if $serverMetrics === null}
                <div class="setting-description">Waiting for the server…</div>
              {:else}
                <ul class="metric-grid">
                  <li class="metric">
                    <span class="metric-value">{formatCount($serverMetrics.connections)}</span>
                    <span class="metric-label">Connections open</span>
                    <span class="metric-note">
                      peak {formatCount($serverMetrics.peakConnections)} this run
                    </span>
                  </li>
                  <li class="metric">
                    <span class="metric-value">
                      {$serverMetrics.framesPerSecond.toFixed(1)}<span class="metric-unit">/s</span>
                    </span>
                    <span class="metric-label">Frames received</span>
                    <span class="metric-note">
                      {formatCount($serverMetrics.frames)} total
                    </span>
                  </li>
                  <li class="metric">
                    <span class="metric-value">
                      {$serverMetrics.dbAverageMs === null
                        ? '—'
                        : formatMs($serverMetrics.dbAverageMs)}
                    </span>
                    <span class="metric-label">Database call</span>
                    <span class="metric-note">
                      {$serverMetrics.dbAverageMs === null ? 'idle · ' : ''}worst
                      {formatMs($serverMetrics.dbMaxMs)} ·
                      {formatCount($serverMetrics.dbCalls)} calls
                    </span>
                  </li>
                  <li class="metric">
                    <span class="metric-value">{formatUptime($serverMetrics.uptimeSeconds)}</span>
                    <span class="metric-label">Uptime</span>
                    <span class="metric-note">since the last restart</span>
                  </li>
                </ul>
              {/if}
            </div>

            <div class="setting-group">
              <span class="setting-label">Rate-limit rejections</span>
              <div class="setting-description">
                Requests turned away since the server started. A climbing auth count is
                somebody guessing keys; climbing messages or uploads is either a member
                flooding or a limit set too low for the room. All four limits are set by
                the server's environment, not from here.
              </div>
              {#if $serverMetrics === null}
                <div class="setting-description">Waiting for the server…</div>
              {:else}
                <ul class="storage-list">
                  {#each [
                    { label: 'Messages', value: $serverMetrics.rejectedMessages },
                    { label: 'Authentication', value: $serverMetrics.rejectedAuth },
                    { label: 'Uploads', value: $serverMetrics.rejectedUploads },
                    { label: 'Replayed signatures', value: $serverMetrics.rejectedReplays }
                  ] as row (row.label)}
                    <li class="storage-row">
                      <span class="storage-label">{row.label}</span>
                      <span class="storage-value">{formatCount(row.value)}</span>
                    </li>
                  {/each}
                </ul>
              {/if}
            </div>
          </div>
        {/if}

        {#if activeTab === 'emojis'}
          <div class="settings-section">
            <h3 class="section-title">Custom Emojis</h3>
            <div class="setting-group">
              <div class="setting-description">
                Upload custom emojis for everyone on this server. They can be used as
                reactions via the emoji picker. Images up to 512 KB (PNG, JPEG, GIF or WebP).
              </div>
              <form class="emoji-form" onsubmit={(event) => { event.preventDefault(); uploadEmoji(); }}>
                <label class="field emoji-name-field">
                  <span>Name</span>
                  <input
                    type="text"
                    bind:value={emojiName}
                    placeholder="party_parrot"
                    maxlength="32"
                    spellcheck="false"
                    autocomplete="off"
                  />
                </label>
                <label class="field">
                  <span>Image</span>
                  <input
                    bind:this={emojiFileInput}
                    type="file"
                    accept="image/png,image/jpeg,image/gif,image/webp"
                    onchange={handleEmojiFileChange}
                  />
                </label>
                <button class="btn btn-primary" type="submit" disabled={!canUploadEmoji}>
                  {uploading ? 'Uploading…' : 'Upload'}
                </button>
              </form>
              {#if emojiName && !emojiNameValid}
                <div class="emoji-hint">Names use 2-32 lowercase letters, digits or underscores.</div>
              {:else if emojiNameTaken}
                <div class="emoji-hint">An emoji with this name already exists.</div>
              {:else if emojiFileTooLarge}
                <div class="emoji-hint">Emoji images must be 512 KB or smaller.</div>
              {/if}
              {#if emojiFeedback}
                <div class="emoji-feedback" class:error={emojiFeedback.kind === 'error'}>
                  {emojiFeedback.text}
                </div>
              {/if}
            </div>

            <div class="setting-group">
              {#if $customEmojiList.length === 0}
                <div class="setting-description">No custom emojis yet.</div>
              {:else}
                <ul class="emoji-list">
                  {#each $customEmojiList as emoji (emoji.name)}
                    <li class="emoji-row">
                      <img src={httpBase + emoji.url} alt={`:${emoji.name}:`} width="24" height="24" loading="lazy" />
                      <span class="emoji-code">:{emoji.name}:</span>
                      {#if emoji.uploadedBy}
                        <span class="emoji-uploader">by {emoji.uploadedBy}</span>
                      {/if}
                      <button
                        class="icon-btn danger"
                        title={`Delete :${emoji.name}:`}
                        aria-label={`Delete emoji ${emoji.name}`}
                        onclick={() => deleteEmoji(emoji.name)}
                      >
                        <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
                          <path d="M3 6h18"></path>
                          <path d="M8 6V4a1 1 0 0 1 1-1h6a1 1 0 0 1 1 1v2"></path>
                          <path d="M19 6l-1 14a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2L5 6"></path>
                        </svg>
                      </button>
                    </li>
                  {/each}
                </ul>
              {/if}
            </div>
          </div>
        {/if}

        {#if activeTab === 'moderation'}
          <div class="settings-section">
            <h3 class="section-title">Moderation</h3>
            {#if canManageServer}
              <div class="setting-group">
                <label class="setting-label" for="slow-mode">Slow mode</label>
                <input
                  id="slow-mode"
                  type="number"
                  bind:value={slowModeSeconds}
                  min="0"
                  max={MAX_SLOW_MODE_SECONDS}
                  step="1"
                />
                <div class="setting-description">
                  Seconds each member must wait between messages ({describeInterval(
                    slowModeSeconds
                  )}). 0 turns slow mode off. Members who can manage messages are exempt.
                </div>
              </div>
              <div class="setting-group">
                <label class="setting-label" for="max-message-length">Max message length</label>
                <input
                  id="max-message-length"
                  type="number"
                  bind:value={maxMessageLength}
                  min={MIN_CONFIGURABLE_MESSAGE_LENGTH}
                  max={MAX_MESSAGE_LENGTH}
                  step="1"
                />
                <div class="setting-description">
                  Characters per message, between {MIN_CONFIGURABLE_MESSAGE_LENGTH} and
                  {MAX_MESSAGE_LENGTH}. The server rejects anything longer, so this can only
                  tighten the built-in limit.
                </div>
              </div>
              <div class="setting-group">
                <label class="toggle-row">
                  <input type="checkbox" bind:checked={profanityFilter} />
                  <span class="toggle-text">
                    <span class="toggle-label">Profanity filter</span>
                    <span class="toggle-description">
                      Replace filtered words with asterisks in new messages. Masking happens on
                      the server before a message is stored, so the original never reaches
                      anyone — including whoever edits it afterwards.
                    </span>
                  </span>
                </label>
              </div>
              <div class="setting-group">
                <label class="setting-label" for="profanity-words">Filtered words</label>
                <textarea
                  id="profanity-words"
                  rows="4"
                  spellcheck="false"
                  autocomplete="off"
                  bind:value={profanityWords}
                  placeholder="One word per line"
                ></textarea>
                <div class="setting-description">
                  One word per line, up to {MAX_PROFANITY_WORDS} words of
                  {MAX_PROFANITY_WORD_LEN} characters. Matching is per whole word and ignores
                  case, so filtering “ass” leaves “class” alone.
                  {chatWordsDraft.length}
                  {chatWordsDraft.length === 1 ? 'word' : 'words'} will be saved.
                </div>
              </div>
              <div class="setting-group">
                <div>
                  <button class="btn btn-primary" onclick={saveChatSettings} disabled={!chatDirty}>
                    Save changes
                  </button>
                </div>
                {#if chatFeedback}
                  <div class="identity-feedback" class:error={chatFeedback.kind === 'error'}>
                    {chatFeedback.text}
                  </div>
                {/if}
              </div>

              <div class="setting-group">
                <span class="setting-label">Auto-moderation rules</span>
                <div class="setting-description">
                  Patterns the server checks every message and every edit against, before it is
                  stored or sent on. When more than one rule matches, the most severe action wins.
                  Members who can manage messages are exempt, and rules never apply in an
                  end-to-end encrypted channel — the server has no text to read there.
                </div>
                {#if $automodRules === null}
                  <div class="setting-description">Loading…</div>
                {:else}
                  {#if automodDraft.length === 0}
                    <div class="setting-description">No rules yet.</div>
                  {/if}
                  <ul class="rule-list">
                    <!-- Keyed by position: a rule has no id of its own, the
                         list is saved and stored as one, and its order is what
                         decides between two equally severe matches. -->
                    {#each automodDraft as rule, index (index)}
                      <li class="rule-row">
                        <div class="rule-head">
                          <label class="rule-enabled">
                            <input type="checkbox" bind:checked={rule.enabled} />
                            <span>Enabled</span>
                          </label>
                          <input
                            class="rule-name"
                            type="text"
                            bind:value={rule.name}
                            maxlength={MAX_AUTOMOD_NAME_LEN}
                            placeholder="Name (shown to whoever trips it)"
                            aria-label={`Name of rule ${index + 1}`}
                          />
                          <button
                            class="btn btn-danger"
                            onclick={() => removeAutomodRule(index)}
                            aria-label={`Remove rule ${index + 1}`}
                          >
                            Remove
                          </button>
                        </div>
                        <div class="rule-fields">
                          <select bind:value={rule.kind} aria-label={`Match kind of rule ${index + 1}`}>
                            {#each AUTOMOD_KINDS as kind}
                              <option value={kind.id}>{kind.label}</option>
                            {/each}
                          </select>
                          <input
                            class="rule-pattern"
                            type="text"
                            spellcheck="false"
                            autocomplete="off"
                            bind:value={rule.pattern}
                            maxlength={MAX_AUTOMOD_PATTERN_LEN}
                            placeholder="Pattern"
                            aria-label={`Pattern of rule ${index + 1}`}
                          />
                          <select bind:value={rule.action} aria-label={`Action of rule ${index + 1}`}>
                            {#each AUTOMOD_ACTIONS as action}
                              <option value={action.id}>{action.label}</option>
                            {/each}
                          </select>
                          {#if rule.action === 'mute'}
                            <input
                              class="rule-mute"
                              type="number"
                              bind:value={rule.muteSeconds}
                              min={MIN_MUTE_SECONDS}
                              max={MAX_MUTE_SECONDS}
                              step="1"
                              aria-label={`Mute duration of rule ${index + 1} in seconds`}
                            />
                          {/if}
                        </div>
                        <div class="setting-description">
                          {AUTOMOD_KINDS.find((kind) => kind.id === rule.kind)?.description}
                          {AUTOMOD_ACTIONS.find((action) => action.id === rule.action)?.description}
                          {#if rule.action === 'mute'}
                            Muted for {describeMute(rule.muteSeconds)}.
                          {/if}
                        </div>
                      </li>
                    {/each}
                  </ul>
                  <div class="rule-actions">
                    <button
                      class="btn"
                      onclick={addAutomodRule}
                      disabled={automodDraft.length >= MAX_AUTOMOD_RULES}
                    >
                      Add rule
                    </button>
                    <button
                      class="btn btn-primary"
                      onclick={saveAutomodRules}
                      disabled={!automodDirty}
                    >
                      Save rules
                    </button>
                    <span class="setting-description">
                      {automodDraft.length} of {MAX_AUTOMOD_RULES}
                    </span>
                  </div>
                  {#if automodFeedback}
                    <div class="identity-feedback" class:error={automodFeedback.kind === 'error'}>
                      {automodFeedback.text}
                    </div>
                  {/if}
                {/if}
              </div>
            {/if}

            <div class="setting-group">
              <span class="setting-label">Ban list</span>
              <div class="setting-description">
                Everyone banned from this server. A ban follows the member's key, so it holds
                even if they come back under a different name.
              </div>
              {#if $bans === null}
                <div class="setting-description">Loading…</div>
              {:else if $bans.length === 0}
                <div class="setting-description">Nobody is banned.</div>
              {:else}
                <ul class="ban-list">
                  <!-- Keyed by both fields: a ban is stored per key, but a
                       row the server sent without one must not collide with
                       another and break the list. -->
                  {#each $bans as ban (`${ban.publicKey}:${ban.user}`)}
                    <li class="ban-row">
                      <span class="ban-name">{$displayNames(ban.user)}</span>
                      <span class="ban-meta">
                        {ban.bannedBy ? `by ${$displayNames(ban.bannedBy)} · ` : ''}{formatBanDate(
                          ban.bannedAt
                        )}
                      </span>
                      <button class="btn" onclick={() => liftBan(ban.user)}>Unban</button>
                    </li>
                  {/each}
                </ul>
              {/if}
              {#if !canManageServer && chatFeedback}
                <div class="identity-feedback" class:error={chatFeedback.kind === 'error'}>
                  {chatFeedback.text}
                </div>
              {/if}
            </div>
          </div>
        {/if}

        {#if activeTab === 'invites'}
          <div class="settings-section">
            <h3 class="section-title">Invites</h3>
            <div class="setting-group">
              <div class="setting-description">
                An invite link lets somebody join without being told the server password. Each
                one can expire and can be limited to a number of uses, so a link that leaks can
                be withdrawn on its own — changing the password for everybody is not the only
                way out. Revoking an invite stops new joins; members who already used it stay,
                and are removed with a ban.
              </div>
            </div>

            <div class="setting-group">
              <span class="setting-label">Create an invite</span>
              <div class="invite-form">
                <label class="invite-field">
                  <span>Expires after</span>
                  <select bind:value={inviteExpiry}>
                    {#each INVITE_EXPIRY_OPTIONS as option (option.seconds)}
                      <option value={option.seconds}>{option.label}</option>
                    {/each}
                  </select>
                </label>
                <label class="invite-field">
                  <span>Max uses</span>
                  <select bind:value={inviteMaxUses}>
                    {#each INVITE_USE_OPTIONS as uses (uses)}
                      <option value={uses}>{uses === 0 ? 'No limit' : uses}</option>
                    {/each}
                  </select>
                </label>
                <button class="btn btn-primary" onclick={createInvite}>Create invite</button>
              </div>
              {#if inviteFeedback}
                <div class="identity-feedback" class:error={inviteFeedback.kind === 'error'}>
                  {inviteFeedback.text}
                </div>
              {/if}
            </div>

            <div class="setting-group">
              <span class="setting-label">Active invites</span>
              {#if $invites === null}
                <div class="setting-description">Loading…</div>
              {:else if $invites.length === 0}
                <div class="setting-description">No invites yet.</div>
              {:else}
                <ul class="invite-list">
                  {#each $invites as invite (invite.code)}
                    <li class="invite-row" class:spent={inviteSpent(invite)}>
                      <div class="invite-identity">
                        <code class="invite-code">{invite.code}</code>
                        <span class="invite-meta">
                          {describeInviteUses(invite)} · {describeInviteExpiry(invite)}{invite.createdBy
                            ? ` · by ${$displayNames(invite.createdBy)}`
                            : ''}
                        </span>
                      </div>
                      <button class="btn" onclick={() => copyInviteLink(invite.code)}>
                        {copiedInvite === invite.code ? 'Copied!' : 'Copy link'}
                      </button>
                      <button class="btn" onclick={() => revokeInvite(invite.code)}>Revoke</button>
                    </li>
                  {/each}
                </ul>
              {/if}
            </div>
          </div>
        {/if}

        {#if activeTab === 'audit'}
          <div class="settings-section">
            <h3 class="section-title">Audit Log</h3>
            <div class="setting-group">
              <div class="setting-description">
                Who kicked, banned or muted a member, who changed a role or a channel's
                permissions, and who ran a Danger Zone action. Written by the server as each
                action succeeds — a refused action is not an action and leaves no entry. The log
                survives a server reset on purpose, and the oldest entries are dropped once it
                fills up.
              </div>
              <div class="rule-actions">
                <button class="btn" onclick={refreshAuditLog}>Refresh</button>
                {#if $auditLog !== null}
                  <span class="setting-description">
                    {$auditLog.length}
                    {$auditLog.length === 1 ? 'entry' : 'entries'}
                  </span>
                {/if}
              </div>
              {#if $auditLog === null}
                <div class="setting-description">Loading…</div>
              {:else if $auditLog.length === 0}
                <div class="setting-description">Nothing has been recorded yet.</div>
              {:else}
                <ul class="audit-list">
                  {#each $auditLog as entry (entry.id)}
                    <li class="audit-row">
                      <span class="audit-action">{auditActionLabel(entry.action)}</span>
                      <span class="audit-meta">
                        {auditActorLabel(entry.actor)}{entry.target
                          ? ` → ${auditTargetLabel(entry.action, entry.target)}`
                          : ''}{entry.detail ? ` · ${entry.detail}` : ''}
                      </span>
                      <span class="audit-time">{formatAuditDate(entry.at)}</span>
                    </li>
                  {/each}
                </ul>
              {/if}
              {#if auditFeedback}
                <div class="identity-feedback" class:error={auditFeedback.kind === 'error'}>
                  {auditFeedback.text}
                </div>
              {/if}
            </div>
          </div>
        {/if}

        {#if activeTab === 'stats'}
          <div class="settings-section">
            <h3 class="section-title">Lifetime Stats</h3>
            <div class="setting-group">
              <label class="toggle-row">
                <input
                  type="checkbox"
                  checked={$statsConfig?.serverEnabled ?? false}
                  disabled={$statsConfig === null}
                  onchange={toggleServerStats}
                />
                <span class="toggle-text">
                  <span class="toggle-label">Allow stat tracking on this server</span>
                  <span class="toggle-description">
                    Lets members record lifetime stats (messages sent, voice minutes, reactions, …)
                    and unlock achievements. Tracking is double opt-in: even with this enabled,
                    nothing is recorded for a member until they opt in themselves in their own
                    settings. Only aggregate counters are stored — never message contents.
                  </span>
                </span>
              </label>
              {#if $statsConfig === null}
                <div class="setting-description">Waiting for the server…</div>
              {/if}
            </div>
            <div class="setting-group">
              <div class="setting-description">
                Turning this off stops all recording immediately. Already recorded stats are kept
                but hidden until tracking is enabled again; each member can delete their own
                recorded stats at any time from Settings → Stats &amp; Privacy.
              </div>
            </div>
          </div>
        {/if}

        {#if activeTab === 'uploads'}
          <div class="settings-section">
            <h3 class="section-title">Files &amp; Uploads</h3>
            <div class="setting-group">
              <label class="setting-label" for="upload-max-size">Max upload size</label>
              <input
                id="upload-max-size"
                type="number"
                bind:value={uploadMaxMb}
                min={MIN_UPLOAD_MAX_BYTES / (1024 * 1024)}
                max={MAX_UPLOAD_MAX_BYTES / (1024 * 1024)}
                step="1"
              />
              <div class="setting-description">
                Maximum size in MB for a single file, between 0.0625 MB (64 KB) and
                {MAX_UPLOAD_MAX_BYTES / (1024 * 1024)} MB. Uploads are stored on the server's
                disk, so raising this raises the space members can consume.
              </div>
            </div>
            <div class="setting-group">
              <span class="setting-label">Allowed file types</span>
              <div class="setting-description">
                Which upload categories members may attach. Active content (HTML, SVG, scripts)
                is always rejected and cannot be enabled. Custom emojis, server icons and
                avatars are uploaded as images too — turning <strong>Images</strong> off blocks
                those as well.
              </div>
              {#each UPLOAD_CATEGORIES as category (category.id)}
                <label class="toggle-row">
                  <input
                    type="checkbox"
                    checked={uploadCategories.includes(category.id)}
                    onchange={() => toggleUploadCategory(category.id)}
                  />
                  <span class="toggle-text">
                    <span class="toggle-label">{category.label}</span>
                    <span class="toggle-description upload-extensions">
                      {category.extensions.map((ext) => `.${ext}`).join(' ')}
                    </span>
                  </span>
                </label>
              {/each}
              {#if uploadCategories.length === 0}
                <div class="setting-description">
                  With no category enabled, members cannot attach files at all.
                </div>
              {/if}
            </div>
            <div class="setting-group">
              <div>
                <button
                  class="btn btn-primary"
                  onclick={saveUploadConfig}
                  disabled={!uploadDirty}
                >Save changes</button>
              </div>
              {#if uploadFeedback}
                <div class="identity-feedback" class:error={uploadFeedback.kind === 'error'}>
                  {uploadFeedback.text}
                </div>
              {/if}
            </div>

            <div class="setting-group">
              <span class="setting-label">Storage used</span>
              <div class="setting-description">
                What the server's upload directory holds right now. Measured when asked rather
                than counted as files arrive, so it also covers emojis, avatars and soundboard
                clips. Deleting a message or an emoji does not delete its file.
              </div>
              {#if $storageUsage === null}
                <div class="setting-description">Measuring…</div>
              {:else}
                <div class="storage-total">
                  {formatBytes($storageUsage.totalBytes)}
                  <span class="storage-files">
                    across {$storageUsage.fileCount}
                    {$storageUsage.fileCount === 1 ? 'file' : 'files'}
                  </span>
                </div>
                {#if $storageUsage.categories.length > 0}
                  <ul class="storage-list">
                    {#each $storageUsage.categories as category (category.id)}
                      <li class="storage-row">
                        <span class="storage-label">
                          {CATEGORY_LABELS[category.id] ?? category.id}
                        </span>
                        <span class="storage-bar" aria-hidden="true">
                          <span
                            class="storage-fill"
                            style={`width: ${
                              $storageUsage.totalBytes > 0
                                ? Math.max(2, (category.bytes / $storageUsage.totalBytes) * 100)
                                : 0
                            }%`}
                          ></span>
                        </span>
                        <span class="storage-value">
                          {formatBytes(category.bytes)}
                          <span class="storage-files">({category.files})</span>
                        </span>
                      </li>
                    {/each}
                  </ul>
                {/if}
              {/if}
              <div>
                <button class="btn" onclick={() => storageUsage.refresh()}>Refresh</button>
              </div>
            </div>
          </div>
        {/if}

        {#if activeTab === 'voice'}
          <div class="settings-section">
            <h3 class="section-title">Voice</h3>
            <div class="setting-description">
              What a newly created voice channel starts with. These are defaults, not a cap:
              existing channels keep their own setting, and whoever creates a channel can pick
              a different preset.
            </div>
            <div class="setting-group">
              <label class="setting-label" for="voice-default-quality">Default quality</label>
              <select id="voice-default-quality" value={voiceQuality} onchange={pickVoicePreset}>
                {#each VOICE_QUALITY_PRESETS as preset (preset.quality)}
                  <option value={preset.quality}>{preset.label}</option>
                {/each}
                {#if !VOICE_QUALITY_PRESETS.some((preset) => preset.quality === voiceQuality)}
                  <!-- A server may be configured with a label this build does
                       not know; keep it selectable instead of silently
                       switching it to something else. -->
                  <option value={voiceQuality}>{voiceQuality}</option>
                {/if}
              </select>
              <div class="setting-description">
                Quality preset assigned to new voice channels. Picking one fills in its bitrate.
              </div>
            </div>
            <div class="setting-group">
              <label class="setting-label" for="voice-default-bitrate">Default bitrate</label>
              <input
                id="voice-default-bitrate"
                type="number"
                bind:value={voiceBitrateKbps}
                min="0"
                max={MAX_VOICE_BITRATE / 1000}
                step="1"
              />
              <div class="setting-description">
                Bitrate in kbps for new voice channels, up to {MAX_VOICE_BITRATE / 1000} kbps.
                0 means uncompressed audio.
              </div>
              <div>
                <button class="btn btn-primary" onclick={saveVoiceDefaults} disabled={!voiceDirty}>
                  Save changes
                </button>
              </div>
              {#if voiceFeedback}
                <div class="identity-feedback" class:error={voiceFeedback.kind === 'error'}>
                  {voiceFeedback.text}
                </div>
              {/if}
            </div>
          </div>
        {/if}

        {#if activeTab === 'screenshare'}
          <div class="settings-section">
            <h3 class="section-title">Screen Share</h3>
            <div class="setting-group">
              <span class="setting-label">Screen share max bitrate</span>
              <input
                type="number"
                bind:value={screenShareCapMbps}
                min="0"
                max="100"
                step="0.5"
              />
              <div class="setting-description">
                Cap in Mbps applied to every member's outgoing screen share; 0 means no limit.
                Screen shares travel peer-to-peer between members, so this limits member
                bandwidth use, not server load.
              </div>
              <div>
                <button
                  class="btn btn-primary"
                  onclick={saveScreenShareCap}
                  disabled={!screenShareCapDirty}
                >Save changes</button>
              </div>
              {#if screenShareFeedback}
                <div class="identity-feedback" class:error={screenShareFeedback.kind === 'error'}>
                  {screenShareFeedback.text}
                </div>
              {/if}
            </div>
          </div>
        {/if}

        {#if activeTab === 'roles'}
          <div class="settings-section">
            <h3 class="section-title">Roles</h3>
            <div class="setting-description">
              Define roles and their permissions. Assign roles to members by
              right-clicking them in the sidebar. Everyone implicitly has
              <strong>@everyone</strong>; grant capabilities on top of it, or lower
              its baseline to restrict everyone.
            </div>

            <div class="roles-layout">
              <div class="roles-list">
                {#each rolesHighToLow as role (role.id)}
                  <button
                    type="button"
                    class="role-row"
                    class:active={role.id === selectedRoleId}
                    onclick={() => selectRole(role.id)}
                  >
                    <span
                      class="role-dot"
                      style={`background: ${role.color ?? 'var(--color-muted)'}`}
                      aria-hidden="true"
                    ></span>
                    {#if role.icon}
                      <img class="role-row-icon" src={httpBase + role.icon} alt="" />
                    {/if}
                    <span class="role-row-name">{role.name}</span>
                    {#if role.isOwner}<span class="badge">Owner</span>{/if}
                    {#if role.isDefault}<span class="badge">Default</span>{/if}
                  </button>
                {/each}
                <button type="button" class="btn role-create" onclick={createRole}>
                  + Create role
                </button>
              </div>

              <div class="role-editor">
                {#if selectedRole}
                  <div class="setting-group">
                    <label class="setting-label" for="role-name">Name</label>
                    <input
                      id="role-name"
                      class="field"
                      bind:value={draftName}
                      maxlength="32"
                      disabled={!nameEditable}
                    />
                  </div>

                  <div class="setting-group">
                    <label class="setting-label" for="role-color">Color</label>
                    <div class="role-color-row">
                      <input
                        id="role-color"
                        class="field"
                        bind:value={draftColor}
                        placeholder="#3b82f6"
                        disabled={!nameEditable}
                      />
                      <span
                        class="role-dot large"
                        style={`background: ${draftColor.trim() || 'var(--color-muted)'}`}
                        aria-hidden="true"
                      ></span>
                    </div>
                  </div>

                  <div class="setting-group">
                    <span class="setting-label">Icon</span>
                    <div class="setting-description">
                      Shown next to the name of every member holding this role. Pick a
                      custom emoji or upload an image (PNG, JPEG, GIF or WebP, up to
                      512 KB).
                    </div>
                    <div class="role-icon-row">
                      <span class="role-icon-preview">
                        {#if draftIcon}
                          <img src={httpBase + draftIcon} alt="" />
                        {:else}
                          <span class="role-icon-empty">None</span>
                        {/if}
                      </span>
                      <select
                        class="field role-icon-select"
                        disabled={!nameEditable || $customEmojiList.length === 0}
                        onchange={pickEmojiIcon}
                        aria-label="Use a custom emoji as the role icon"
                      >
                        <option value="">
                          {$customEmojiList.length === 0 ? 'No custom emojis' : 'Use an emoji…'}
                        </option>
                        {#each $customEmojiList as emoji (emoji.name)}
                          <option value={emoji.url}>:{emoji.name}:</option>
                        {/each}
                      </select>
                      <button
                        type="button"
                        class="btn"
                        disabled={!nameEditable || roleIconUploading}
                        onclick={() => roleIconInput?.click()}
                      >{roleIconUploading ? 'Uploading…' : 'Upload image…'}</button>
                      {#if draftIcon}
                        <button
                          type="button"
                          class="btn btn-ghost"
                          disabled={!nameEditable}
                          onclick={() => (draftIcon = '')}
                        >Remove</button>
                      {/if}
                      <input
                        bind:this={roleIconInput}
                        type="file"
                        accept="image/png,image/jpeg,image/gif,image/webp"
                        class="sr-only"
                        onchange={uploadRoleIcon}
                      />
                    </div>
                  </div>

                  <div class="setting-group">
                    <span class="setting-label">Permissions</span>
                    {#if selectedRole.isOwner}
                      <div class="setting-description">
                        The Owner role always has every permission and cannot be changed.
                      </div>
                    {/if}
                    {#each PERMISSION_GROUPS as group (group.title)}
                      <div class="perm-group">
                        <div class="perm-group-title">{group.title}</div>
                        {#each group.permissions as perm (perm.key)}
                          <label class="perm-row">
                            <input
                              type="checkbox"
                              checked={isPermissionOn(perm.flag)}
                              disabled={!permsEditable}
                              onchange={() => togglePermission(perm.flag)}
                            />
                            <span class="perm-text">
                              <span class="perm-label">{perm.label}</span>
                              <span class="perm-desc">{perm.description}</span>
                            </span>
                          </label>
                        {/each}
                      </div>
                    {/each}
                  </div>

                  {#if !selectedRole.isDefault && !selectedRole.isOwner}
                    <div class="setting-group">
                      <span class="setting-label">Position</span>
                      <div class="role-reorder">
                        <button
                          type="button"
                          class="btn"
                          disabled={!canManageRole(selectedRole)}
                          onclick={() => selectedRole && moveRole(selectedRole, -1)}
                        >Move up</button>
                        <button
                          type="button"
                          class="btn"
                          disabled={!canManageRole(selectedRole)}
                          onclick={() => selectedRole && moveRole(selectedRole, 1)}
                        >Move down</button>
                      </div>
                    </div>
                  {/if}

                  {#if roleFeedback}
                    <div class="identity-feedback" class:error={roleFeedback.kind === 'error'}>
                      {roleFeedback.text}
                    </div>
                  {/if}

                  <div class="role-actions">
                    <button
                      type="button"
                      class="btn btn-primary"
                      disabled={!nameEditable && !permsEditable}
                      onclick={saveRole}
                    >Save changes</button>
                    {#if deletable}
                      <button type="button" class="btn btn-danger" onclick={deleteRole}>
                        Delete role
                      </button>
                    {/if}
                  </div>
                {:else}
                  <div class="setting-description">Select a role to edit it.</div>
                {/if}
              </div>
            </div>
          </div>
        {/if}

        {#if activeTab === 'danger'}
          <div class="settings-section">
            <h3 class="section-title">Danger Zone</h3>
            <div class="setting-group">
              <span class="setting-label">Purge all messages</span>
              <div class="setting-description">
                Permanently delete every message, pin and reaction on this server, in every
                channel, for everyone. Channels, members and uploads are kept — the files
                behind deleted attachments stay on disk. This cannot be undone.
              </div>
              <div><button class="btn btn-danger" onclick={purgeMessages}>Purge messages…</button></div>
            </div>
            <div class="setting-group">
              <span class="setting-label">Reset server</span>
              <div class="setting-description">
                Delete every channel except <strong>general</strong>, plus all categories,
                channel permission overrides, wiki pages, messages and every role other than
                <strong>@everyone</strong> and <strong>Owner</strong> — those two stay so the
                server still has an administrator. Members, bans, emojis, sounds and recorded
                stats are kept. This cannot be undone.
              </div>
              <div><button class="btn btn-danger" onclick={resetServer}>Reset server…</button></div>
            </div>
            {#if dangerFeedback}
              <div class="identity-feedback" class:error={dangerFeedback.kind === 'error'}>
                {dangerFeedback.text}
              </div>
            {/if}
          </div>
        {/if}
        </div>
      </div>

      <div class="modal-footer">
        <button class="btn btn-primary" onclick={close}>Done</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-overlay {
    position: fixed;
    inset: 0;
    /* Plain dim instead of a backdrop blur — blur is very expensive in
       WebKitGTK (Linux) while the modal is open. */
    background: var(--color-overlay);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-5);
    z-index: var(--z-modal);
    animation: fadeIn 0.15s ease-out;
  }

  .modal-content {
    background: var(--color-surface-elevated);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    border: 1px solid var(--color-surface-outline);
    width: min(864px, 94vw);
    height: min(672px, 88vh);
    overflow: hidden;
    display: flex;
    flex-direction: column;
    animation: slideIn 0.18s var(--motion-easing-standard);
  }

  .settings-layout {
    display: flex;
    flex: 1;
    min-height: 0;
  }

  .settings-tabs {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-4) var(--space-3);
    border-right: 1px solid var(--color-surface-outline);
    flex-shrink: 0;
    width: 12rem;
    overflow-y: auto;
  }

  .tab-btn {
    text-align: left;
    justify-content: flex-start;
    background: transparent;
    border: none;
    color: var(--color-muted);
    font-weight: 500;
    font-size: var(--text-md);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-md);
  }

  .tab-btn:hover {
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
  }

  .tab-btn.selected {
    background: var(--color-primary-container);
    color: var(--color-primary);
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-5);
    border-bottom: 1px solid var(--color-surface-outline);
  }

  .modal-header h2 {
    font-size: var(--text-lg);
  }

  .modal-body {
    flex: 1;
    min-width: 0;
    padding: var(--space-5);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    padding: var(--space-3) var(--space-5);
    border-top: 1px solid var(--color-surface-outline);
  }

  .settings-section {
    display: grid;
    gap: var(--space-4);
  }

  .section-title {
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--color-muted);
  }

  .setting-group {
    display: grid;
    gap: var(--space-2);
  }

  .setting-label {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-weight: 500;
    color: var(--color-on-surface);
    font-size: var(--text-md);
  }

  .setting-description {
    font-size: var(--text-sm);
    color: var(--color-muted);
    line-height: 1.5;
  }

  .toggle-row {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
  }

  .toggle-row input[type='checkbox'] {
    margin-top: 0.2rem;
    accent-color: var(--color-primary);
    width: 1rem;
    height: 1rem;
    flex-shrink: 0;
  }

  .toggle-text {
    display: grid;
    gap: 0.125rem;
  }

  .toggle-label {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-weight: 500;
    font-size: var(--text-md);
    color: var(--color-on-surface);
  }

  .toggle-description {
    font-size: var(--text-sm);
    color: var(--color-muted);
  }

  .identity-feedback {
    font-size: var(--text-sm);
    color: var(--color-muted);
  }

  .upload-extensions {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    word-break: break-word;
  }

  .identity-feedback.error {
    color: var(--color-warning);
  }

  .icon-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .icon-preview {
    width: 3rem;
    height: 3rem;
    border-radius: var(--radius-md);
    object-fit: cover;
    border: 1px solid var(--color-surface-outline);
    flex-shrink: 0;
  }

  .emoji-form {
    display: flex;
    align-items: flex-end;
    gap: var(--space-3);
    flex-wrap: wrap;
  }

  .emoji-name-field input {
    width: 12rem;
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .emoji-hint,
  .emoji-feedback {
    font-size: var(--text-sm);
    color: var(--color-muted);
  }

  .emoji-feedback.error,
  .emoji-hint {
    color: var(--color-warning);
  }

  .emoji-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: var(--space-1);
  }

  .emoji-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
  }

  .emoji-row:hover {
    background: var(--color-surface-raised);
  }

  .emoji-row img {
    width: 1.5rem;
    height: 1.5rem;
    object-fit: contain;
    flex-shrink: 0;
  }

  .emoji-code {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    color: var(--color-on-surface);
  }

  .emoji-uploader {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .emoji-row .icon-btn {
    flex-shrink: 0;
    margin-left: auto;
  }

  /* Ban list, online members and the storage breakdown share the plain
     list-of-rows shape the emoji list already uses. */
  .rule-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: var(--space-3);
  }

  /* Outlined rather than filled: the inputs inside a row already carry
     `--color-surface-raised`, so a filled card would swallow them. */
  .rule-row {
    display: grid;
    gap: var(--space-2);
    padding: var(--space-3);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-md);
  }

  .rule-head {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .rule-enabled {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
    color: var(--color-on-surface-variant);
    white-space: nowrap;
  }

  .rule-enabled input[type='checkbox'] {
    accent-color: var(--color-primary);
    width: 1rem;
    height: 1rem;
    min-height: 0;
    flex-shrink: 0;
  }

  .rule-name {
    flex: 1 1 12rem;
    min-width: 0;
  }

  /* The pattern gets the room: it is the part that is read character by
     character when a rule does not do what its author expected. */
  .rule-fields {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .rule-pattern {
    flex: 1 1 14rem;
    min-width: 0;
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .rule-mute {
    flex: 0 0 7rem;
  }

  .rule-actions {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
  }

  .invite-form {
    display: flex;
    flex-wrap: wrap;
    align-items: end;
    gap: var(--space-3);
  }

  .invite-field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .invite-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2);
    border-radius: var(--radius-sm);
  }

  .invite-row:hover {
    background: var(--color-surface-raised);
  }

  /* An expired or used-up invite is kept in the list so it can be cleared
     away deliberately, but it should not read as one that still works. */
  .invite-row.spent .invite-code,
  .invite-row.spent .invite-meta {
    opacity: 0.55;
    text-decoration: line-through;
  }

  .invite-identity {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
    margin-right: auto;
  }

  .invite-code {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    color: var(--color-on-surface);
    overflow-wrap: anywhere;
  }

  .invite-meta {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .audit-list,
  .ban-list,
  .online-list,
  .storage-list,
  .invite-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: var(--space-1);
  }

  .ban-row,
  .online-row,
  .storage-row {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
  }

  .ban-row:hover,
  .online-row:hover {
    background: var(--color-surface-raised);
  }

  /* The log is dense and unbounded in width — the timestamp is pushed to the
     end so the eye can scan down it, and the middle column takes the slack.
     Rows carry the raised background all the time rather than on hover: they
     are read, not acted on, and the separation is what keeps a wrapped entry
     from running into the next one. */
  .audit-row {
    display: flex;
    align-items: baseline;
    gap: var(--space-2);
    background: var(--color-surface-raised);
    padding: var(--space-1) var(--space-2);
    border-radius: var(--radius-sm);
  }

  .audit-action {
    color: var(--color-on-surface);
    font-size: var(--text-sm);
    flex-shrink: 0;
  }

  .audit-meta {
    font-size: var(--text-xs);
    color: var(--color-muted);
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .audit-time {
    font-size: var(--text-xs);
    color: var(--color-muted);
    flex-shrink: 0;
  }

  .ban-name,
  .online-name {
    color: var(--color-on-surface);
    font-size: var(--text-sm);
  }

  .ban-meta,
  .online-account {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .ban-row .btn {
    margin-left: auto;
    flex-shrink: 0;
  }

  .online-dot {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: var(--color-success, #22c55e);
    flex-shrink: 0;
  }

  .metric-grid {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(9rem, 1fr));
    gap: var(--space-2);
  }

  .metric {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
    background: var(--color-surface-raised);
  }

  .metric-value {
    font-size: var(--text-lg);
    color: var(--color-on-surface);
    /* The numbers change every few seconds in place; a proportional font
       would make the whole tile shuffle sideways on every refresh. */
    font-variant-numeric: tabular-nums;
  }

  .metric-unit {
    font-size: var(--text-sm);
    color: var(--color-muted);
  }

  .metric-label {
    font-size: var(--text-sm);
    color: var(--color-on-surface);
  }

  .metric-note {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .storage-total {
    font-size: var(--text-lg);
    color: var(--color-on-surface);
  }

  .storage-files {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .storage-label {
    font-size: var(--text-sm);
    color: var(--color-on-surface);
    width: 7rem;
    flex-shrink: 0;
  }

  .storage-bar {
    flex: 1;
    height: 0.5rem;
    border-radius: var(--radius-sm);
    background: var(--color-surface-raised);
    overflow: hidden;
  }

  .storage-fill {
    display: block;
    height: 100%;
    background: var(--color-primary);
  }

  .storage-value {
    font-size: var(--text-sm);
    color: var(--color-muted);
    width: 6.5rem;
    text-align: right;
    flex-shrink: 0;
  }

  @keyframes fadeIn {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }

  @keyframes slideIn {
    from {
      opacity: 0;
      transform: translateY(8px) scale(0.98);
    }
    to {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  /* ── Roles editor ──────────────────────────────────────────────────────── */
  .roles-layout {
    display: grid;
    grid-template-columns: minmax(160px, 220px) minmax(0, 1fr);
    gap: var(--space-4);
    margin-top: var(--space-3);
  }

  .roles-list {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    align-content: start;
  }

  .role-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: transparent;
    color: var(--color-on-surface);
    cursor: pointer;
    text-align: left;
    font: inherit;
  }

  .role-row:hover {
    background: var(--color-surface-raised);
  }

  .role-row.active {
    background: var(--color-surface-raised);
    border-color: var(--color-surface-outline);
  }

  .role-row-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .role-dot {
    width: 12px;
    height: 12px;
    border-radius: var(--radius-pill, 50%);
    flex-shrink: 0;
  }

  .role-row-icon {
    width: var(--space-4);
    height: var(--space-4);
    object-fit: contain;
    flex-shrink: 0;
  }

  .role-icon-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .role-icon-preview {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--space-7);
    height: var(--space-7);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-sm);
    background: var(--color-surface-raised);
    flex-shrink: 0;
  }

  .role-icon-preview img {
    width: var(--space-5);
    height: var(--space-5);
    object-fit: contain;
  }

  .role-icon-empty {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .role-icon-select {
    width: auto;
    min-width: 10rem;
  }

  .role-dot.large {
    width: 20px;
    height: 20px;
  }

  .role-create {
    margin-top: var(--space-2);
    justify-content: center;
  }

  .role-editor {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    min-width: 0;
  }

  .role-color-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .perm-group {
    margin-top: var(--space-2);
  }

  .perm-group-title {
    font-size: var(--text-xs);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--color-muted);
    margin-bottom: var(--space-1);
  }

  .perm-row {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    padding: var(--space-1) 0;
    cursor: pointer;
  }

  .perm-row input[disabled] {
    cursor: not-allowed;
  }

  .perm-text {
    display: flex;
    flex-direction: column;
  }

  .perm-label {
    font-size: var(--text-sm);
    color: var(--color-on-surface);
  }

  .perm-desc {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .role-reorder,
  .role-actions {
    display: flex;
    gap: var(--space-2);
  }

  @media (max-width: 640px) {
    .roles-layout {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
