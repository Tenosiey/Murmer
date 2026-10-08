<script lang="ts">
  import IdentityBackup from '$lib/components/IdentityBackup.svelte';
  import { loadKeyPair } from '$lib/keypair';
  import { onMount } from 'svelte';
  import { session } from '$lib/stores/session';
  import { avatars } from '$lib/stores/avatars';
  import { connection } from '$lib/stores/connection';
  import { selectedServer } from '$lib/stores/servers';
  import { httpBaseFromWs } from '$lib/server-url';
  import { MAX_AVATAR_BYTES } from '$lib/chat/constants';
  import { uploadImage } from '$lib/upload';
  import UserAvatar from '$lib/components/UserAvatar.svelte';
  import { AUTO_AWAY_OPTIONS, autoAwayMinutes } from '$lib/stores/autoAway';
  import { t } from '$lib/i18n';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  let publicKey = $state('');
  let keyCopied = $state(false);

  onMount(() => {
    try {
      publicKey = loadKeyPair().publicKey;
    } catch (e) {
      console.error('Failed to load key pair', e);
    }
  });

  async function copyPublicKey() {
    try {
      await navigator.clipboard.writeText(publicKey);
      keyCopied = true;
      setTimeout(() => { keyCopied = false; }, 2000);
    } catch (e) {
      console.error('Failed to copy public key', e);
    }
  }

  let avatarFileInput: HTMLInputElement | null = $state(null);
  let avatarUploading = $state(false);
  let avatarError = $state('');
  let hasAvatar = $derived(Boolean($session.user && $avatars[$session.user]));

  async function uploadAvatar(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0] ?? null;
    input.value = '';
    if (!file || !$selectedServer) return;
    avatarError = '';
    avatarUploading = true;
    const result = await uploadImage(httpBaseFromWs($selectedServer), file, MAX_AVATAR_BYTES);
    avatarUploading = false;
    if (!result.ok) {
      avatarError = result.message;
      return;
    }
    // Validated and registered server-side; the confirmation arrives as a
    // broadcast avatar-update frame which updates the store.
    avatars.setSelf(result.url);
  }

  function removeAvatar() {
    avatarError = '';
    avatars.setSelf(null);
  }
</script>

{#if active}
  <div class="settings-section">
    <h3 class="section-title">{t('settings.tab.identity')}</h3>
    <div class="setting-group">
      <span class="setting-label">{t('identity.avatar')}</span>
      {#if $connection === 'connected' && $session.user}
        <div class="avatar-row">
          <UserAvatar name={$session.user} />
          <button
            class="btn"
            onclick={() => avatarFileInput?.click()}
            disabled={avatarUploading}
          >
            {avatarUploading
              ? t('identity.uploading')
              : hasAvatar
                ? t('identity.changeImage')
                : t('identity.uploadImage')}
          </button>
          {#if hasAvatar}
            <button class="btn btn-danger" onclick={removeAvatar} disabled={avatarUploading}>
              {t('identity.remove')}
            </button>
          {/if}
          <input
            bind:this={avatarFileInput}
            type="file"
            accept="image/png,image/jpeg,image/gif,image/webp"
            hidden
            onchange={uploadAvatar}
          />
        </div>
        {#if avatarError}
          <div class="setting-description avatar-error" role="alert">{avatarError}</div>
        {/if}
        <div class="setting-description">
          {t('identity.avatarHint')}
        </div>
      {:else}
        <div class="setting-description">{t('identity.connectForAvatar')}</div>
      {/if}
    </div>
    <div class="setting-group">
      <label class="setting-label" for="public-key-display">{t('identity.publicKey')}</label>
      <div class="pubkey-row">
        <input
          id="public-key-display"
          class="pubkey-input"
          type="text"
          readonly
          value={publicKey}
        />
        <button class="icon-btn copy-btn" onclick={copyPublicKey} title={t('identity.copyKey')}>
          {#if keyCopied}
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <polyline points="20,6 9,17 4,12"></polyline>
            </svg>
          {:else}
            <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
              <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
              <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
            </svg>
          {/if}
        </button>
      </div>
      <div class="setting-description">
        {t('identity.publicKeyHint')}
      </div>
    </div>
    <div class="setting-group">
      <label for="auto-away-select" class="setting-label">{t('identity.away')}</label>
      <div class="select-container">
        <select id="auto-away-select" class="device-select" bind:value={$autoAwayMinutes}>
          {#each AUTO_AWAY_OPTIONS as minutes}
            <option value={minutes}>{minutes === 0 ? t('identity.never') : t('identity.afterMinutes', { count: minutes })}</option>
          {/each}
        </select>
        <div class="select-arrow">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="6,9 12,15 18,9"></polyline>
          </svg>
        </div>
      </div>
      <div class="setting-description">
        {t('identity.awayHint')}
      </div>
    </div>
    <IdentityBackup />
  </div>
{/if}

<style>

  .avatar-row {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }

  /* Outranks the modal's shared `.modal-body .setting-description`, which
     lives in the parent's stylesheet and would otherwise mute the red. */
  .setting-group .setting-description.avatar-error {
    color: var(--color-error);
  }

  .pubkey-row {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }

  .pubkey-input {
    flex: 1;
    min-width: 0;
    color: var(--color-muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    cursor: text;
    user-select: all;
  }

  .copy-btn {
    border: 1px solid var(--color-surface-outline);
  }
</style>
