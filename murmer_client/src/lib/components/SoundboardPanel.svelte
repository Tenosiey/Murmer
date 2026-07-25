<!--
  Soundboard popover.

  Opens from the voice control bar in the channel sidebar and is only usable
  while connected to a voice channel — playing a sound outside one has nobody
  to hear it. Playing is gated by USE_SOUNDBOARD, uploading and deleting by
  MANAGE_SOUNDS; both gates are cosmetic and re-checked server-side.

  Volume and mute are per listener and never leave this client: each layer is
  applied by the local player when a `soundboard-play` frame arrives.
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { chat } from '$lib/stores/chat';
  import { selectedServer } from '$lib/stores/servers';
  import { soundsByPopularity } from '$lib/stores/soundboard';
  import {
    soundboardEnabled,
    soundboardVolume,
    soundboardPrefs
  } from '$lib/stores/soundboardSettings';
  import { httpBaseFromWs } from '$lib/server-url';
  import { describeServerError } from '$lib/errors';
  import { dialogs } from '$lib/stores/dialogs';
  import {
    MAX_SOUND_FILE_BYTES,
    MAX_SOUND_NAME_LEN,
    MIN_SOUND_NAME_LEN,
    SOUNDBOARD_COOLDOWN_MS,
    SOUND_ACCEPT
  } from '$lib/chat/constants';
  import type { Message, SoundboardSound } from '$lib/types';

  interface Props {
    /** The voice channel the user is in, or null when not connected. */
    currentVoiceChannel?: number | null;
    inVoice?: boolean;
    /** Cosmetic gate: whether the viewer holds USE_SOUNDBOARD. */
    canPlay?: boolean;
    /** Cosmetic gate: whether the viewer holds MANAGE_SOUNDS. */
    canManage?: boolean;
  }

  let {
    currentVoiceChannel = null,
    inVoice = false,
    canPlay = false,
    canManage = false
  }: Props = $props();

  let open = $state(false);
  let search = $state('');
  let feedback: { text: string; kind: 'error' | 'info' } | null = $state(null);
  /** Local cooldown mirror, so the buttons dim instead of erroring. */
  let cooldownUntil = $state(0);
  let now = $state(Date.now());
  /** Which sound's volume popover is expanded, if any. */
  let tweaking: number | null = $state(null);

  let httpBase = $derived($selectedServer ? httpBaseFromWs($selectedServer) : '');

  let filtered = $derived(
    search.trim()
      ? $soundsByPopularity.filter((s) =>
          s.name.toLowerCase().includes(search.trim().toLowerCase())
        )
      : $soundsByPopularity
  );

  let onCooldown = $derived(now < cooldownUntil);

  // Only tick while a cooldown is actually running, so an idle popover costs
  // nothing.
  $effect(() => {
    if (!open || cooldownUntil === 0) return;
    const timer = setInterval(() => {
      now = Date.now();
      if (now >= cooldownUntil) cooldownUntil = 0;
    }, 100);
    return () => clearInterval(timer);
  });

  const SOUND_ERROR_CODES = new Set([
    'sound-permission-denied',
    'soundboard-permission-denied',
    'invalid-sound-name',
    'invalid-sound-file',
    'sound-name-taken',
    'sound-limit-reached',
    'sound-update-failed',
    'sound-not-found',
    'soundboard-cooldown'
  ]);

  function handleServerError(msg: Message) {
    const code = (msg as any).message;
    if (!open || typeof code !== 'string' || !SOUND_ERROR_CODES.has(code)) return;
    feedback = { text: describeServerError(code), kind: 'error' };
  }

  onMount(() => {
    chat.on('error', handleServerError);
    return () => chat.off('error', handleServerError);
  });

  function play(sound: SoundboardSound) {
    if (!inVoice || currentVoiceChannel === null || !canPlay || onCooldown) return;
    feedback = null;
    chat.sendRaw({ type: 'play-sound', id: sound.id, channelId: currentVoiceChannel });
    cooldownUntil = Date.now() + SOUNDBOARD_COOLDOWN_MS;
    now = Date.now();
  }

  function soundVolume(sound: SoundboardSound): number {
    return $soundboardPrefs.soundVolumes[sound.id] ?? 1;
  }

  function isMuted(sound: SoundboardSound): boolean {
    return $soundboardPrefs.mutedSounds.includes(sound.id);
  }

  // ── Upload / manage ─────────────────────────────────────────────────────────
  let uploading = $state(false);
  let fileInput: HTMLInputElement | null = $state(null);

  function pickFile() {
    feedback = null;
    fileInput?.click();
  }

  /** Derive a starting name from the filename: "air-horn.mp3" -> "air horn". */
  function nameFromFile(file: File): string {
    const stem = file.name.replace(/\.[^.]+$/, '').replace(/[_-]+/g, ' ').trim();
    return stem.slice(0, MAX_SOUND_NAME_LEN);
  }

  async function uploadSound(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    input.value = '';
    if (!file || !httpBase) return;

    if (file.size > MAX_SOUND_FILE_BYTES) {
      feedback = {
        text: `Sounds must be at most ${Math.round(MAX_SOUND_FILE_BYTES / 1024)} KB.`,
        kind: 'error'
      };
      return;
    }

    const suggested = nameFromFile(file);
    const name = await dialogs.prompt({
      title: 'Name this sound',
      label: 'Name',
      message: `${MIN_SOUND_NAME_LEN}-${MAX_SOUND_NAME_LEN} characters.`,
      initial: suggested,
      maxLength: MAX_SOUND_NAME_LEN
    });
    if (name === null) return;
    const trimmed = name.trim();
    if (trimmed.length < MIN_SOUND_NAME_LEN || trimmed.length > MAX_SOUND_NAME_LEN) {
      feedback = {
        text: `Names must be ${MIN_SOUND_NAME_LEN}-${MAX_SOUND_NAME_LEN} characters.`,
        kind: 'error'
      };
      return;
    }

    uploading = true;
    feedback = null;
    const form = new FormData();
    form.append('file', file);
    try {
      const res = await fetch(httpBase + '/upload', { method: 'POST', body: form });
      if (res.status === 415) {
        feedback = {
          text: 'Audio uploads are disabled on this server. Enable the Audio category in Files & Uploads.',
          kind: 'error'
        };
        return;
      }
      if (res.status === 413) {
        feedback = { text: 'That sound is too large to upload.', kind: 'error' };
        return;
      }
      if (!res.ok) throw new Error(`upload failed with status ${res.status}`);
      const data = await res.json();
      if (typeof data.url !== 'string') throw new Error('upload response missing url');
      // Registration is permission-checked server-side; success arrives as an
      // updated sound-list broadcast, errors as an error frame handled above.
      chat.sendRaw({ type: 'add-sound', name: trimmed, url: data.url });
    } catch (e) {
      console.error('sound upload failed', e);
      feedback = { text: 'Sound upload failed. Please try again.', kind: 'error' };
    } finally {
      uploading = false;
    }
  }

  async function renameSound(sound: SoundboardSound) {
    const name = await dialogs.prompt({
      title: 'Rename sound',
      label: 'Name',
      message: `${MIN_SOUND_NAME_LEN}-${MAX_SOUND_NAME_LEN} characters.`,
      initial: sound.name,
      maxLength: MAX_SOUND_NAME_LEN
    });
    if (name === null) return;
    const trimmed = name.trim();
    if (trimmed === sound.name) return;
    if (trimmed.length < MIN_SOUND_NAME_LEN || trimmed.length > MAX_SOUND_NAME_LEN) {
      feedback = {
        text: `Names must be ${MIN_SOUND_NAME_LEN}-${MAX_SOUND_NAME_LEN} characters.`,
        kind: 'error'
      };
      return;
    }
    feedback = null;
    chat.sendRaw({ type: 'rename-sound', id: sound.id, name: trimmed });
  }

  async function deleteSound(sound: SoundboardSound) {
    const confirmed = await dialogs.confirm({
      title: 'Delete sound',
      message: `Delete "${sound.name}" from this server's soundboard? This cannot be undone.`,
      confirmLabel: 'Delete',
      danger: true
    });
    if (!confirmed) return;
    feedback = null;
    chat.sendRaw({ type: 'remove-sound', id: sound.id });
  }

  function toggle() {
    open = !open;
    if (open) {
      feedback = null;
      tweaking = null;
      now = Date.now();
    }
  }
