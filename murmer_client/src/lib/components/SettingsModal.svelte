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
              <label class="setting-label" for="ptt-key-button">Push-to-Talk Key</label>
              <div class="ptt-key-setting">
                <button
                  id="ptt-key-button"
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
    inset: 0;
    background: color-mix(in srgb, var(--color-overlay) 92%, rgba(5, 10, 26, 0.75));
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 1200;
    backdrop-filter: blur(16px);
    animation: fadeIn 0.24s ease-out;
  }

  .modal-content {
    background: color-mix(in srgb, var(--color-surface-elevated) 88%, transparent);
    border-radius: var(--radius-lg);
    box-shadow: var(--shadow-md);
    border: 1px solid var(--color-surface-outline);
    width: min(540px, 92vw);
    max-height: 82vh;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    animation: slideIn 0.28s cubic-bezier(0.25, 0.9, 0.3, 1.2);
    backdrop-filter: var(--blur-elevated);
  }

  .modal-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: clamp(1.25rem, 4vw, 1.5rem);
    border-bottom: 1px solid var(--color-surface-outline);
    background: color-mix(in srgb, var(--color-surface-raised) 86%, transparent);
  }

  .modal-header h2 {
    margin: 0;
    font-size: 1.3rem;
    letter-spacing: -0.01em;
  }

  .close-btn {
    background: transparent;
    border: 1px solid transparent;
    color: var(--color-muted);
    cursor: pointer;
    padding: 0.45rem;
    border-radius: var(--radius-sm);
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .close-btn:hover,
  .close-btn:focus-visible {
    border-color: color-mix(in srgb, var(--color-muted) 30%, transparent);
    color: var(--color-on-surface);
    background: color-mix(in srgb, var(--color-primary) 10%, transparent);
  }

  .modal-body {
    padding: clamp(1.25rem, 4vw, 1.75rem);
    max-height: 62vh;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2rem;
  }

  .settings-section {
    display: grid;
    gap: 1.25rem;
  }

  .section-title {
    margin: 0;
    font-size: 1rem;
    font-weight: 600;
    letter-spacing: 0.03em;
    color: var(--color-secondary);
  }

  .setting-group {
    display: grid;
    gap: 0.75rem;
  }

  .setting-label {
    display: flex;
    justify-content: space-between;
    align-items: center;
    font-weight: 600;
    color: var(--color-on-surface);
    font-size: 0.92rem;
  }

  .setting-value {
    font-size: 0.85rem;
    color: var(--color-secondary);
  }

  .setting-description {
    font-size: 0.8rem;
    color: var(--color-muted);
    line-height: 1.5;
  }

  .slider-container {
    position: relative;
    height: 10px;
    background: color-mix(in srgb, var(--color-surface-raised) 86%, transparent);
    border-radius: 999px;
    border: 1px solid var(--color-surface-outline);
    overflow: hidden;
  }

  .slider-track-fill {
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    background: linear-gradient(90deg, var(--color-primary), var(--color-secondary));
    border-radius: inherit;
    pointer-events: none;
  }

  .volume-slider {
    width: 100%;
    height: 100%;
    -webkit-appearance: none;
    appearance: none;
    background: transparent;
    position: relative;
    z-index: 2;
  }

  .volume-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--color-on-primary);
    border: 3px solid var(--color-primary);
    box-shadow: var(--shadow-sm);
    cursor: pointer;
    transition: transform var(--transition);
  }

  .volume-slider::-webkit-slider-thumb:hover {
    transform: scale(1.05);
  }

  .volume-slider::-moz-range-thumb {
    width: 22px;
    height: 22px;
    border-radius: 50%;
    background: var(--color-on-primary);
    border: 3px solid var(--color-primary);
    box-shadow: var(--shadow-sm);
    cursor: pointer;
    transition: transform var(--transition);
  }

  .volume-slider::-moz-range-thumb:hover {
    transform: scale(1.05);
  }

  .select-container {
    position: relative;
  }

  .device-select {
    width: 100%;
    padding: 0.75rem 1rem;
    border-radius: var(--radius-md);
    border: 1px solid var(--color-surface-outline);
    background: color-mix(in srgb, var(--color-surface-elevated) 88%, transparent);
    color: var(--color-on-surface);
    appearance: none;
  }

  .select-arrow {
    position: absolute;
    right: 0.9rem;
    top: 50%;
    transform: translateY(-50%);
    pointer-events: none;
    color: var(--color-muted);
  }

  .ptt-key-button,
  .update-btn,
  .primary-btn {
    border-radius: var(--radius-sm);
    font-weight: 600;
    padding: 0.75rem 1.1rem;
    border: 1px solid transparent;
    background: color-mix(in srgb, var(--color-primary) 12%, transparent);
    color: var(--color-secondary);
    cursor: pointer;
    transition: var(--transition);
  }

  .ptt-key-button.capturing {
    background: color-mix(in srgb, var(--color-warning) 26%, transparent);
    color: var(--color-on-surface);
  }

  .ptt-key-button:disabled {
    opacity: 0.6;
    cursor: wait;
  }

  .update-btn {
    justify-self: start;
  }

  .update-message {
    font-size: 0.85rem;
    color: var(--color-muted);
  }

  .update-message.success {
    color: var(--color-success);
  }

  .update-message.warning {
    color: var(--color-warning);
  }

  .primary-btn {
    background: linear-gradient(135deg, var(--color-primary), var(--color-secondary));
    color: var(--color-on-primary);
    border: none;
    padding-inline: 1.5rem;
  }

  .ptt-key-button:hover,
  .update-btn:hover,
  .primary-btn:hover {
    transform: translateY(-1px);
    box-shadow: var(--shadow-xs);
  }

  .modal-footer {
    padding: clamp(1rem, 3vw, 1.5rem);
    border-top: 1px solid var(--color-surface-outline);
    display: flex;
    justify-content: flex-end;
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
      transform: translateY(12px);
      opacity: 0;
    }
    to {
      transform: translateY(0);
      opacity: 1;
    }
  }
</style>

