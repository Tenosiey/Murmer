<script lang="ts">
  import { onMount } from 'svelte';
  import { chat, onServerError } from '$lib/stores/chat';
  import { selectedServer } from '$lib/stores/servers';
  import { customEmojis, customEmojiList } from '$lib/stores/customEmojis';
  import { dialogs } from '$lib/stores/dialogs';
  import { httpBaseFromWs } from '$lib/server-url';
  import { uploadImage } from '$lib/upload';
  import { t } from '$lib/i18n';
  import {
    EMOJI_NAME_RE,
    MAX_EMOJI_FILE_BYTES
  } from '$lib/chat/constants';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  let httpBase = $derived($selectedServer ? httpBaseFromWs($selectedServer) : '');

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
    const result = await uploadImage(httpBase, emojiFile, MAX_EMOJI_FILE_BYTES);
    uploading = false;
    if (!result.ok) {
      emojiFeedback = { text: result.message, kind: 'error' };
      return;
    }
    // Registration is role-checked server-side; success arrives as an
    // updated emoji-list broadcast, errors as an error frame handled above.
    chat.sendRaw({ type: 'add-emoji', name, url: result.url });
    emojiName = '';
    emojiFile = null;
    if (emojiFileInput) emojiFileInput.value = '';
  }

  async function deleteEmoji(name: string) {
    const ok = await dialogs.confirm({
      title: t('emojis.deleteTitle'),
      message: t('emojis.deleteMessage', { name }),
      confirmLabel: t('emojis.delete'),
      danger: true
    });
    if (!ok) return;
    chat.sendRaw({ type: 'remove-emoji', name });
  }

  const EMOJI_ERROR_CODES = new Set([
    'emoji-permission-denied',
    'invalid-emoji-name',
    'invalid-emoji-url',
    'emoji-name-taken',
    'emoji-limit-reached',
    'emoji-update-failed',
    'emoji-not-found'
  ]);

  onMount(() =>
    onServerError(EMOJI_ERROR_CODES, (text) => {
      emojiFeedback = { text, kind: 'error' };
    })
  );
</script>

{#if active}
  <div class="settings-section">
    <h3 class="section-title">{t('emojis.customEmojis')}</h3>
    <div class="setting-group">
      <div class="setting-description">
        {t('emojis.uploadCustomEmojisFor')}
      </div>
      <form class="emoji-form" onsubmit={(event) => { event.preventDefault(); uploadEmoji(); }}>
        <label class="field emoji-name-field">
          <span>{t('emojis.name')}</span>
          <input
            type="text"
            bind:value={emojiName}
            placeholder={t('emojis.partyParrot')}
            maxlength="32"
            spellcheck="false"
            autocomplete="off"
          />
        </label>
        <label class="field">
          <span>{t('emojis.image')}</span>
          <input
            bind:this={emojiFileInput}
            type="file"
            accept="image/png,image/jpeg,image/gif,image/webp"
            onchange={handleEmojiFileChange}
          />
        </label>
        <button class="btn btn-primary" type="submit" disabled={!canUploadEmoji}>
          {uploading ? t('emojis.uploading') : t('emojis.upload')}
        </button>
      </form>
      {#if emojiName && !emojiNameValid}
        <div class="emoji-hint">{t('emojis.namesUse232')}</div>
      {:else if emojiNameTaken}
        <div class="emoji-hint">{t('emojis.anEmojiWithThis')}</div>
      {:else if emojiFileTooLarge}
        <div class="emoji-hint">{t('emojis.emojiImagesMustBe')}</div>
      {/if}
      {#if emojiFeedback}
        <div class="emoji-feedback" class:error={emojiFeedback.kind === 'error'}>
          {emojiFeedback.text}
        </div>
      {/if}
    </div>

    <div class="setting-group">
      {#if $customEmojiList.length === 0}
        <div class="setting-description">{t('emojis.noCustomEmojisYet')}</div>
      {:else}
        <ul class="emoji-list">
          {#each $customEmojiList as emoji (emoji.name)}
            <li class="emoji-row">
              <img src={httpBase + emoji.url} alt={`:${emoji.name}:`} width="24" height="24" loading="lazy" />
              <span class="emoji-code">:{emoji.name}:</span>
              {#if emoji.uploadedBy}
                <span class="emoji-uploader">{t('emojis.uploadedBy', { name: emoji.uploadedBy })}</span>
              {/if}
              <button
                class="icon-btn danger"
                title={t('emojis.deleteTooltip', { name: emoji.name })}
                aria-label={t('emojis.deleteLabel', { name: emoji.name })}
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

<style>
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
</style>
