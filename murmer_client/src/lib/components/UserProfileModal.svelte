<!--
  A member's profile: avatar, the name shown for them, account name, roles,
  member since and their "about" text. Opened by clicking a member in the
  sidebar, their avatar or name on a message, or "View profile" in the user
  context menu.

  Viewing your own profile turns it into an editor for what you own — avatar,
  display name, nickname, status line and about text. The account name is never editable: it
  is bound to your key on the server and everything (auth, roles, DMs, message
  authorship) is addressed by it, so it is shown as a read-only handle.

  A nickname set by a moderator lands in the same field: it is this server's
  label for you, so the editor shows what it currently is rather than hiding
  the fact that somebody else changed it.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { displayNames } from '$lib/stores/profiles';
  import { onServerError } from '$lib/stores/chat';
  import { session } from '$lib/stores/session';
  import { avatars } from '$lib/stores/avatars';
  import { profiles, statusTexts } from '$lib/stores/profiles';
  import { roleDefinitions } from '$lib/stores/roleDefinitions';
  import { userRoleIds } from '$lib/stores/roles';
  import { selectedServer } from '$lib/stores/servers';
  import { blockedUsers, confirmBlock } from '$lib/stores/blocks';
  import { httpBaseFromWs } from '$lib/server-url';
  import { uploadImage } from '$lib/upload';
  import {
    MAX_ABOUT_LENGTH,
    MAX_AVATAR_BYTES,
    MAX_DISPLAY_NAME_LENGTH,
    MAX_NICKNAME_LENGTH,
    MAX_STATUS_TEXT_LENGTH
  } from '$lib/chat/constants';
  import UserAvatar from '$lib/components/UserAvatar.svelte';
  import RoleIcon from '$lib/components/RoleIcon.svelte';
  import type { RoleDef } from '$lib/types';
  import { modalFocus } from '$lib/focus';
  import { t } from '$lib/i18n';

  /** Split around the name, which is rendered bold. */
  const nicknameHint = t('userProfileModal.nicknameHint').split('{name}');

  interface Props {
    open: boolean;
    /** Account name of the profile being viewed, or null when closed. */
    user: string | null;
    close: () => void;
    /** Start a direct message conversation with the viewed user. */
    onOpenDm: (user: string) => void;
  }

  let { open, user, close, onOpenDm }: Props = $props();

  const PROFILE_ERROR_CODES = new Set([
    'invalid-display-name',
    'invalid-about',
    'invalid-status-text',
    'profile-update-failed',
    'invalid-avatar',
    'avatar-update-failed',
    'invalid-nickname',
    'nickname-permission-denied',
    'nickname-update-failed'
  ]);

  let httpBase = $derived($selectedServer ? httpBaseFromWs($selectedServer) : '');
  let isSelf = $derived(user !== null && user === $session.user);
  let profile = $derived(user ? ($profiles[user] ?? null) : null);
  let shownName = $derived(user ? $displayNames(user) : '');
  let avatarUrl = $derived(user ? ($avatars[user] ?? null) : null);
  let statusText = $derived(user ? $statusTexts(user) : '');

  /** The user's roles, highest position first, with `@everyone` left out. */
  let userRoles = $derived.by((): RoleDef[] => {
    const ids = user ? ($userRoleIds[user] ?? []) : [];
    return $roleDefinitions
      .filter((role) => ids.includes(role.id) && !role.isDefault)
      .sort((a, b) => b.position - a.position);
  });

  let memberSince = $derived.by(() => {
    if (!profile?.createdAt) return null;
    const date = new Date(profile.createdAt);
    return Number.isNaN(date.getTime())
      ? null
      : date.toLocaleDateString(undefined, { year: 'numeric', month: 'long', day: 'numeric' });
  });

  // ── Editing (own profile only) ────────────────────────────────────────────
  let editing = $state(false);
  let draftName = $state('');
  let draftNickname = $state('');
  let draftAbout = $state('');
  let draftStatus = $state('');
  /** 'keep', 'never', or how many minutes from now the status lapses. */
  let draftStatusExpiry = $state('never');
  let canKeepExpiry = $state(false);

  const STATUS_EXPIRY_CHOICES = [
    { value: '30', label: t('duration.minutes', { count: 30 }) },
    { value: '60', label: t('duration.hours', { count: 1 }) },
    { value: '240', label: t('duration.hours', { count: 4 }) },
    { value: '1440', label: t('duration.days', { count: 1 }) },
    { value: '10080', label: t('duration.weeks', { count: 1 }) }
  ];
  let feedback: { text: string; kind: 'error' | 'info' } | null = $state(null);
  let avatarInput: HTMLInputElement | null = $state(null);
  let avatarUploading = $state(false);

  /* Leave edit mode whenever the modal closes or another profile is shown, so
     a half-typed draft never leaks into somebody else's profile. */
  $effect(() => {
    void user;
    if (!open) editing = false;
    feedback = null;
  });

  function startEditing() {
    draftName = profile?.displayName ?? '';
    draftNickname = profile?.nickname ?? '';
    draftAbout = profile?.about ?? '';
    draftStatus = statusText;
    canKeepExpiry = !!statusText && !!profile?.statusExpiresAt;
    draftStatusExpiry = canKeepExpiry ? 'keep' : 'never';
    feedback = null;
    editing = true;
  }

  function cancelEditing() {
    editing = false;
    feedback = null;
  }

  function saveProfile() {
    const name = draftName.trim();
    const nickname = draftNickname.trim();
    const about = draftAbout.trim();
    const status = draftStatus.trim();
    if (
      status.length > MAX_STATUS_TEXT_LENGTH ||
      name.length > MAX_DISPLAY_NAME_LENGTH ||
      nickname.length > MAX_NICKNAME_LENGTH ||
      about.length > MAX_ABOUT_LENGTH
    ) {
      feedback = { text: t('userProfileModal.tooLong'), kind: 'error' };
      return;
    }
    // Empty clears the field server-side; the broadcast confirms the change.
    profiles.saveSelf({ displayName: name, about });
    // The nickname is a separate frame because it is separately authorized —
    // only here it is our own, which never needs a permission.
    if (user && nickname !== (profile?.nickname ?? '')) profiles.setNickname(user, nickname);
    if (status !== statusText || draftStatusExpiry !== 'keep') {
      const expiresAt =
        draftStatusExpiry === 'keep'
          ? (profile?.statusExpiresAt ?? null)
          : draftStatusExpiry === 'never'
            ? null
            : Date.now() + Number(draftStatusExpiry) * 60_000;
      profiles.setStatusText(status, expiresAt);
    }
    feedback = { text: t('userProfileModal.sent'), kind: 'info' };
    editing = false;
  }

  async function uploadAvatar(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0] ?? null;
    input.value = '';
    if (!file) return;
    feedback = null;
    avatarUploading = true;
    const result = await uploadImage(httpBase, file, MAX_AVATAR_BYTES);
    avatarUploading = false;
    if (!result.ok) {
      feedback = { text: result.message, kind: 'error' };
      return;
    }
    // Registered over the WebSocket; the avatar-update broadcast confirms it.
    avatars.setSelf(result.url);
  }

  function removeAvatar() {
    feedback = null;
    avatars.setSelf(null);
  }

  onMount(() =>
    onServerError(PROFILE_ERROR_CODES, (text) => {
      if (open) feedback = { text, kind: 'error' };
    })
  );

  function messageUser() {
    if (!user) return;
    onOpenDm(user);
    close();
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') close();
  }

  function handleOverlayKeydown(event: KeyboardEvent) {
    // Keyboard activation of the focused backdrop only; keystrokes inside
    // the modal content (inputs, buttons) bubble here and must not close it.
    if (event.target !== event.currentTarget) return;
    if (event.key === 'Enter' || event.key === ' ') close();
  }
