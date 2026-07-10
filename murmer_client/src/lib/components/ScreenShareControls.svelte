<!--
  Screen Share Controls Component
  
  Provides UI for starting/stopping screen sharing and configuring quality settings.
-->
<script lang="ts">
  import { isScreenSharing, startScreenShare, stopScreenShare, screenShareSettings } from '$lib/stores/screenShare';
  import { session } from '$lib/stores/session';
  import { QUALITY_PRESETS, type QualityPreset } from '$lib/screenshare/manager';
  import { dialogs } from '$lib/stores/dialogs';
  
  export let currentVoiceChannel: number | null = null;
  export let inVoice: boolean = false;

  let showSettings = false;
  let selectedPreset: QualityPreset = '720p';
  let customWidth = 1280;
  let customHeight = 720;
  let customFrameRate = 30;
  let useCustom = false;

  async function toggleScreenShare() {
    if ($isScreenSharing) {
      stopScreenShare();
    } else {
      if (!inVoice || !currentVoiceChannel || !$session.user) {
        await dialogs.alert({
          title: 'Join a voice channel first',
          message: 'You must be in a voice channel to share your screen.'
        });
        return;
      }

      try {
        // Apply selected settings
        if (useCustom) {
          screenShareSettings.set({
            width: customWidth,
            height: customHeight,
            frameRate: customFrameRate
          });
        } else {
          screenShareSettings.set(QUALITY_PRESETS[selectedPreset]);
        }

        await startScreenShare($session.user, currentVoiceChannel);
      } catch (error) {
        console.error('Failed to start screen share:', error);
        dialogs.alert({
          title: 'Screen sharing failed',
          message: 'Could not start screen sharing. Please ensure you granted permission.'
        });
      }
    }
  }

  function applyPreset(preset: QualityPreset) {
    selectedPreset = preset;
    const settings = QUALITY_PRESETS[preset];
    customWidth = settings.width;
    customHeight = settings.height;
    customFrameRate = settings.frameRate;
    useCustom = false;
  }
</script>

<div class="screenshare-controls">
  <button
    class="screenshare-button"
    class:active={$isScreenSharing}
    on:click={toggleScreenShare}
    disabled={!inVoice}
    title={inVoice ? ($isScreenSharing ? 'Stop sharing screen' : 'Share screen') : 'Join a voice channel to share screen'}
    aria-label={$isScreenSharing ? 'Stop sharing screen' : 'Share screen'}
  >
    <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
      <path stroke-linecap="round" stroke-linejoin="round" d="M9 17.25v1.007a3 3 0 01-.879 2.122L7.5 21h9l-.621-.621A3 3 0 0115 18.257V17.25m6-12V15a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 15V5.25m18 0A2.25 2.25 0 0018.75 3H5.25A2.25 2.25 0 003 5.25m18 0V12a2.25 2.25 0 01-2.25 2.25H5.25A2.25 2.25 0 013 12V5.25" />
    </svg>
    {$isScreenSharing ? 'Stop Sharing' : 'Share Screen'}
  </button>

  <button
    class="settings-button"
    on:click={() => showSettings = !showSettings}
    title="Screen share settings"
    aria-label="Screen share settings"
    disabled={$isScreenSharing}
  >
    <svg xmlns="http://www.w3.org/2000/svg" fill="none" viewBox="0 0 24 24" stroke-width="1.5" stroke="currentColor">
      <path stroke-linecap="round" stroke-linejoin="round" d="M9.594 3.94c.09-.542.56-.94 1.11-.94h2.593c.55 0 1.02.398 1.11.94l.213 1.281c.063.374.313.686.645.87.074.04.147.083.22.127.324.196.72.257 1.075.124l1.217-.456a1.125 1.125 0 011.37.49l1.296 2.247a1.125 1.125 0 01-.26 1.431l-1.003.827c-.293.24-.438.613-.431.992a6.759 6.759 0 010 .255c-.007.378.138.75.43.99l1.005.828c.424.35.534.954.26 1.43l-1.298 2.247a1.125 1.125 0 01-1.369.491l-1.217-.456c-.355-.133-.75-.072-1.076.124a6.57 6.57 0 01-.22.128c-.331.183-.581.495-.644.869l-.213 1.28c-.09.543-.56.941-1.11.941h-2.594c-.55 0-1.02-.398-1.11-.94l-.213-1.281c-.062-.374-.312-.686-.644-.87a6.52 6.52 0 01-.22-.127c-.325-.196-.72-.257-1.076-.124l-1.217.456a1.125 1.125 0 01-1.369-.49l-1.297-2.247a1.125 1.125 0 01.26-1.431l1.004-.827c.292-.24.437-.613.43-.992a6.932 6.932 0 010-.255c.007-.378-.138-.75-.43-.99l-1.004-.828a1.125 1.125 0 01-.26-1.43l1.297-2.247a1.125 1.125 0 011.37-.491l1.216.456c.356.133.751.072 1.076-.124.072-.044.146-.087.22-.128.332-.183.582-.495.644-.869l.214-1.281z" />
      <path stroke-linecap="round" stroke-linejoin="round" d="M15 12a3 3 0 11-6 0 3 3 0 016 0z" />
    </svg>
  </button>
