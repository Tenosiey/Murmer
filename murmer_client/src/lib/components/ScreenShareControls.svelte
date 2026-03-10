<!--
  Screen Share Controls Component
  
  Provides UI for starting/stopping screen sharing and configuring quality settings.
-->
<script lang="ts">
  import { isScreenSharing, startScreenShare, stopScreenShare, screenShareSettings } from '$lib/stores/screenShare';
  import { session } from '$lib/stores/session';
  import { QUALITY_PRESETS, type QualityPreset } from '$lib/screenshare/manager';
  
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
        alert('You must be in a voice channel to share your screen');
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
        alert('Failed to start screen sharing. Please ensure you granted permission.');
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
    gap: 0.5rem;
    margin-top: 0.5rem;
  }

  .screenshare-button {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    padding: 0.75rem 1rem;
    background: var(--bg-secondary, #2a2a2a);
    border: 1px solid var(--border-color, #404040);
    border-radius: 8px;
    color: var(--text-primary, #ffffff);
    font-size: 0.9rem;
    cursor: pointer;
    transition: all 0.2s;
  }

  .screenshare-button:not(:disabled):hover {
    background: var(--bg-hover, #3a3a3a);
    border-color: var(--accent-color, #5865f2);
  }

  .screenshare-button.active {
    background: var(--accent-color, #5865f2);
    border-color: var(--accent-color, #5865f2);
  }

  .screenshare-button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .screenshare-button svg {
    width: 1.25rem;
    height: 1.25rem;
  }

  .settings-button {
    padding: 0.75rem;
    background: var(--bg-secondary, #2a2a2a);
    border: 1px solid var(--border-color, #404040);
    border-radius: 8px;
    color: var(--text-secondary, #b0b0b0);
    cursor: pointer;
    transition: all 0.2s;
  }

  .settings-button:not(:disabled):hover {
    background: var(--bg-hover, #3a3a3a);
    color: var(--text-primary, #ffffff);
  }

  .settings-button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .settings-button svg {
    width: 1.25rem;
    height: 1.25rem;
    display: block;
  }

  .settings-panel {
    margin-top: 1rem;
    padding: 1rem;
    background: var(--bg-secondary, #2a2a2a);
    border: 1px solid var(--border-color, #404040);
    border-radius: 8px;
  }

  .settings-panel h4 {
    margin: 0 0 1rem 0;
    font-size: 1rem;
    font-weight: 600;
    color: var(--text-primary, #ffffff);
  }

  .preset-buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    margin-bottom: 1rem;
  }

  .preset-button {
    padding: 0.5rem 1rem;
    background: var(--bg-tertiary, #1a1a1a);
    border: 1px solid var(--border-color, #404040);
    border-radius: 6px;
    color: var(--text-secondary, #b0b0b0);
    font-size: 0.85rem;
    cursor: pointer;
    transition: all 0.2s;
  }

  .preset-button:hover {
    background: var(--bg-hover, #3a3a3a);
    color: var(--text-primary, #ffffff);
  }

  .preset-button.selected {
    background: var(--accent-color, #5865f2);
    border-color: var(--accent-color, #5865f2);
    color: #ffffff;
  }

  .custom-settings {
    margin-top: 1rem;
    padding-top: 1rem;
    border-top: 1px solid var(--border-color, #404040);
  }

  .custom-settings > label {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    margin-bottom: 0.75rem;
    color: var(--text-primary, #ffffff);
    font-size: 0.9rem;
    cursor: pointer;
  }

  .custom-inputs {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
    margin-top: 0.75rem;
  }

  .custom-inputs label {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    color: var(--text-secondary, #b0b0b0);
    font-size: 0.85rem;
  }

  .custom-inputs input[type="number"] {
    padding: 0.5rem;
    background: var(--bg-tertiary, #1a1a1a);
    border: 1px solid var(--border-color, #404040);
    border-radius: 6px;
    color: var(--text-primary, #ffffff);
    font-size: 0.9rem;
  }

  .custom-inputs input[type="number"]:focus {
    outline: none;
    border-color: var(--accent-color, #5865f2);
  }

  .settings-note {
    margin: 1rem 0 0 0;
    padding: 0.75rem;
    background: var(--bg-tertiary, #1a1a1a);
    border-radius: 6px;
    color: var(--text-secondary, #b0b0b0);
    font-size: 0.8rem;
    line-height: 1.4;
  }

  input[type="checkbox"] {
    cursor: pointer;
  }
</style>
