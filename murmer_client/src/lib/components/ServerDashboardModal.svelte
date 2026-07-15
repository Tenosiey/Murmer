<!--
  Server management dashboard for moderators and above. Separate from the
  user-facing SettingsModal: everything in here is server-wide state. Tabs are
  tiered by moderation rank (Mod 1 < Admin 2 < Owner 3). Custom emojis are
  fully functional; the remaining sections are placeholders for future
  server-side settings and render disabled controls.
-->
<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { chat } from '$lib/stores/chat';
  import { selectedServer } from '$lib/stores/servers';
  import { customEmojis, customEmojiList } from '$lib/stores/customEmojis';
  import { dialogs } from '$lib/stores/dialogs';
  import { describeServerError } from '$lib/errors';
  import { httpBaseFromWs } from '$lib/server-url';
  import { EMOJI_NAME_RE, MAX_EMOJI_FILE_BYTES } from '$lib/chat/constants';
  import type { Message } from '$lib/types';

  export let open: boolean;
  export let close: () => void;
  /** The viewer's moderation rank; gates which tabs are visible. */
  export let rank: number;

  // Each server-wide topic lives on its own tab; minRank hides tabs the
  // viewer's role does not reach.
  const TABS = [
    { id: 'overview', label: 'Overview', minRank: 2 },
    { id: 'emojis', label: 'Emojis', minRank: 1 },
    { id: 'moderation', label: 'Moderation', minRank: 1 },
    { id: 'uploads', label: 'Files & Uploads', minRank: 2 },
    { id: 'voice', label: 'Voice', minRank: 2 },
    { id: 'roles', label: 'Roles', minRank: 3 },
    { id: 'danger', label: 'Danger Zone', minRank: 3 }
  ] as const;
  let activeTab: (typeof TABS)[number]['id'] = 'emojis';

  $: visibleTabs = TABS.filter((tab) => rank >= tab.minRank);
  // If the active tab disappears (e.g. the viewer's role changed), fall back
  // to the first visible one.
  $: if (visibleTabs.length > 0 && !visibleTabs.some((tab) => tab.id === activeTab)) {
    activeTab = visibleTabs[0].id;
  }

  $: httpBase = $selectedServer ? httpBaseFromWs($selectedServer) : '';

  // ── Custom emoji management ────────────────────────────────────────────────
  let emojiName = '';
  let emojiFile: File | null = null;
  let emojiFileInput: HTMLInputElement | null = null;
  let uploading = false;
  let emojiFeedback: { text: string; kind: 'error' | 'info' } | null = null;

  $: normalizedEmojiName = emojiName.trim().toLowerCase();
  $: emojiNameValid = EMOJI_NAME_RE.test(normalizedEmojiName);
  $: emojiNameTaken = emojiNameValid && normalizedEmojiName in $customEmojis;
  $: emojiFileTooLarge = emojiFile !== null && emojiFile.size > MAX_EMOJI_FILE_BYTES;
  $: canUploadEmoji =
    !uploading && emojiNameValid && !emojiNameTaken && emojiFile !== null && !emojiFileTooLarge;

  const EMOJI_ERROR_CODES = new Set([
    'emoji-permission-denied',
    'invalid-emoji-name',
    'invalid-emoji-url',
    'emoji-name-taken',
    'emoji-limit-reached',
    'emoji-update-failed',
    'emoji-not-found'
  ]);

  function handleServerError(msg: Message) {
    const code = (msg as any).message;
    if (!open || typeof code !== 'string' || !EMOJI_ERROR_CODES.has(code)) return;
    emojiFeedback = { text: describeServerError(code), kind: 'error' };
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

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      close();
    }
  }

  function handleOverlayKeydown(event: KeyboardEvent) {
    if (event.key === 'Enter' || event.key === ' ') {
      close();
    }
  }
</script>

{#if open}
  <div class="modal-overlay" on:click={close} on:keydown={handleOverlayKeydown} role="dialog" aria-modal="true" aria-labelledby="server-dashboard-title" tabindex="-1">
    <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
    <!-- svelte-ignore a11y-click-events-have-key-events -->
    <!-- svelte-ignore a11y-no-noninteractive-tabindex -->
    <div class="modal-content" on:click|stopPropagation on:keydown={handleKeydown} role="document" tabindex="0">
      <div class="modal-header">
        <h2 id="server-dashboard-title">Server Dashboard</h2>
        <button class="icon-btn close-btn" on:click={close} aria-label="Close server dashboard">
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
              on:click={() => (activeTab = tab.id)}
            >{tab.label}</button>
          {/each}
        </nav>

        <div class="modal-body">
        {#if activeTab === 'overview'}
          <div class="settings-section">
            <h3 class="section-title">Server Identity</h3>
            <div class="setting-group">
              <span class="setting-label">Server name <span class="badge">Coming soon</span></span>
              <input type="text" placeholder="My Murmer Server" disabled />
              <div class="setting-description">The name shown to members of this server.</div>
            </div>
            <div class="setting-group">
              <span class="setting-label">Description <span class="badge">Coming soon</span></span>
              <textarea rows="2" placeholder="What this server is about…" disabled></textarea>
              <div class="setting-description">A short description shown when joining.</div>
            </div>
            <div class="setting-group">
              <span class="setting-label">Welcome message <span class="badge">Coming soon</span></span>
              <input type="text" placeholder="Welcome to the server!" disabled />
              <div class="setting-description">Sent to new members the first time they connect.</div>
            </div>
            <div class="setting-group">
              <span class="setting-label">Server icon <span class="badge">Coming soon</span></span>
              <div><button class="btn" disabled>Upload icon…</button></div>
              <div class="setting-description">Shown in the server list of every member.</div>
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
              <form class="emoji-form" on:submit|preventDefault={uploadEmoji}>
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
                    on:change={handleEmojiFileChange}
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
                        on:click={() => deleteEmoji(emoji.name)}
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
            <h3 class="section-title">Voice</h3>
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
            <div class="setting-group">
              <span class="setting-label">Role management <span class="badge">Coming soon</span></span>
              <div class="setting-description">
                Create custom roles and manage assignments from here. Until then, roles are assigned
                by Owners via the user context menu in the sidebar.
              </div>
              <div><button class="btn" disabled>Manage roles…</button></div>
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
        <button class="btn btn-primary" on:click={close}>Done</button>
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
    width: min(720px, 94vw);
    height: min(560px, 82vh);
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
</style>
