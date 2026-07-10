<!--
  Settings modal allowing users to adjust audio devices and check for updates.
  The component renders nothing when the `open` prop is false.
-->
<script lang="ts">
  import {
    volume,
    inputDeviceId,
    outputDeviceId,
    voiceMode,
    vadSensitivity,
    pttKey,
    echoCancellation,
    noiseSuppression,
    autoGainControl
  } from '$lib/stores/settings';
  import { APP_VERSION } from '$lib/version';
  import { theme, accent, DEFAULT_ACCENT } from '$lib/stores/theme';
  import ThemeWheel from '$lib/components/ThemeWheel.svelte';
  import { loadKeyPair } from '$lib/keypair';
  import { onMount } from 'svelte';
  import { PushToTalkManager } from '$lib/voice/ptt';
  import { check } from '@tauri-apps/plugin-updater';
  import { relaunch } from '@tauri-apps/plugin-process';
  import { dialogs } from '$lib/stores/dialogs';
  export let open: boolean;
  export let close: () => void;

  let updateMessage = '';
  let updating = false;
  let publicKey = '';
  let keyCopied = false;

  let inputs: MediaDeviceInfo[] = [];
  let outputs: MediaDeviceInfo[] = [];
  let capturingPttKey = false;

  // Preset theme colors shown next to the wheel; each is a wheel position.
  const ACCENT_PRESETS = [
    { name: 'Sky', hue: 215, saturation: 78 },
    { name: 'Indigo', hue: 250, saturation: 68 },
    { name: 'Violet', hue: 285, saturation: 68 },
    { name: 'Rose', hue: 340, saturation: 72 },
    { name: 'Ember', hue: 22, saturation: 80 },
    { name: 'Moss', hue: 150, saturation: 55 }
  ];

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
    if (updating) return;
    updating = true;
    updateMessage = 'Checking...';
    try {
      const update = await check();
      if (!update) {
        updateMessage = 'You are running the latest version.';
        return;
      }
      updateMessage = `Update available: ${update.version}`;
      const install = await dialogs.confirm({
        title: 'Install update?',
        message: `Version ${update.version} is available (you have ${APP_VERSION}). The app will restart after installing.`,
        confirmLabel: 'Install'
      });
      if (!install) return;
      let contentLength = 0;
      let downloaded = 0;
      await update.downloadAndInstall((event) => {
        if (event.event === 'Started') {
          contentLength = event.data.contentLength ?? 0;
          updateMessage = 'Downloading update...';
        } else if (event.event === 'Progress') {
          downloaded += event.data.chunkLength;
          if (contentLength > 0) {
            updateMessage = `Downloading update... ${Math.round((downloaded / contentLength) * 100)}%`;
          }
        } else if (event.event === 'Finished') {
          updateMessage = 'Installing update...';
        }
      });
      // On Windows the installer exits the app itself; relaunch covers other platforms.
      await relaunch();
    } catch (e) {
      console.error('Update failed', e);
      updateMessage = 'Update failed. Try again or download the latest release from GitHub.';
    } finally {
      updating = false;
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
        <button class="icon-btn close-btn" on:click={close} aria-label="Close settings">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>

      <div class="modal-body">
        <div class="settings-section">
          <h3 class="section-title">Appearance</h3>

          <div class="setting-group">
            <span class="setting-label" id="theme-mode-label">Theme</span>
            <div class="mode-toggle" role="group" aria-labelledby="theme-mode-label">
              <button
                class="btn mode-btn"
                class:selected={$theme === 'dark'}
                aria-pressed={$theme === 'dark'}
                on:click={() => theme.set('dark')}
              >Dark</button>
              <button
                class="btn mode-btn"
                class:selected={$theme === 'light'}
                aria-pressed={$theme === 'light'}
                on:click={() => theme.set('light')}
              >Light</button>
            </div>
          </div>

          <div class="setting-group">
            <span class="setting-label">Theme color</span>
            <div class="accent-picker">
              <ThemeWheel
                hue={($accent ?? DEFAULT_ACCENT).hue}
                saturation={($accent ?? DEFAULT_ACCENT).saturation}
                onchange={(hue, saturation) => accent.set({ hue, saturation })}
              />
              <div class="accent-side">
                <div class="swatch-grid">
                  {#each ACCENT_PRESETS as preset}
                    <button
                      class="swatch"
                      class:selected={$accent?.hue === preset.hue && $accent?.saturation === preset.saturation}
                      style={`background: hsl(${preset.hue} ${preset.saturation}% 50%);`}
                      title={preset.name}
                      aria-label={`Use ${preset.name} theme color`}
                      on:click={() => accent.set({ hue: preset.hue, saturation: preset.saturation })}
                    ></button>
                  {/each}
                </div>
                <button class="btn reset-accent" on:click={() => accent.reset()} disabled={$accent === null}>
                  Reset to default
                </button>
              </div>
            </div>
            <div class="setting-description">
              Drag the dot to recolor the whole app — the angle picks the color, the distance from the center picks how strong it is.
            </div>
          </div>
        </div>

        <div class="settings-section">
          <h3 class="section-title">Audio</h3>
          
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
          <h3 class="section-title">Microphone processing</h3>

          <div class="setting-group">
            <label class="toggle-row">
              <input type="checkbox" bind:checked={$noiseSuppression} />
              <span class="toggle-text">
                <span class="toggle-label">Noise suppression</span>
                <span class="toggle-description">Filter out constant background noise like fans or keyboards</span>
              </span>
            </label>
            <label class="toggle-row">
              <input type="checkbox" bind:checked={$echoCancellation} />
              <span class="toggle-text">
                <span class="toggle-label">Echo cancellation</span>
                <span class="toggle-description">Prevent others from hearing their own audio through your microphone</span>
              </span>
            </label>
            <label class="toggle-row">
              <input type="checkbox" bind:checked={$autoGainControl} />
              <span class="toggle-text">
                <span class="toggle-label">Automatic gain control</span>
                <span class="toggle-description">Keep your voice at a steady volume level</span>
              </span>
            </label>
            <div class="setting-description">
              Changes apply immediately, including while you are in a voice channel.
            </div>
          </div>
        </div>

        <div class="settings-section">
          <h3 class="section-title">Voice activation</h3>
          
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
                  class="btn ptt-key-button"
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
          <h3 class="section-title">Identity</h3>
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
              <button class="icon-btn copy-btn" on:click={copyPublicKey} title="Copy public key">
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
          <h3 class="section-title">Updates</h3>
          <div class="setting-group">
            <button class="btn update-btn" on:click={checkUpdates} disabled={updating}>Check for Updates</button>
            {#if updateMessage}
              <div class="update-message" class:success={updateMessage.startsWith('You are running')} class:warning={updateMessage.startsWith('Update available')}>
                {updateMessage}
              </div>
            {/if}
            <div class="setting-description">Current version: {APP_VERSION}</div>
          </div>
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
    /* A plain dim instead of a full-screen backdrop blur — blur here is
       very expensive in WebKitGTK (Linux) while the modal is open. */
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
    width: min(540px, 92vw);
    max-height: 82vh;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    animation: slideIn 0.18s var(--motion-easing-standard);
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
    padding: var(--space-5);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
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
    justify-content: space-between;
    align-items: center;
    font-weight: 500;
    color: var(--color-on-surface);
    font-size: var(--text-md);
  }

  .setting-value {
    font-size: var(--text-sm);
    color: var(--color-muted);
    font-family: var(--font-mono);
  }

  .setting-description {
    font-size: var(--text-sm);
    color: var(--color-muted);
    line-height: 1.5;
  }

  .mode-toggle {
    display: flex;
    gap: var(--space-2);
  }

  .mode-btn.selected {
    background: var(--color-primary-container);
    border-color: var(--color-primary);
    color: var(--color-primary);
  }

  .accent-picker {
    display: flex;
    align-items: center;
    gap: var(--space-5);
  }

  .accent-side {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  .swatch-grid {
    display: grid;
    grid-template-columns: repeat(3, auto);
    gap: var(--space-2);
    justify-content: start;
  }

  .swatch {
    width: 2rem;
    height: 2rem;
    padding: 0;
    border-radius: var(--radius-pill);
    border: 1px solid var(--color-surface-outline);
  }

  .swatch.selected {
    box-shadow:
      0 0 0 2px var(--color-surface-elevated),
      0 0 0 4px var(--color-primary);
  }

  .reset-accent {
    align-self: flex-start;
  }

  .slider-container {
    position: relative;
    height: var(--control-height);
    display: flex;
    align-items: center;
  }

  .slider-container::before {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    height: 4px;
    top: 50%;
    transform: translateY(-50%);
    background: var(--color-surface-raised);
    border-radius: var(--radius-pill);
    pointer-events: none;
  }

  .slider-track-fill {
    position: absolute;
    left: 0;
    height: 4px;
    top: 50%;
    transform: translateY(-50%);
    background: var(--color-primary);
    border-radius: var(--radius-pill);
    pointer-events: none;
    z-index: 1;
  }

  .volume-slider {
    width: 100%;
    height: 100%;
    -webkit-appearance: none;
    appearance: none;
    background: transparent;
    border: none;
    position: relative;
    z-index: 2;
    margin: 0;
    padding: 0;
    min-height: 0;
    cursor: pointer;
  }

  .volume-slider:focus {
    box-shadow: none;
  }

  .volume-slider::-webkit-slider-thumb {
    -webkit-appearance: none;
    appearance: none;
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--color-on-surface);
    border: 2px solid var(--color-primary);
    cursor: pointer;
  }

  .volume-slider::-moz-range-thumb {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--color-on-surface);
    border: 2px solid var(--color-primary);
    cursor: pointer;
  }

  .volume-slider::-moz-range-track {
    background: transparent;
    border: none;
  }

  .toggle-row {
    display: flex;
    align-items: flex-start;
    gap: var(--space-3);
    cursor: pointer;
  }

  .toggle-row input[type='checkbox'] {
    margin-top: 0.2rem;
    accent-color: var(--color-primary);
    width: 1rem;
    height: 1rem;
    flex-shrink: 0;
    cursor: pointer;
  }

  .toggle-text {
    display: grid;
    gap: 0.125rem;
  }

  .toggle-label {
    font-weight: 500;
    font-size: var(--text-md);
    color: var(--color-on-surface);
  }

  .toggle-description {
    font-size: var(--text-sm);
    color: var(--color-muted);
    line-height: 1.4;
  }

  .pubkey-row {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }

  .pubkey-input {
    flex: 1;
    min-width: 0;
    color: var(--color-muted);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    cursor: text;
    user-select: all;
  }

  .copy-btn {
    border: 1px solid var(--color-surface-outline);
  }

  .select-container {
    position: relative;
  }

  .device-select {
    width: 100%;
    appearance: none;
    padding-right: var(--space-6);
    border-radius: var(--radius-md);
  }

  .select-arrow {
    position: absolute;
    right: var(--space-3);
    top: 50%;
    transform: translateY(-50%);
    pointer-events: none;
    color: var(--color-muted);
    display: inline-flex;
  }

  .ptt-key-button.capturing {
    border-color: var(--color-warning);
    color: var(--color-warning);
  }

  .update-btn {
    justify-self: start;
  }

  .update-message {
    font-size: var(--text-sm);
    color: var(--color-muted);
  }

  .update-message.success {
    color: var(--color-success);
  }

  .update-message.warning {
    color: var(--color-warning);
  }

  .modal-footer {
    padding: var(--space-4) var(--space-5);
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
      transform: translateY(8px);
      opacity: 0;
    }
    to {
      transform: translateY(0);
      opacity: 1;
    }
  }
</style>
