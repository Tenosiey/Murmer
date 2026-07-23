<!--
  Server management dashboard for moderators and above. Separate from the
  user-facing SettingsModal: everything in here is server-wide state. Tabs are
  tiered by moderation rank (Mod 1 < Admin 2 < Owner 3). The Overview tab
  (server identity), custom emojis and the stats toggle are fully functional;
  the remaining sections are placeholders for future server-side settings and
  render disabled controls.
-->
<script lang="ts">
  import { onMount, onDestroy, untrack } from 'svelte';
  import { chat } from '$lib/stores/chat';
  import { selectedServer } from '$lib/stores/servers';
  import { customEmojis, customEmojiList } from '$lib/stores/customEmojis';
  import { dialogs } from '$lib/stores/dialogs';
  import { describeServerError } from '$lib/errors';
  import { httpBaseFromWs } from '$lib/server-url';
  import {
    EMOJI_NAME_RE,
    MAX_EMOJI_FILE_BYTES,
    MAX_SERVER_NAME_LENGTH,
    MAX_SERVER_DESCRIPTION_LENGTH,
    MAX_WELCOME_MESSAGE_LENGTH,
    MAX_SERVER_ICON_BYTES
  } from '$lib/chat/constants';
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
    { id: 'emojis', label: 'Emojis', perm: PERMISSIONS.MANAGE_EMOJIS },
    { id: 'moderation', label: 'Moderation', perm: PERMISSIONS.BAN_MEMBERS },
    { id: 'stats', label: 'Stats', perm: PERMISSIONS.MANAGE_SERVER },
    { id: 'uploads', label: 'Files & Uploads', perm: PERMISSIONS.MANAGE_SERVER },
    { id: 'voice', label: 'Voice', perm: PERMISSIONS.MANAGE_SERVER },
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
    const form = new FormData();
    form.append('file', file);
    try {
      const res = await fetch(httpBase + '/upload', { method: 'POST', body: form });
      if (res.status === 415) {
        identityFeedback = { text: 'This image type is not allowed on the server.', kind: 'error' };
        return;
      }
      if (res.status === 413) {
        identityFeedback = { text: 'That image is too large to upload.', kind: 'error' };
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

  // ── Screen share bitrate cap (Voice tab) ──────────────────────────────────
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
    } else if (ROLE_ERROR_CODES.has(code)) {
      roleErrorFeedback(code);
    }
  }

  onMount(() => chat.on('error', handleServerError));
  onDestroy(() => chat.off('error', handleServerError));

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
    const form = new FormData();
    form.append('file', emojiFile);
    try {
      const res = await fetch(httpBase + '/upload', { method: 'POST', body: form });
      if (res.status === 415) {
        emojiFeedback = { text: 'This image type is not allowed on the server.', kind: 'error' };
        return;
      }
      if (res.status === 413) {
        emojiFeedback = { text: 'That image is too large to upload.', kind: 'error' };
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
  let draftPermissions = $state(0);
  let roleFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);

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
    chat.sendRaw({
      type: 'update-role',
      id: selectedRole.id,
      name: draftName.trim(),
      color: color === '' ? null : color,
      permissions: draftPermissions >>> 0
    });
    roleFeedback = { text: 'Changes sent.', kind: 'info' };
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
            <div class="setting-group">
              <span class="setting-label">Slow mode <span class="badge">Coming soon</span></span>
              <input type="number" min="0" placeholder="0" disabled />
              <div class="setting-description">Seconds each member must wait between messages.</div>
            </div>
            <div class="setting-group">
              <span class="setting-label">Max message length <span class="badge">Coming soon</span></span>
              <input type="number" value="4000" disabled />
              <div class="setting-description">Maximum characters per message. Currently fixed at 4000.</div>
            </div>
            <div class="setting-group">
              <label class="toggle-row">
                <input type="checkbox" disabled />
                <span class="toggle-text">
                  <span class="toggle-label">Profanity filter <span class="badge">Coming soon</span></span>
                  <span class="toggle-description">Automatically hide messages containing filtered words.</span>
                </span>
              </label>
            </div>
            <div class="setting-group">
              <span class="setting-label">Ban list <span class="badge">Coming soon</span></span>
              <div class="setting-description">
                Review and lift bans from here. Until then, bans are managed via the user context menu.
              </div>
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
              <span class="setting-label">Max upload size <span class="badge">Coming soon</span></span>
              <input type="number" value="10" disabled />
              <div class="setting-description">Maximum file size in MB. Currently fixed at 10 MB.</div>
            </div>
            <div class="setting-group">
              <span class="setting-label">Allowed file types <span class="badge">Coming soon</span></span>
              <input type="text" value="images, documents, archives, audio, video" disabled />
              <div class="setting-description">
                Which upload categories members may attach. Active content (HTML, SVG, scripts) is always rejected.
              </div>
            </div>
          </div>
        {/if}

        {#if activeTab === 'voice'}
          <div class="settings-section">
            <h3 class="section-title">Voice &amp; Screen Share</h3>
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
            <div class="setting-group">
              <span class="setting-label">Default bitrate <span class="badge">Coming soon</span></span>
              <input type="number" value="64000" disabled />
              <div class="setting-description">Bitrate in bits per second assigned to new voice channels.</div>
            </div>
            <div class="setting-group">
              <span class="setting-label">Default quality <span class="badge">Coming soon</span></span>
              <select disabled>
                <option>Standard</option>
                <option>High</option>
              </select>
              <div class="setting-description">Quality preset assigned to new voice channels.</div>
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
              <span class="setting-label">Purge all messages <span class="badge">Coming soon</span></span>
              <div class="setting-description">Permanently delete every message on this server.</div>
              <div><button class="btn btn-danger" disabled>Purge messages…</button></div>
            </div>
            <div class="setting-group">
              <span class="setting-label">Reset server <span class="badge">Coming soon</span></span>
              <div class="setting-description">
                Remove all channels, categories, roles and messages and start over.
              </div>
              <div><button class="btn btn-danger" disabled>Reset server…</button></div>
            </div>
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
