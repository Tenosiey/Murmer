<script lang="ts">
  import { onMount } from 'svelte';
  import { onServerError } from '$lib/stores/chat';
  import { selectedServer } from '$lib/stores/servers';
  import { httpBaseFromWs } from '$lib/server-url';
  import { uploadImage } from '$lib/upload';
  import { onlineUsers } from '$lib/stores/online';
  import { displayNames } from '$lib/stores/profiles';
  import {
    MAX_SERVER_NAME_LENGTH,
    MAX_SERVER_DESCRIPTION_LENGTH,
    MAX_WELCOME_MESSAGE_LENGTH,
    MAX_SERVER_ICON_BYTES
  } from '$lib/chat/constants';
  import { serverIdentity } from '$lib/stores/serverIdentity';
  import { t } from '$lib/i18n';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  let httpBase = $derived($selectedServer ? httpBaseFromWs($selectedServer) : '');

  // Filled from the server's answer when the dashboard opens, then left
  // alone: incoming identity broadcasts must not clobber what the user is
  // typing.
  let identityName = $state($serverIdentity?.name ?? '');
  let identityDescription = $state($serverIdentity?.description ?? '');
  let identityWelcome = $state($serverIdentity?.welcomeMessage ?? '');
  let identityFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);
  /** Set after sending a save; cleared (with feedback) once the broadcast
      confirms the change or an error frame arrives. */
  let identitySavePending = $state(false);
  let iconFileInput: HTMLInputElement | null = $state(null);
  let iconUploading = $state(false);

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
      identityFeedback = { text: t('overview.saved'), kind: 'info' };
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
    iconUploading = true;
    const result = await uploadImage(httpBase, file, MAX_SERVER_ICON_BYTES);
    iconUploading = false;
    if (!result.ok) {
      identityFeedback = { text: result.message, kind: 'error' };
      return;
    }
    // Registration is role-checked server-side, like emoji registration.
    serverIdentity.save({ icon: result.url });
  }

  function removeIcon() {
    identityFeedback = null;
    serverIdentity.save({ icon: null });
  }

  const IDENTITY_ERROR_CODES = new Set([
    'identity-permission-denied',
    'invalid-server-name',
    'invalid-server-description',
    'invalid-welcome-message',
    'invalid-server-icon',
    'identity-update-failed'
  ]);

  onMount(() =>
    onServerError(IDENTITY_ERROR_CODES, (text) => {
    identitySavePending = false;
      identityFeedback = { text, kind: 'error' };
    })
  );
</script>

{#if active}
  <div class="settings-section">
    <h3 class="section-title">{t('overview.serverIdentity')}</h3>
    <div class="setting-group">
      <span class="setting-label">{t('overview.serverName')}</span>
      <input
        type="text"
        bind:value={identityName}
        placeholder={t('overview.myMurmerServer')}
        maxlength={MAX_SERVER_NAME_LENGTH}
        disabled={$serverIdentity === null}
      />
      <div class="setting-description">{t('overview.theNameShownTo')}</div>
    </div>
    <div class="setting-group">
      <span class="setting-label">{t('overview.description')}</span>
      <textarea
        rows="2"
        bind:value={identityDescription}
        placeholder={t('overview.whatThisServerIs')}
        maxlength={MAX_SERVER_DESCRIPTION_LENGTH}
        disabled={$serverIdentity === null}
      ></textarea>
      <div class="setting-description">{t('overview.aShortDescriptionShown')}</div>
    </div>
    <div class="setting-group">
      <span class="setting-label">{t('overview.welcomeMessage')}</span>
      <input
        type="text"
        bind:value={identityWelcome}
        placeholder={t('overview.welcomeToTheServer')}
        maxlength={MAX_WELCOME_MESSAGE_LENGTH}
        disabled={$serverIdentity === null}
      />
      <div class="setting-description">
        {t('overview.shownToNewMembers')}
      </div>
    </div>
    <div class="setting-group">
      <div>
        <button
          class="btn btn-primary"
          onclick={saveIdentity}
          disabled={!identityDirty || $serverIdentity === null}
        >{t('overview.saveChanges')}</button>
      </div>
      {#if $serverIdentity === null}
        <div class="setting-description">{t('overview.waitingForTheServer')}</div>
      {/if}
      {#if identityFeedback}
        <div class="identity-feedback" class:error={identityFeedback.kind === 'error'}>
          {identityFeedback.text}
        </div>
      {/if}
    </div>
    <div class="setting-group">
      <span class="setting-label">{t('overview.serverIcon')}</span>
      <div class="icon-row">
        {#if $serverIdentity?.icon}
          <img
            class="icon-preview"
            src={httpBase + $serverIdentity.icon}
            alt={t('overview.serverIcon')}
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
            ? t('overview.uploading')
            : $serverIdentity?.icon
              ? t('overview.replaceIcon')
              : t('overview.uploadIcon')}
        </button>
        {#if $serverIdentity?.icon}
          <button class="btn btn-danger" onclick={removeIcon} disabled={iconUploading}>
            {t('overview.remove')}
          </button>
        {/if}
      </div>
      <div class="setting-description">
        {t('overview.shownInTheServer')}
      </div>
    </div>
  </div>

  <div class="settings-section">
    <h3 class="section-title">{t('overview.onlineNow', { count: $onlineUsers.length })}</h3>
    <div class="setting-group">
      <div class="setting-description">
        {t('overview.membersConnectedToThis')}
      </div>
      {#if $onlineUsers.length === 0}
        <div class="setting-description">{t('overview.nobodyIsConnected')}</div>
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

<style>
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

  .online-dot {
    width: 0.5rem;
    height: 0.5rem;
    border-radius: 50%;
    background: var(--color-success);
    flex-shrink: 0;
  }
</style>