</script>

{#if open && user}
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
  <div
    class="modal-overlay"
    onclick={close}
    onkeydown={handleOverlayKeydown}
    role="dialog"
    aria-modal="true"
    aria-labelledby="user-profile-title"
    tabindex="-1"
  >
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div
      use:modalFocus
      class="modal-content"
      onclick={(event) => event.stopPropagation()}
      onkeydown={handleKeydown}
      role="document"
      tabindex="0"
    >
      <div class="modal-header">
        <h2 id="user-profile-title">{t('userProfileModal.profile')}</h2>
        <button class="icon-btn close-btn" onclick={close} aria-label={t('userProfileModal.closeProfile')}>
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>

      <div class="modal-body">
        <div class="identity">
          <UserAvatar name={user} size="lg" />
          <div class="identity-text">
            <div class="name-row">
              <span class="display-name">{shownName}</span>
              {#if userRoles[0]?.icon}
                <RoleIcon icon={userRoles[0].icon} role={userRoles[0].name} size="md" />
              {/if}
            </div>
            <span class="handle">{user}</span>
            {#if statusText && !(isSelf && editing)}
              <span class="status-line">{statusText}</span>
            {/if}
          </div>
        </div>

        {#if isSelf && editing}
          <div class="field-group">
            <label class="field-label" for="profile-display-name">{t('userProfileModal.displayName')}</label>
            <input
              id="profile-display-name"
              class="field"
              bind:value={draftName}
              maxlength={MAX_DISPLAY_NAME_LENGTH}
              placeholder={user}
            />
            <span class="hint">
              {nicknameHint[0]}<strong>{user}</strong>{nicknameHint[1]}
            </span>
          </div>

          <div class="field-group">
            <label class="field-label" for="profile-nickname">{t('userProfileModal.nicknameOnThisServer')}</label>
            <input
              id="profile-nickname"
              class="field"
              bind:value={draftNickname}
              maxlength={MAX_NICKNAME_LENGTH}
              placeholder={profile?.displayName || user}
            />
            <span class="hint">
              {t('userProfileModal.overridesYourDisplayName')}
            </span>
          </div>

          <div class="field-group">
            <label class="field-label" for="profile-status">{t('userProfileModal.status')}</label>
            <input
              id="profile-status"
              class="field"
              bind:value={draftStatus}
              maxlength={MAX_STATUS_TEXT_LENGTH}
              placeholder={t('userProfileModal.whatAreYouUp')}
            />
            <label class="hint" for="profile-status-expiry">{t('userProfileModal.clearAfter')}</label>
            <select id="profile-status-expiry" class="field" bind:value={draftStatusExpiry}>
              {#if canKeepExpiry}
                <option value="keep">{t('userProfileModal.keepTheCurrentExpiry')}</option>
              {/if}
              <option value="never">{t('userProfileModal.donTClear')}</option>
              {#each STATUS_EXPIRY_CHOICES as choice (choice.value)}
                <option value={choice.value}>{choice.label}</option>
              {/each}
            </select>
          </div>

          <div class="field-group">
            <label class="field-label" for="profile-about">{t('userProfileModal.about')}</label>
            <textarea
              id="profile-about"
              class="field about-input"
              bind:value={draftAbout}
              maxlength={MAX_ABOUT_LENGTH}
              rows="4"
              placeholder={t('userProfileModal.somethingAboutYou')}
            ></textarea>
            <span class="hint">{draftAbout.length} / {MAX_ABOUT_LENGTH}</span>
          </div>

          <div class="field-group">
            <span class="field-label">{t('userProfileModal.avatar')}</span>
            <div class="avatar-actions">
              <button
                class="btn"
                onclick={() => avatarInput?.click()}
                disabled={avatarUploading || !httpBase}
              >
                {avatarUploading
                  ? t('userProfileModal.uploading')
                  : t(avatarUrl ? 'userProfileModal.changeImage' : 'userProfileModal.uploadImage')}
              </button>
              {#if avatarUrl}
                <button class="btn btn-danger" onclick={removeAvatar} disabled={avatarUploading}>
                  {t('userProfileModal.remove')}
                </button>
              {/if}
              <input
                bind:this={avatarInput}
                type="file"
                accept="image/png,image/jpeg,image/gif,image/webp"
                class="sr-only"
                onchange={uploadAvatar}
              />
            </div>
          </div>
        {:else}
          {#if profile?.nickname}
            <div class="field-group">
              <span class="field-label">{t('userProfileModal.nicknameOnThisServer')}</span>
              <span class="member-since">
                {profile.nickname}
                {#if profile.displayName}
                  <span class="hint">{t('userProfileModal.displayNameHint', { name: profile.displayName })}</span>
                {/if}
              </span>
            </div>
          {/if}

          {#if userRoles.length > 0}
            <div class="field-group">
              <span class="field-label">{t('userProfileModal.roles')}</span>
              <div class="role-chips">
                {#each userRoles as role (role.id)}
                  <span class="role-chip">
                    <span
                      class="role-dot"
                      style={`background: ${role.color ?? 'var(--color-muted)'}`}
                      aria-hidden="true"
                    ></span>
                    {#if role.icon}
                      <RoleIcon icon={role.icon} role={role.name} />
                    {/if}
                    <span style={role.color ? `color: ${role.color}` : ''}>{role.name}</span>
                  </span>
                {/each}
              </div>
            </div>
          {/if}

          {#if memberSince}
            <div class="field-group">
              <span class="field-label">{t('userProfileModal.memberSince')}</span>
              <span class="member-since">{memberSince}</span>
            </div>
          {/if}

          {#if profile?.about}
            <div class="field-group">
              <span class="field-label">{t('userProfileModal.about')}</span>
              <p class="about-text">{profile.about}</p>
            </div>
          {/if}
        {/if}

        {#if feedback}
          <div class="feedback" class:error={feedback.kind === 'error'} role="status">
            {feedback.text}
          </div>
        {/if}
      </div>

      <div class="modal-footer">
        {#if isSelf}
          {#if editing}
            <button class="btn" onclick={cancelEditing}>{t('userProfileModal.cancel')}</button>
            <button class="btn btn-primary" onclick={saveProfile}>{t('userProfileModal.saveProfile')}</button>
          {:else}
            <button class="btn btn-primary" onclick={startEditing}>{t('userProfileModal.editProfile')}</button>
          {/if}
        {:else}
          {#if user && $blockedUsers.includes(user)}
            <button class="btn" onclick={() => user && blockedUsers.setBlocked(user, false)}>{t('userProfileModal.unblock')}</button>
          {:else}
            <button class="btn btn-danger" onclick={() => user && confirmBlock(user, $displayNames(user))}>{t('userProfileModal.block')}</button>
          {/if}
          <button class="btn btn-primary" onclick={messageUser}>{t('userProfileModal.sendMessage')}</button>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-overlay {
    position: fixed;
    inset: 0;
    background: var(--color-overlay);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: var(--space-5);
    z-index: var(--z-modal);
  }

  .modal-content {
    background: var(--color-surface-elevated);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    border: 1px solid var(--color-surface-outline);
    width: min(480px, 94vw);
    max-height: min(640px, 86vh);
    overflow: hidden;
    display: flex;
    flex-direction: column;
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: var(--space-4);
    border-bottom: 1px solid var(--color-surface-outline);
  }

  .modal-header h2 {
    margin: 0;
    font-size: var(--text-lg);
  }

  .modal-body {
    padding: var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    overflow-y: auto;
  }

  .modal-footer {
    display: flex;
    justify-content: flex-end;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-top: 1px solid var(--color-surface-outline);
  }

  .identity {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-width: 0;
  }

  .identity-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .name-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }

  .display-name {
    font-size: var(--text-xl);
    font-weight: 600;
    color: var(--color-on-surface);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* The account name: the identity everything is addressed by, so it gets the
     monospace treatment reserved for exact values. */
  .handle {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .status-line {
    font-size: var(--text-sm);
    color: var(--color-on-surface-variant);
    overflow-wrap: anywhere;
  }

  .field-group {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
  }

  .field-label {
    font-size: var(--text-xs);
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--color-muted);
    font-weight: 600;
  }

  .hint {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .about-input {
    resize: vertical;
    font: inherit;
  }

  .about-text {
    margin: 0;
    font-size: var(--text-md);
    color: var(--color-on-surface-variant);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
  }

  .member-since {
    font-size: var(--text-md);
    color: var(--color-on-surface-variant);
  }

  .role-chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .role-chip {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-1) var(--space-2);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-pill);
    background: var(--color-surface-raised);
    font-size: var(--text-sm);
  }

  .role-dot {
    width: 8px;
    height: 8px;
    border-radius: var(--radius-pill, 50%);
    flex-shrink: 0;
  }

  .avatar-actions {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
  }

  .feedback {
    font-size: var(--text-sm);
    color: var(--color-muted);
  }

  .feedback.error {
    color: var(--color-error);
  }
</style>
