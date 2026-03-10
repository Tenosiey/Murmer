<!--
  Settings modal allowing users to adjust audio devices and check for updates.
  The component renders nothing when the `open` prop is false.
-->
<script lang="ts">
  import { volume, inputDeviceId, outputDeviceId, voiceMode, vadSensitivity, pttKey } from '$lib/stores/settings';
  import { APP_VERSION } from '$lib/version';
  import { loadKeyPair } from '$lib/keypair';
  import { onMount } from 'svelte';
  import { PushToTalkManager } from '$lib/voice/ptt';
  export let open: boolean;
  export let close: () => void;

  let updateMessage = '';
  let publicKey = '';
  let keyCopied = false;

  let inputs: MediaDeviceInfo[] = [];
  let outputs: MediaDeviceInfo[] = [];
  let capturingPttKey = false;

  onMount(async () => {
    try {
      const kp = loadKeyPair();
      publicKey = kp.publicKey;
    } catch (e) {
      console.error('Failed to load key pair', e);
    }
    try {
      const devices = await navigator.mediaDevices.enumerateDevices();
      inputs = devices.filter((d) => d.kind === 'audioinput');
      outputs = devices.filter((d) => d.kind === 'audiooutput');
    } catch (e) {
      console.error('Failed to enumerate devices', e);
    }
  });

  async function copyPublicKey() {
    try {
      await navigator.clipboard.writeText(publicKey);
      keyCopied = true;
      setTimeout(() => { keyCopied = false; }, 2000);
    } catch (e) {
      console.error('Failed to copy public key', e);
    }
  }

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
          <h3 class="section-title">🔑 Identity</h3>
          <div class="setting-group">
            <label class="setting-label" for="public-key-display">Public Key</label>
            <div class="pubkey-row">
              <input
                id="public-key-display"
                class="pubkey-input"
                type="text"
                readonly
                value={publicKey}
              />
              <button class="copy-btn" on:click={copyPublicKey} title="Copy public key">
                {#if keyCopied}
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <polyline points="20,6 9,17 4,12"></polyline>
                  </svg>
                {:else}
                  <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                    <rect x="9" y="9" width="13" height="13" rx="2" ry="2"></rect>
                    <path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"></path>
                  </svg>
                {/if}
              </button>
            </div>
            <div class="setting-description">
              Your Ed25519 public key identifies you on the server. Share it with the server admin to receive a role.
            </div>
          </div>
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
    gap: 0.5rem;
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
    height: 2.25rem;
    display: flex;
    align-items: center;
  }

  .slider-container::before {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    height: 6px;
    top: 50%;
    transform: translateY(-50%);
    background: color-mix(in srgb, var(--color-surface-raised) 86%, transparent);
    border-radius: 999px;
    border: 1px solid var(--color-surface-outline);
    pointer-events: none;
  }

  .slider-track-fill {
    position: absolute;
    left: 0;
    height: 6px;
    top: 50%;
    transform: translateY(-50%);
    background: linear-gradient(90deg, var(--color-primary), var(--color-secondary));
    border-radius: 999px;
    pointer-events: none;
    z-index: 1;
  }

  .volume-slider {
    width: 100%;
    height: 100%;
    -webkit-appearance: none;
    appearance: none;
    background: transparent;
    position: relative;
    z-index: 2;
    margin: 0;
    padding: 0;
    cursor: pointer;
  }

  .volume-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--color-on-primary);
    border: 2.5px solid var(--color-primary);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.25);
    cursor: pointer;
    transition: transform var(--transition), box-shadow var(--transition);
  }

  .volume-slider::-webkit-slider-thumb:hover {
    transform: scale(1.12);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
  }

  .volume-slider::-moz-range-thumb {
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--color-on-primary);
    border: 2.5px solid var(--color-primary);
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.25);
    cursor: pointer;
    transition: transform var(--transition), box-shadow var(--transition);
  }

  .volume-slider::-moz-range-thumb:hover {
    transform: scale(1.12);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.3);
  }

  .volume-slider::-moz-range-track {
    background: transparent;
    border: none;
  }

  .pubkey-row {
    display: flex;
    gap: 0.5rem;
    align-items: center;
  }

  .pubkey-input {
    flex: 1;
    padding: 0.65rem 0.85rem;
    border-radius: var(--radius-md);
    border: 1px solid var(--color-surface-outline);
    background: color-mix(in srgb, var(--color-surface-elevated) 88%, transparent);
    color: var(--color-muted);
    font-family: 'Courier New', Courier, monospace;
    font-size: 0.78rem;
    cursor: text;
    user-select: all;
  }

  .copy-btn {
    flex-shrink: 0;
    background: color-mix(in srgb, var(--color-primary) 12%, transparent);
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    color: var(--color-secondary);
    cursor: pointer;
    padding: 0.55rem;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    transition: var(--transition);
  }

  .copy-btn:hover {
    transform: translateY(-1px);
    box-shadow: var(--shadow-xs);
    border-color: color-mix(in srgb, var(--color-primary) 30%, transparent);
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

