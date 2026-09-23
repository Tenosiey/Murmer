<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { chat } from '$lib/stores/chat';
  import { selectedServer } from '$lib/stores/servers';
  import { describeServerError } from '$lib/errors';
  import { httpBaseFromWs } from '$lib/server-url';
  import { uploadForm, uploadErrorMessage } from '$lib/upload';
  import { onlineUsers } from '$lib/stores/online';
  import { displayNames } from '$lib/stores/profiles';
  import {
    MAX_SERVER_NAME_LENGTH,
    MAX_SERVER_DESCRIPTION_LENGTH,
    MAX_WELCOME_MESSAGE_LENGTH,
    MAX_SERVER_ICON_BYTES
  } from '$lib/chat/constants';
  import { serverIdentity } from '$lib/stores/serverIdentity';
  import type { Message } from '$lib/types';

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

  const IDENTITY_ERROR_CODES = new Set([
    'identity-permission-denied',
    'invalid-server-name',
    'invalid-server-description',
    'invalid-welcome-message',
    'invalid-server-icon',
    'identity-update-failed'
  ]);

  function handleServerError(msg: Message) {
    const code = msg.message;
    if (typeof code !== 'string') return;
    if (IDENTITY_ERROR_CODES.has(code)) {
      identitySavePending = false;
      identityFeedback = { text: describeServerError(code), kind: 'error' };
    }
  }

  onMount(() => chat.on('error', handleServerError));
  onDestroy(() => chat.off('error', handleServerError));
</script>

{#if active}
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
    background: var(--color-success, #22c55e);
    flex-shrink: 0;
  }
</style>
