<!--
  Settings modal allowing users to adjust audio devices and check for updates.
  The component renders nothing when the `open` prop is false.
-->
<script lang="ts">
  import { volume, inputDeviceId, outputDeviceId, voiceMode, vadSensitivity, pttKey } from '$lib/stores/settings';
  import { APP_VERSION } from '$lib/version';
  import { onMount } from 'svelte';
  import { PushToTalkManager } from '$lib/voice/ptt';
  export let open: boolean;
  export let close: () => void;

  let updateMessage = '';

  let inputs: MediaDeviceInfo[] = [];
  let outputs: MediaDeviceInfo[] = [];
  let capturingPttKey = false;

  onMount(async () => {
    try {
      const devices = await navigator.mediaDevices.enumerateDevices();
      inputs = devices.filter((d) => d.kind === 'audioinput');
      outputs = devices.filter((d) => d.kind === 'audiooutput');
    } catch (e) {
      console.error('Failed to enumerate devices', e);
    }
  });

  async function checkUpdates() {
    updateMessage = 'Checking...';
    try {
      const res = await fetch(
        'https://api.github.com/repos/Tenosiey/Murmer/releases?per_page=10'
      );
      if (!res.ok) throw new Error('request failed');
      const releases = await res.json();
      if (Array.isArray(releases) && releases.length > 0) {
        const stable = releases.find((r) => !r.prerelease);
        const pre = releases.find((r) => r.prerelease);
        if (pre && pre.tag_name && pre.tag_name !== APP_VERSION) {
          updateMessage = `Pre-release available: ${pre.tag_name}`;
        } else if (stable && stable.tag_name && stable.tag_name !== APP_VERSION) {
          updateMessage = `Update available: ${stable.tag_name}`;
        } else {
          updateMessage = 'You are running the latest version.';
        }
      } else {
        updateMessage = 'No releases found.';
      }
    } catch (e) {
      updateMessage = 'Failed to check for updates.';
    }
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

  async function capturePttKey() {
    capturingPttKey = true;
    try {
      const pttManager = new PushToTalkManager();
      const newKey = await pttManager.captureKey();
      pttKey.set(newKey);
      pttManager.destroy();
    } catch (error) {
      console.error('Failed to capture PTT key:', error);
    } finally {
      capturingPttKey = false;
    }
  }
</script>

{#if open}
  <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
  <!-- svelte-ignore a11y-click-events-have-key-events -->
  <div class="modal-overlay" on:click={close} on:keydown={handleOverlayKeydown} role="dialog" aria-modal="true" aria-labelledby="settings-title" tabindex="-1">
    <!-- svelte-ignore a11y-no-noninteractive-element-interactions -->
    <!-- svelte-ignore a11y-click-events-have-key-events -->
    <!-- svelte-ignore a11y-no-noninteractive-tabindex -->
    <div class="modal-content" on:click|stopPropagation on:keydown={handleKeydown} role="document" tabindex="0">
      <div class="modal-header">
        <h2 id="settings-title">Settings</h2>
        <button class="close-btn" on:click={close} aria-label="Close settings">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>

      <div class="modal-body">
        <div class="settings-section">
          <h3 class="section-title">🔊 Audio Settings</h3>
          
          <div class="setting-group">
            <label for="volume-slider" class="setting-label">
              Volume
              <span class="setting-value">{Math.round($volume * 100)}%</span>
            </label>
            <div class="slider-container">
              <input
                id="volume-slider"
                class="volume-slider"
                type="range"
                min="0"
                max="1"
                step="0.01"
                bind:value={$volume}
              />
              <div class="slider-track-fill" style="width: {$volume * 100}%"></div>
            </div>
          </div>

          <div class="setting-group">
            <label for="input-select" class="setting-label">Input Device (Microphone)</label>
            <div class="select-container">
              <select id="input-select" class="device-select" bind:value={$inputDeviceId}>
                <option value="">Default</option>
                {#each inputs as dev}
                  <option value={dev.deviceId}>{dev.label || dev.deviceId}</option>
                {/each}
              </select>
              <div class="select-arrow">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <polyline points="6,9 12,15 18,9"></polyline>
                </svg>
              </div>
            </div>
          </div>

          <div class="setting-group">
            <label for="output-select" class="setting-label">Output Device (Speakers)</label>
            <div class="select-container">
              <select id="output-select" class="device-select" bind:value={$outputDeviceId}>
                <option value="">Default</option>
                {#each outputs as dev}
                  <option value={dev.deviceId}>{dev.label || dev.deviceId}</option>
                {/each}
              </select>
              <div class="select-arrow">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <polyline points="6,9 12,15 18,9"></polyline>
                </svg>
              </div>
            </div>
          </div>
        </div>

        <div class="settings-section">
          <h3 class="section-title">🎙️ Voice Activation</h3>
          
          <div class="setting-group">
            <label for="voice-mode-select" class="setting-label">Voice Mode</label>
            <div class="select-container">
              <select id="voice-mode-select" class="device-select" bind:value={$voiceMode}>
                <option value="continuous">Always On</option>
                <option value="vad">Voice Activity Detection</option>
                <option value="ptt">Push to Talk</option>
              </select>
              <div class="select-arrow">
                <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                  <polyline points="6,9 12,15 18,9"></polyline>
                </svg>
              </div>
            </div>
          </div>

          {#if $voiceMode === 'vad'}
            <div class="setting-group">
              <label for="vad-sensitivity-slider" class="setting-label">
                VAD Sensitivity
                <span class="setting-value">{Math.round((1 - $vadSensitivity) * 100)}%</span>
              </label>
              <div class="slider-container">
                <input
                  id="vad-sensitivity-slider"
                  class="volume-slider"
                  type="range"
                  min="0.01"
                  max="0.5"
                  step="0.01"
                  bind:value={$vadSensitivity}
                />
                <div class="slider-track-fill" style="width: {(1 - ($vadSensitivity / 0.5)) * 100}%"></div>
              </div>
              <div class="setting-description">
                Higher sensitivity detects quieter speech but may pick up background noise
              </div>
            </div>
          {/if}

          {#if $voiceMode === 'ptt'}
            <div class="setting-group">
              <label class="setting-label">Push-to-Talk Key</label>
              <div class="ptt-key-setting">
                <button 
                  class="ptt-key-button" 
                  class:capturing={capturingPttKey}
                  on:click={capturePttKey}
                  disabled={capturingPttKey}
                >
                  {#if capturingPttKey}
                    Press any key...
                  {:else}
                    {PushToTalkManager.getKeyDisplayName($pttKey)}
                  {/if}
                </button>
                <div class="setting-description">
                  Click the button above and press the key you want to use for push-to-talk
                </div>
              </div>
            </div>
          {/if}
        </div>

        <div class="settings-section">
          <h3 class="section-title">🔄 Updates</h3>
          <div class="setting-group">
            <button class="update-btn" on:click={checkUpdates}>Check for Updates</button>
            {#if updateMessage}
              <div class="update-message" class:success={updateMessage.includes('latest')} class:warning={updateMessage.includes('available')}>
                {updateMessage}
              </div>
            {/if}
          </div>
        </div>
      </div>

      <div class="modal-footer">
        <button class="primary-btn" on:click={close}>Done</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .modal-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: var(--color-overlay);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1000;
    backdrop-filter: blur(4px);
    animation: fadeIn 0.2s ease-out;
  }

  .modal-content {
    background: var(--color-panel-elevated);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-lg);
    border: 1px solid var(--color-border);
    width: 90%;
    max-width: 500px;
    max-height: 80vh;
    overflow: hidden;
    animation: slideIn 0.3s ease-out;
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 1.5rem;
    border-bottom: 1px solid var(--color-border-subtle);
    background: var(--color-panel);
  }

  .modal-header h2 {
    margin: 0;
    font-size: 1.25rem;
    font-weight: 600;
    color: var(--color-text);
  }

  .close-btn {
    background: transparent;
    border: none;
    color: var(--color-text-muted);
    cursor: pointer;
    padding: 0.5rem;
    border-radius: var(--radius-sm);
    transition: var(--transition);
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .close-btn:hover {
    background: var(--color-bg-elevated);
    color: var(--color-text);
  }

  .modal-body {
    padding: 1.5rem;
    max-height: 60vh;
    overflow-y: auto;
  }

  .settings-section {
    margin-bottom: 2rem;
  }

  .settings-section:last-child {
    margin-bottom: 0;
  }

  .section-title {
    margin: 0 0 1rem 0;
    font-size: 1rem;
    font-weight: 600;
    color: var(--color-text);
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }

  .setting-group {
    margin-bottom: 1.5rem;
  }

  .setting-group:last-child {
    margin-bottom: 0;
  }

  .setting-label {
    display: flex;
    justify-content: space-between;
    align-items: center;
    margin-bottom: 0.5rem;
    font-weight: 500;
    color: var(--color-text);
    font-size: 0.9rem;
  }

  .setting-value {
    font-weight: 600;
    color: var(--color-accent);
  }

  .slider-container {
    position: relative;
    height: 8px;
    background: var(--color-panel);
    border: 1px solid var(--color-border-subtle);
    border-radius: 4px;
    margin: 0.25rem 0;
    padding: 0;
  }

  .volume-slider {
    width: 100%;
    height: 8px;
    -webkit-appearance: none;
    appearance: none;
    background: transparent;
    cursor: pointer;
    position: relative;
    z-index: 2;
    margin: 0;
    padding: 0;
    border: none;
    outline: none;
  }

  .volume-slider::-webkit-slider-track {
    width: 100%;
    height: 8px;
    background: transparent;
    border: none;
    border-radius: 4px;
  }

  .volume-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 20px;
    height: 20px;
    background: var(--color-accent);
    border-radius: 50%;
    cursor: pointer;
    box-shadow: var(--shadow-md);
    transition: var(--transition);
    border: 2px solid white;
    margin-top: -6px;
  }

  .volume-slider::-webkit-slider-thumb:hover {
    background: var(--color-accent-hover);
    transform: scale(1.15);
    box-shadow: var(--shadow-lg);
  }

  .volume-slider::-webkit-slider-thumb:active {
    transform: scale(1.1);
  }

  .volume-slider::-moz-range-track {
    width: 100%;
    height: 8px;
    background: transparent;
    border: none;
    border-radius: 4px;
  }

  .volume-slider::-moz-range-thumb {
    width: 20px;
    height: 20px;
    background: var(--color-accent);
    border-radius: 50%;
    cursor: pointer;
    border: 2px solid white;
    box-shadow: var(--shadow-md);
    transition: var(--transition);
  }

  .volume-slider::-moz-range-thumb:hover {
    background: var(--color-accent-hover);
    transform: scale(1.15);
    box-shadow: var(--shadow-lg);
  }

  .volume-slider::-moz-range-thumb:active {
    transform: scale(1.1);
  }

  .slider-track-fill {
    position: absolute;
    top: 1px;
    left: 1px;
    height: 6px;
    background: linear-gradient(90deg, var(--color-accent), var(--color-accent-hover));
    border-radius: 3px;
    transition: width 0.15s ease;
    pointer-events: none;
    z-index: 1;
  }

  .select-container {
    position: relative;
  }

  .device-select {
    width: 100%;
    padding: 0.75rem 2.5rem 0.75rem 0.75rem;
    background: var(--color-panel);
    border: 1px solid var(--color-border-subtle);
    border-radius: var(--radius-sm);
    color: var(--color-text);
    font-size: 0.9rem;
    cursor: pointer;
    transition: var(--transition);
    appearance: none;
    -webkit-appearance: none;
    -moz-appearance: none;
  }

  .device-select:focus {
    outline: none;
    border-color: var(--color-accent);
    box-shadow: 0 0 0 3px rgba(124, 58, 237, 0.1);
  }

  .device-select:hover {
    border-color: var(--color-border);
  }

  .select-arrow {
    position: absolute;
    right: 0.75rem;
    top: 50%;
    transform: translateY(-50%);
    color: var(--color-text-muted);
    pointer-events: none;
    transition: var(--transition);
  }

  .device-select:focus + .select-arrow {
    color: var(--color-accent);
  }

  .update-btn {
    padding: 0.75rem 1.5rem;
    background: var(--color-accent-alt);
    color: white;
    border: none;
    border-radius: var(--radius-sm);
    font-weight: 500;
    cursor: pointer;
    transition: var(--transition);
    font-size: 0.9rem;
  }

  .update-btn:hover {
    background: var(--color-accent-hover);
    transform: translateY(-1px);
    box-shadow: var(--shadow-sm);
  }

  .update-message {
    margin-top: 0.75rem;
    padding: 0.75rem;
    border-radius: var(--radius-sm);
    font-size: 0.9rem;
    background: var(--color-panel);
    border: 1px solid var(--color-border-subtle);
    color: var(--color-text-muted);
  }

  .update-message.success {
    background: rgba(16, 185, 129, 0.1);
    border-color: var(--color-success);
    color: var(--color-success);
  }

  .update-message.warning {
    background: rgba(245, 158, 11, 0.1);
    border-color: var(--color-warning);
    color: var(--color-warning);
  }

  .modal-footer {
    padding: 1.5rem;
    border-top: 1px solid var(--color-border-subtle);
    background: var(--color-panel);
    display: flex;
    justify-content: flex-end;
  }

  .primary-btn {
    padding: 0.75rem 2rem;
    background: var(--color-accent);
    color: white;
    border: none;
    border-radius: var(--radius-sm);
    font-weight: 500;
    cursor: pointer;
    transition: var(--transition);
    font-size: 0.9rem;
  }

  .primary-btn:hover {
    background: var(--color-accent-hover);
    transform: translateY(-1px);
    box-shadow: var(--shadow-sm);
  }

  @keyframes fadeIn {
    from { opacity: 0; }
    to { opacity: 1; }
  }

  @keyframes slideIn {
    from { 
      opacity: 0;
      transform: translateY(-20px) scale(0.95);
    }
    to { 
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  .setting-description {
    font-size: 0.8rem;
    color: var(--color-text-muted);
    margin-top: 0.5rem;
    line-height: 1.4;
  }

  .ptt-key-setting {
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }

  .ptt-key-button {
    padding: 0.75rem 1rem;
    background: var(--color-panel);
    border: 2px solid var(--color-border-subtle);
    color: var(--color-text);
    border-radius: var(--radius-sm);
    cursor: pointer;
    font-weight: 500;
    transition: var(--transition);
    min-height: 48px;
    display: flex;
    align-items: center;
    justify-content: center;
  }

  .ptt-key-button:hover:not(:disabled) {
    background: var(--color-bg-elevated);
    border-color: var(--color-accent);
  }

  .ptt-key-button.capturing {
    background: var(--color-accent);
    color: white;
    border-color: var(--color-accent);
    animation: pulse 1.5s infinite;
  }

  .ptt-key-button:disabled {
    cursor: not-allowed;
    opacity: 0.7;
  }

  @keyframes pulse {
    0%, 100% { opacity: 1; }
    50% { opacity: 0.7; }
  }

  /* Mobile responsiveness */
  @media (max-width: 640px) {
    .modal-content {
      width: 95%;
      max-height: 85vh;
    }

    .modal-header,
    .modal-body,
    .modal-footer {
      padding: 1rem;
    }

    .settings-section {
      margin-bottom: 1.5rem;
    }

    .setting-group {
      margin-bottom: 1rem;
    }
  }
</style>