</div>

{#if showSettings}
  <div class="settings-panel">
    <h4>Screen Share Quality</h4>
    
    <div class="preset-buttons">
      {#each Object.keys(QUALITY_PRESETS) as preset}
        <button
          class="preset-button"
          class:selected={!useCustom && selectedPreset === preset}
          on:click={() => applyPreset(preset as QualityPreset)}
        >
          {preset}
        </button>
      {/each}
    </div>

    <div class="custom-settings">
      <label>
        <input type="checkbox" bind:checked={useCustom} />
        Custom Settings
      </label>

      {#if useCustom}
        <div class="custom-inputs">
          <label>
            Width
            <input type="number" bind:value={customWidth} min="640" max="3840" step="1" />
          </label>
          <label>
            Height
            <input type="number" bind:value={customHeight} min="480" max="2160" step="1" />
          </label>
          <label>
            Frame Rate
            <input type="number" bind:value={customFrameRate} min="15" max="60" step="5" />
          </label>
        </div>
      {/if}
    </div>

    <p class="settings-note">
      Note: Settings only apply when starting a new screen share. Higher quality requires more bandwidth.
    </p>
  </div>
{/if}

<style>
  .screenshare-controls {
    display: flex;
    gap: var(--space-1);
  }

  /* Styled to match the voice-control buttons that sit right above
     these controls in the channel sidebar. */
  .screenshare-button {
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

  .screenshare-button:not(:disabled):hover {
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
  }

  .screenshare-button.active {
    background: var(--color-primary-container);
    color: var(--color-primary);
  }

  .screenshare-button:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .screenshare-button svg {
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

  .settings-button:not(:disabled):hover {
    background: var(--color-surface-raised);
    color: var(--color-on-surface);
  }

  .settings-button:disabled {
    opacity: 0.45;
    cursor: default;
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

  .preset-buttons {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin-bottom: var(--space-3);
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

  .custom-settings {
    margin-top: var(--space-3);
    padding-top: var(--space-3);
    border-top: 1px solid var(--color-surface-outline);
  }

  .custom-settings > label {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin-bottom: var(--space-2);
    color: var(--color-on-surface);
    font-size: var(--text-sm);
    cursor: pointer;
  }

  .custom-inputs {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }

  .custom-inputs label {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    color: var(--color-muted);
    font-size: var(--text-sm);
  }

  .settings-note {
    margin: var(--space-3) 0 0 0;
    color: var(--color-muted);
    font-size: var(--text-xs);
    line-height: 1.5;
  }

  input[type="checkbox"] {
    cursor: pointer;
    min-height: 0;
  }
</style>