</script>

<div class="soundboard">
  <button
    class="soundboard-button"
    class:active={open}
    onclick={toggle}
    disabled={!inVoice}
    title={inVoice ? 'Soundboard' : 'Join a voice channel to use the soundboard'}
    aria-label="Soundboard"
    aria-expanded={open}
  >
    <svg
      xmlns="http://www.w3.org/2000/svg"
      fill="none"
      viewBox="0 0 24 24"
      stroke-width="1.8"
      stroke="currentColor"
    >
      <path stroke-linecap="round" stroke-linejoin="round" d="M9 18V5l10-2v13" />
      <circle cx="6" cy="18" r="3" />
      <circle cx="16" cy="16" r="3" />
    </svg>
    Soundboard
  </button>
</div>

{#if open}
  <div class="soundboard-panel">
    <div class="panel-head">
      <h4>Soundboard</h4>
      {#if canManage}
        <button class="link-btn" onclick={pickFile} disabled={uploading}>
          {uploading ? 'Uploading…' : 'Add sound'}
        </button>
        <input
          bind:this={fileInput}
          class="hidden-input"
          type="file"
          accept={SOUND_ACCEPT}
          onchange={uploadSound}
        />
      {/if}
    </div>

    {#if !canPlay}
      <p class="panel-note">You do not have permission to play sounds on this server.</p>
    {/if}

    {#if $soundsByPopularity.length > 4}
      <input
        class="sound-search"
        type="search"
        placeholder="Search sounds"
        bind:value={search}
        aria-label="Search sounds"
      />
    {/if}

    {#if filtered.length === 0}
      <p class="panel-note">
        {#if $soundsByPopularity.length === 0}
          No sounds yet.{#if canManage} Add one to get started.{/if}
        {:else}
          No sounds match "{search}".
        {/if}
      </p>
    {:else}
      <div class="sound-grid">
        {#each filtered as sound (sound.id)}
          <div class="sound-cell" class:muted={isMuted(sound)}>
            <button
              class="sound-btn"
              onclick={() => play(sound)}
              oncontextmenu={(e) => {
                e.preventDefault();
                tweaking = tweaking === sound.id ? null : sound.id;
              }}
              disabled={!canPlay || onCooldown || !$soundboardEnabled}
              title={isMuted(sound)
                ? `${sound.name} (muted for you)`
                : `${sound.name} — right-click for volume`}
            >
              <span class="sound-name">{sound.name}</span>
              <span class="sound-plays">{sound.playCount}</span>
            </button>
            <button
              class="sound-menu-btn"
              onclick={() => (tweaking = tweaking === sound.id ? null : sound.id)}
              title="Sound options"
              aria-label={`Options for ${sound.name}`}
            >
              <svg
                xmlns="http://www.w3.org/2000/svg"
                fill="none"
                viewBox="0 0 24 24"
                stroke-width="1.8"
                stroke="currentColor"
              >
                <circle cx="12" cy="5" r="1" />
                <circle cx="12" cy="12" r="1" />
                <circle cx="12" cy="19" r="1" />
              </svg>
            </button>
          </div>

          {#if tweaking === sound.id}
            <div class="sound-options">
              <div class="volume-row">
                <input
                  class="volume-slider"
                  type="range"
                  min="0"
                  max="1"
                  step="0.01"
                  value={soundVolume(sound)}
                  oninput={(e) =>
                    soundboardPrefs.setSoundVolume(sound.id, parseFloat(e.currentTarget.value))}
                  aria-label={`Volume for ${sound.name}`}
                />
                <span class="volume-value">{Math.round(soundVolume(sound) * 100)}%</span>
              </div>
              <div class="option-actions">
                <button
                  class="option-btn"
                  onclick={() => soundboardPrefs.setSoundMuted(sound.id, !isMuted(sound))}
                >
                  {isMuted(sound) ? 'Unmute' : 'Mute'}
                </button>
                {#if canManage}
                  <button class="option-btn" onclick={() => renameSound(sound)}>Rename</button>
                  <button class="option-btn danger" onclick={() => deleteSound(sound)}>
                    Delete
                  </button>
                {/if}
              </div>
              {#if sound.uploadedBy}
                <p class="option-meta">Added by {sound.uploadedBy}</p>
              {/if}
            </div>
          {/if}
        {/each}
      </div>
    {/if}

    <div class="panel-footer">
      <label class="master-toggle">
        <input type="checkbox" bind:checked={$soundboardEnabled} />
        Hear sounds
      </label>
      <div class="volume-row">
        <span class="volume-icon" aria-hidden="true">
          <svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="1.8"
            stroke-linecap="round"
            stroke-linejoin="round"
          >
            <polygon points="11 5 6 9 2 9 2 15 6 15 11 19 11 5" />
            <path d="M15.54 8.46a5 5 0 0 1 0 7.07" />
          </svg>
        </span>
        <input
          class="volume-slider"
          type="range"
          min="0"
          max="1"
          step="0.01"
          bind:value={$soundboardVolume}
          disabled={!$soundboardEnabled}
          aria-label="Soundboard volume"
        />
        <span class="volume-value">{Math.round($soundboardVolume * 100)}%</span>
      </div>
    </div>

    {#if feedback}
      <p class="panel-feedback" class:error={feedback.kind === 'error'}>{feedback.text}</p>
    {/if}
  </div>
{/if}

<style>
  .soundboard {
    display: flex;
  }

  /* Matches the voice/screen-share buttons it sits beside in the sidebar. */
  .soundboard-button {
    flex: 1;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-height: var(--control-height);
    padding: var(--space-1) var(--space-2);
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--color-on-surface-variant);
    font-size: var(--text-sm);
    font-weight: 500;
    cursor: pointer;
  }

  .soundboard-button:not(:disabled):hover {
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
  }

  .soundboard-button.active {
    background: var(--color-primary-container);
    color: var(--color-primary);
  }

  .soundboard-button:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .soundboard-button svg {
    width: 1.125rem;
    height: 1.125rem;
    flex-shrink: 0;
  }

  .soundboard-panel {
    margin-top: var(--space-2);
    padding: var(--space-3);
    background: var(--color-surface-raised);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-md);
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .panel-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    gap: var(--space-2);
  }

  .panel-head h4 {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--color-on-surface);
  }

  .link-btn {
    padding: 0;
    background: none;
    border: none;
    color: var(--color-primary);
    font-size: var(--text-xs);
    cursor: pointer;
  }

  .link-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .hidden-input {
    display: none;
  }

  .sound-search {
    width: 100%;
    font-size: var(--text-sm);
  }

  .sound-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-2);
    max-height: 14rem;
    overflow-y: auto;
  }

  .sound-cell {
    display: flex;
    align-items: stretch;
    gap: 1px;
    min-width: 0;
  }

  .sound-cell.muted {
    opacity: 0.5;
  }

  .sound-btn {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    padding: var(--space-2);
    background: var(--color-surface-elevated);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-sm) 0 0 var(--radius-sm);
    color: var(--color-on-surface);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .sound-btn:not(:disabled):hover {
    border-color: var(--color-primary);
    color: var(--color-primary);
  }

  .sound-btn:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .sound-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sound-plays {
    flex-shrink: 0;
    color: var(--color-muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .sound-menu-btn {
    flex-shrink: 0;
    width: 1.5rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    background: var(--color-surface-elevated);
    border: 1px solid var(--color-surface-outline);
    border-radius: 0 var(--radius-sm) var(--radius-sm) 0;
    color: var(--color-muted);
    cursor: pointer;
  }

  .sound-menu-btn:hover {
    color: var(--color-on-surface);
  }

  .sound-menu-btn svg {
    width: 1rem;
    height: 1rem;
  }

  .sound-options {
    grid-column: 1 / -1;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding: var(--space-2);
    background: var(--color-surface-elevated);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-sm);
  }

  .option-actions {
    display: flex;
    gap: var(--space-2);
  }

  .option-btn {
    flex: 1;
    min-height: 1.75rem;
    padding: 0 var(--space-2);
    background: var(--color-surface-raised);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-sm);
    color: var(--color-on-surface-variant);
    font-size: var(--text-xs);
    cursor: pointer;
  }

  .option-btn:hover {
    border-color: var(--color-outline-strong);
    color: var(--color-on-surface);
  }

  .option-btn.danger:hover {
    border-color: var(--color-danger);
    color: var(--color-danger);
  }

  .option-meta {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-xs);
  }

  .panel-footer {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    padding-top: var(--space-3);
    border-top: 1px solid var(--color-surface-outline);
  }

  .master-toggle {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    color: var(--color-on-surface);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .master-toggle input {
    min-height: 0;
    cursor: pointer;
  }

  .volume-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }

  .volume-icon {
    display: inline-flex;
    flex-shrink: 0;
    color: var(--color-muted);
  }

  .volume-slider {
    flex: 1;
    -webkit-appearance: none;
    appearance: none;
    height: 4px;
    min-height: 0;
    padding: 0;
    border: none;
    border-radius: var(--radius-pill);
    background: var(--color-surface-raised);
    outline: none;
  }

  .volume-slider:focus {
    box-shadow: none;
  }

  .volume-slider:focus-visible {
    outline: 2px solid var(--color-primary);
    outline-offset: 2px;
  }

  .volume-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--color-on-surface);
    border: 2px solid var(--color-primary);
    margin-top: -5px;
  }

  .volume-slider::-moz-range-thumb {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    background: var(--color-on-surface);
    border: 2px solid var(--color-primary);
  }

  .volume-value {
    min-width: 2.5rem;
    text-align: right;
    color: var(--color-muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }

  .panel-note {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-xs);
    line-height: 1.5;
  }

  .panel-feedback {
    margin: 0;
    color: var(--color-muted);
    font-size: var(--text-xs);
    line-height: 1.5;
  }

  .panel-feedback.error {
    color: var(--color-danger);
  }
</style>
