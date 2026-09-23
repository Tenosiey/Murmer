<script lang="ts">
  import { onMount, onDestroy, untrack } from 'svelte';
  import { chat } from '$lib/stores/chat';
  import { describeServerError } from '$lib/errors';
  import {
    MIN_UPLOAD_MAX_BYTES,
    MAX_UPLOAD_MAX_BYTES,
    UPLOAD_CATEGORIES
  } from '$lib/chat/constants';
  import { storageUsage, formatBytes } from '$lib/stores/storageUsage';
  import { uploadConfig, setUploadConfig } from '$lib/stores/uploadConfig';
  import type { Message } from '$lib/types';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  const BYTES_PER_MB = 1024 * 1024;
  // Filled from the server's answer when the dashboard opens, then left
  // alone: incoming broadcasts must not clobber what the user is editing.
  let uploadMaxMb = $state($uploadConfig.maxBytes / BYTES_PER_MB);
  let uploadCategories: string[] = $state([...$uploadConfig.categories]);
  let uploadFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);
  /** Set after sending a save; cleared once the broadcast confirms the
      change or an error frame arrives. */
  let uploadSavePending = $state(false);

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

  // Measured on request rather than tracked, so it is asked for when the tab
  // is opened and by the refresh button.
  $effect(() => {
    if (active) {
      untrack(() => storageUsage.refresh());
    }
  });

  const CATEGORY_LABELS: Record<string, string> = {
    ...Object.fromEntries(UPLOAD_CATEGORIES.map((category) => [category.id, category.label])),
    // Files whose extension left the safe-list still occupy disk.
    other: 'Other'
  };

  const UPLOAD_ERROR_CODES = new Set([
    'upload-permission-denied',
    'invalid-upload-config',
    'upload-config-update-failed'
  ]);

  function handleServerError(msg: Message) {
    const code = msg.message;
    if (typeof code !== 'string') return;
    if (UPLOAD_ERROR_CODES.has(code)) {
      uploadSavePending = false;
      uploadFeedback = { text: describeServerError(code), kind: 'error' };
    }
  }

  onMount(() => chat.on('error', handleServerError));
  onDestroy(() => chat.off('error', handleServerError));
</script>

{#if active}
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

<style>
  /* Outranks the dashboard's shared `.modal-body .toggle-description`,
     which lives in the parent's stylesheet. */
  .toggle-text .toggle-description.upload-extensions {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    word-break: break-word;
  }

  .storage-total {
    font-size: var(--text-lg);
    color: var(--color-on-surface);
  }

  .storage-files {
    font-size: var(--text-xs);
    color: var(--color-muted);
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
</style>
