<!--
  Camera controls: switch the webcam on for the voice channel, and pick which
  camera and at what quality. Sits under the screen-share controls in the
  channel sidebar and is styled to match them.
-->
<script lang="ts">
  import {
    cameraOn,
    cameraDeviceId,
    cameraQuality,
    mirrorSelfView,
    toggleCamera
  } from '$lib/stores/webcam';
  import { videoInputs, refreshVideoDevices } from '$lib/stores/videoDevices';
  import { CAMERA_QUALITIES, type CameraQuality } from '$lib/voice/camera';
  import { session } from '$lib/stores/session';
  import { dialogs } from '$lib/stores/dialogs';

  interface Props {
    currentVoiceChannel?: number | null;
    inVoice?: boolean;
  }

  let { currentVoiceChannel = null, inVoice = false }: Props = $props();

  let showSettings = $state(false);
  /** Held while the camera is opening: the prompt can take a while. */
  let busy = $state(false);

  async function openSettings() {
    showSettings = !showSettings;
    // Labels only exist once camera permission has been granted, so this is
    // usually a list of device ids until the first time the camera runs.
    if (showSettings) await refreshVideoDevices();
  }

  async function toggle() {
    if (busy) return;
    if (!inVoice || currentVoiceChannel === null || !$session.user) {
      await dialogs.alert({
        title: 'Join a voice channel first',
        message: 'You must be in a voice channel to turn your camera on.'
      });
      return;
    }
    busy = true;
    try {
      await toggleCamera($session.user, currentVoiceChannel);
    } catch (error) {
      console.error('Failed to start the camera:', error);
      await dialogs.alert({
        title: 'Camera unavailable',
        message:
          'Could not start your camera. Check that it is plugged in and that ' +
          'Murmer is allowed to use it.'
      });
    } finally {
      busy = false;
    }
  }
</script>

<div class="webcam-controls">
  <button
    class="webcam-button"
    class:active={$cameraOn}
    onclick={toggle}
    disabled={!inVoice || busy}
    title={inVoice
      ? $cameraOn
        ? 'Turn your camera off'
        : 'Turn your camera on'
      : 'Join a voice channel to use your camera'}
    aria-label={$cameraOn ? 'Turn your camera off' : 'Turn your camera on'}
    aria-pressed={$cameraOn}
  >
    {#if $cameraOn}
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M16 16v2a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h2"/><path d="M22 8l-6 4 6 4V8z"/><line x1="2" y1="2" x2="22" y2="22"/></svg>
    {:else}
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><polygon points="23 7 16 12 23 17 23 7"/><rect x="1" y="5" width="15" height="14" rx="2" ry="2"/></svg>
    {/if}
    {$cameraOn ? 'Stop Video' : 'Start Video'}
  </button>

  <button
    class="settings-button"
    class:active={showSettings}
    onclick={openSettings}
    title="Camera settings"
    aria-label="Camera settings"
    aria-expanded={showSettings}
  >
    <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><circle cx="12" cy="12" r="3"/><path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/></svg>
  </button>
</div>

{#if showSettings}
  <div class="settings-panel">
    <h4>Camera</h4>

    <label class="field-label">
      Device
      <select
        class="field"
        value={$cameraDeviceId ?? ''}
        onchange={(e) => cameraDeviceId.set((e.currentTarget as HTMLSelectElement).value || null)}
      >
        <option value="">System default</option>
        {#each $videoInputs as device (device.deviceId)}
          <option value={device.deviceId}>{device.label || device.deviceId}</option>
        {/each}
      </select>
    </label>

    <div class="preset-buttons" role="group" aria-label="Camera quality">
      {#each CAMERA_QUALITIES as quality (quality)}
        <button
          class="preset-button"
          class:selected={$cameraQuality === quality}
          onclick={() => cameraQuality.set(quality as CameraQuality)}
        >
          {quality}
        </button>
      {/each}
    </div>

    <label class="checkbox">
      <input type="checkbox" bind:checked={$mirrorSelfView} />
      Mirror my own preview
    </label>

    <p class="settings-note">
      Video is sent to everyone in the channel directly, one stream per person —
      a lower resolution is the cheaper choice in a busy channel. Changes apply
      to a running camera straight away.
    </p>
  </div>
{/if}

<style>
  .webcam-controls {
    display: flex;
    gap: var(--space-1);
    margin-top: var(--space-1);
  }

  /* Matches the screen-share and voice-control buttons stacked above these
     in the channel sidebar. */
  .webcam-button {
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

  .webcam-button:not(:disabled):hover {
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
  }

  .webcam-button.active {
    background: var(--color-primary-container);
    color: var(--color-primary);
  }

  .webcam-button:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .webcam-button svg {
    width: 1.125rem;
    height: 1.125rem;
    flex-shrink: 0;
  }

  .settings-button {
    width: var(--control-height);
    min-height: var(--control-height);
    flex-shrink: 0;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
    background: transparent;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--color-muted);
    cursor: pointer;
  }

  .settings-button:hover {
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
  }

  .settings-button.active {
    background: var(--color-primary-container);
    color: var(--color-primary);
  }

  .settings-button svg {
    width: 1.125rem;
    height: 1.125rem;
    display: block;
  }

  .settings-panel {
    margin-top: var(--space-2);
    padding: var(--space-3);
    background: var(--color-surface-raised);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-md);
  }

  .settings-panel h4 {
    margin: 0 0 var(--space-3) 0;
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--color-on-surface);
  }

  .field-label {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    color: var(--color-muted);
    font-size: var(--text-sm);
  }

  .field-label select {
    width: 100%;
  }

  .preset-buttons {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin-top: var(--space-3);
  }

  .preset-button {
    padding: var(--space-1) var(--space-3);
    background: var(--color-surface-elevated);
    border: 1px solid var(--color-surface-outline);
    border-radius: var(--radius-sm);
    color: var(--color-muted);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .preset-button:hover {
    border-color: var(--color-outline-strong);
    color: var(--color-on-surface);
  }

  .preset-button.selected {
    background: var(--color-primary);
    border-color: var(--color-primary);
    color: var(--color-on-primary);
  }

  .checkbox {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-top: var(--space-3);
    color: var(--color-on-surface);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .checkbox input {
    cursor: pointer;
    min-height: 0;
  }

  .settings-note {
    margin: var(--space-3) 0 0 0;
    color: var(--color-muted);
    font-size: var(--text-xs);
    line-height: 1.5;
  }
</style>
