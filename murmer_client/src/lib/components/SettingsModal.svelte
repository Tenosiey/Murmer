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
  import { serverInfo } from '$lib/stores/serverInfo';
  import { theme, accent, DEFAULT_ACCENT, accentToHex, hexToAccent, type Accent } from '$lib/stores/theme';
  import ThemeWheel from '$lib/components/ThemeWheel.svelte';
  import MurmerLogo from '$lib/components/MurmerLogo.svelte';
  import { loadKeyPair } from '$lib/keypair';
  import { onMount, onDestroy } from 'svelte';
  import { PushToTalkManager } from '$lib/voice/ptt';
  import {
    hotkeys,
    HOTKEY_ACTIONS,
    globalHotkeysEnabled,
    eventToCombo,
    formatCombo,
    type HotkeyActionId
  } from '$lib/stores/hotkeys';
  import { suspendGlobalHotkeys, resumeGlobalHotkeys } from '$lib/stores/globalHotkeys';
  import { check } from '@tauri-apps/plugin-updater';
  import { relaunch } from '@tauri-apps/plugin-process';
  import { dialogs } from '$lib/stores/dialogs';
  import { stats, statsConfig, statsSnapshot } from '$lib/stores/stats';
  import { session } from '$lib/stores/session';
  import { avatars } from '$lib/stores/avatars';
  import { connection } from '$lib/stores/connection';
  import { selectedServer } from '$lib/stores/servers';
  import { httpBaseFromWs } from '$lib/server-url';
  import { MAX_AVATAR_BYTES } from '$lib/chat/constants';
  import UserAvatar from '$lib/components/UserAvatar.svelte';
  import UserStatsPanel from '$lib/components/UserStatsPanel.svelte';
  interface Props {
    open: boolean;
    close: () => void;
  }

  let { open, close }: Props = $props();

  let updateMessage = $state('');
  let updating = $state(false);
  let publicKey = $state('');
  let keyCopied = $state(false);

  let inputs: MediaDeviceInfo[] = $state([]);
  let outputs: MediaDeviceInfo[] = $state([]);
  let capturingPttKey = $state(false);

  // Each settings topic lives on its own tab shown in the left rail.
  const TABS = [
    { id: 'appearance', label: 'Appearance' },
    { id: 'audio', label: 'Audio' },
    { id: 'microphone', label: 'Microphone' },
    { id: 'voice', label: 'Voice' },
    { id: 'hotkeys', label: 'Hotkeys' },
    { id: 'identity', label: 'Identity' },
    { id: 'stats', label: 'Stats & Privacy' },
    { id: 'about', label: 'About' },
    { id: 'server', label: 'Server', ownerOnly: true }
  ] as const;
  let activeTab: (typeof TABS)[number]['id'] = $state('appearance');

  const REPO_URL = 'https://github.com/Tenosiey/Murmer';
  const ABOUT_LINKS = [
    { label: 'GitHub repository', url: REPO_URL },
    { label: 'Report an issue', url: `${REPO_URL}/issues` },
    { label: 'Releases & changelog', url: `${REPO_URL}/releases` },
    { label: 'License', url: `${REPO_URL}/blob/main/LICENSE` }
  ];

  // Preset theme colors shown next to the wheel; each is a wheel position.
  // "Lime" is the brand color and the default — both logo variants (#c8ff3e
  // and #84b800) sit on this hue and differ only in lightness, which the
  // wheel does not carry.
  const ACCENT_PRESETS = [
    { name: 'Lime', hue: DEFAULT_ACCENT.hue, saturation: DEFAULT_ACCENT.saturation },
    { name: 'Sky', hue: 215, saturation: 78 },
    { name: 'Indigo', hue: 250, saturation: 68 },
    { name: 'Violet', hue: 285, saturation: 68 },
    { name: 'Rose', hue: 340, saturation: 72 },
    { name: 'Ember', hue: 22, saturation: 80 },
    { name: 'Moss', hue: 150, saturation: 55 }
  ];

  // The hex field mirrors the wheel. Edits only take effect on Enter or on
  // blur, so a half-typed code never repaints the whole app.
  let hexInput = $state(accentToHex(DEFAULT_ACCENT));
  let hexInvalid = $state(false);

  function syncHexField(value: Accent | null) {
    hexInput = accentToHex(value ?? DEFAULT_ACCENT);
    hexInvalid = false;
  }

  function commitHex() {
    const parsed = hexToAccent(hexInput);
    if (!parsed) {
      hexInvalid = true;
      return;
    }
    const current = $accent ?? DEFAULT_ACCENT;
    if (parsed.hue === current.hue && parsed.saturation === current.saturation) {
      // Unchanged: leaving the field must not pin the default palette to a
      // wheel position, which would enable "Reset to default" out of nowhere.
      hexInvalid = false;
      return;
    }
    accent.set(parsed);
    // Re-render from the stored position: the wheel drops the hex's
    // lightness, so the field must show the color that was actually applied.
    syncHexField(parsed);
  }

  /**
   * The overlay closes the modal on Enter/Space, so keys typed here must not
   * bubble that far — committing a color would otherwise dismiss settings.
   * Escape still passes through to close the modal.
   */
  function handleHexKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') return;
    event.stopPropagation();
    if (event.key === 'Enter') {
      event.preventDefault();
      commitHex();
    }
  }

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

  // ── Avatar ─────────────────────────────────────────────────────────────────
  let avatarFileInput: HTMLInputElement | null = $state(null);
  let avatarUploading = $state(false);
  let avatarError = $state('');
  let hasAvatar = $derived(Boolean($session.user && $avatars[$session.user]));

  async function uploadAvatar(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0] ?? null;
    input.value = '';
    if (!file || !$selectedServer) return;
    avatarError = '';
    if (file.size > MAX_AVATAR_BYTES) {
      avatarError = 'Avatars must be 1 MB or smaller.';
      return;
    }
    avatarUploading = true;
    const form = new FormData();
    form.append('file', file);
    try {
      const res = await fetch(httpBaseFromWs($selectedServer) + '/upload', {
        method: 'POST',
        body: form
      });
      if (res.status === 415) {
        avatarError = 'This image type is not allowed on the server.';
        return;
      }
      if (res.status === 413) {
        avatarError = 'That image is too large to upload.';
        return;
      }
      if (!res.ok) throw new Error(`upload failed with status ${res.status}`);
      const data = await res.json();
      if (typeof data.url !== 'string') throw new Error('upload response missing url');
      // Validated and registered server-side; the confirmation arrives as a
      // broadcast avatar-update frame which updates the store.
      avatars.setSelf(data.url);
    } catch (e) {
      console.error('avatar upload failed', e);
      avatarError = 'Avatar upload failed. Please try again.';
    } finally {
      avatarUploading = false;
    }
  }

  function removeAvatar() {
    avatarError = '';
    avatars.setSelf(null);
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
    // Keyboard activation of the focused backdrop only; keystrokes inside
    // the modal content (inputs, buttons) bubble here and must not close it.
    if (event.target !== event.currentTarget) return;
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

  // ── Hotkey capture ─────────────────────────────────────────────────────
  let capturingHotkeyId: HotkeyActionId | null = $state(null);
  let hotkeyCaptureCleanup: (() => void) | null = null;

  function stopHotkeyCapture() {
    hotkeyCaptureCleanup?.();
    hotkeyCaptureCleanup = null;
    if (capturingHotkeyId !== null) {
      capturingHotkeyId = null;
      resumeGlobalHotkeys();
    }
  }

  /**
   * Grab the next full key combination and bind it to the action. Runs in
   * the capture phase so the press can't leak into the app (e.g. Escape
   * closing the modal); modifier-only presses keep the capture open,
   * Escape cancels it.
   */
  function captureHotkey(id: HotkeyActionId) {
    if (capturingHotkeyId === id) {
      stopHotkeyCapture();
      return;
    }
    stopHotkeyCapture();
    capturingHotkeyId = id;
    // Release OS-level shortcuts while capturing: a registered combo would
    // be consumed by the OS and could never be captured (or re-assigned).
    suspendGlobalHotkeys();
    const handler = (event: KeyboardEvent) => {
      event.preventDefault();
      event.stopPropagation();
      if (event.key === 'Escape') {
        stopHotkeyCapture();
        return;
      }
      const combo = eventToCombo(event);
      if (!combo) return;
      hotkeys.bind(id, combo);
      stopHotkeyCapture();
    };
    document.addEventListener('keydown', handler, true);
    hotkeyCaptureCleanup = () => document.removeEventListener('keydown', handler, true);
  }

  onDestroy(stopHotkeyCapture);


  function toggleStatsOptIn(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    stats.setOptIn(input.checked);
  }

  async function deleteMyStats() {
    const confirmed = await dialogs.confirm({
      title: 'Delete your stats?',
      message:
        'This permanently removes all recorded lifetime stats and achievements for your user on this server.',
      confirmLabel: 'Delete stats',
      danger: true
    });
    if (confirmed) stats.resetStats();
  }
  let visibleTabs = $derived(TABS.filter((tab) => !('ownerOnly' in tab && tab.ownerOnly) || $serverInfo));
  // If the active tab disappears (e.g. server info clears), fall back to the first.
  $effect(() => {
    if (!visibleTabs.some((tab) => tab.id === activeTab)) {
      activeTab = visibleTabs[0].id;
    }
  });
  $effect(() => {
    syncHexField($accent);
  });
  // Never leave a dangling capture when the modal closes or unmounts.
  $effect(() => {
    if (!open && capturingHotkeyId !== null) stopHotkeyCapture();
  });
  // ── Lifetime stats (double opt-in) ─────────────────────────────────────
  let statsTracking = $derived(($statsConfig?.serverEnabled ?? false) && ($statsConfig?.optedIn ?? false));
  // Refresh the own snapshot whenever the tab is opened while tracking is on.
  $effect(() => {
    if (open && activeTab === 'stats' && statsTracking) {
      stats.fetchStats();
    }
  });
  let ownSnapshot =
    $derived($statsSnapshot && $statsSnapshot.user === $session.user ? $statsSnapshot : null);
</script>

{#if open}
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div class="modal-overlay" onclick={close} onkeydown={handleOverlayKeydown} role="dialog" aria-modal="true" aria-labelledby="settings-title" tabindex="-1">
    <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex -->
    <div class="modal-content" onclick={(event) => event.stopPropagation()} onkeydown={handleKeydown} role="document" tabindex="0">
      <div class="modal-header">
        <h2 id="settings-title">Settings</h2>
        <button class="icon-btn close-btn" onclick={close} aria-label="Close settings">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <line x1="18" y1="6" x2="6" y2="18"></line>
            <line x1="6" y1="6" x2="18" y2="18"></line>
          </svg>
        </button>
      </div>

      <div class="settings-layout">
        <nav class="settings-tabs" aria-label="Settings sections">
          {#each visibleTabs as tab}
            <button
              class="tab-btn"
              class:selected={activeTab === tab.id}
              aria-pressed={activeTab === tab.id}
              onclick={() => (activeTab = tab.id)}
            >{tab.label}</button>
          {/each}
        </nav>

        <div class="modal-body">
        {#if activeTab === 'appearance'}
        <div class="settings-section">
          <h3 class="section-title">Appearance</h3>

          <div class="setting-group">
            <span class="setting-label" id="theme-mode-label">Theme</span>
            <div class="mode-toggle" role="group" aria-labelledby="theme-mode-label">
              <button
                class="btn mode-btn"
                class:selected={$theme === 'dark'}
                aria-pressed={$theme === 'dark'}
                onclick={() => theme.set('dark')}
              >Dark</button>
              <button
                class="btn mode-btn"
                class:selected={$theme === 'light'}
                aria-pressed={$theme === 'light'}
                onclick={() => theme.set('light')}
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
                      onclick={() => accent.set({ hue: preset.hue, saturation: preset.saturation })}
                    ></button>
                  {/each}
                </div>
                <label class="field hex-field">
                  <span>Hex code</span>
                  <input
                    class="hex-input"
                    class:invalid={hexInvalid}
                    type="text"
                    maxlength="7"
                    spellcheck="false"
                    autocomplete="off"
                    placeholder="#27c0e8"
                    aria-invalid={hexInvalid}
                    bind:value={hexInput}
                    oninput={() => (hexInvalid = false)}
                    onblur={commitHex}
                    onkeydown={handleHexKeydown}
                  />
                </label>
                <button class="btn reset-accent" onclick={() => accent.reset()} disabled={$accent === null}>
                  Reset to default
                </button>
              </div>
            </div>
            <div class="setting-description">
              Drag the dot to recolor the whole app — the angle picks the color, the distance from the center picks how strong it is.
              You can also type a hex code; its brightness is set by the theme, so only the color and its strength are taken from it.
            </div>
          </div>
        </div>
        {/if}

        {#if activeTab === 'audio'}
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
        {/if}

        {#if activeTab === 'microphone'}
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
        {/if}

        {#if activeTab === 'voice'}
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
                  onclick={capturePttKey}
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
        {/if}

        {#if activeTab === 'hotkeys'}
        <div class="settings-section">
          <h3 class="section-title">Hotkeys</h3>

          <div class="setting-group">
            {#each HOTKEY_ACTIONS as action (action.id)}
              <div class="hotkey-row">
                <span class="toggle-text">
                  <span class="toggle-label">
                    {action.label}
                    {#if action.global}
                      <span class="global-badge" title="Also works while another app is focused">system-wide</span>
                    {/if}
                  </span>
                  <span class="toggle-description">{action.description}</span>
                </span>
                <div class="hotkey-controls">
                  <button
                    class="btn hotkey-btn"
                    class:capturing={capturingHotkeyId === action.id}
                    class:unset={!$hotkeys[action.id] && capturingHotkeyId !== action.id}
                    onclick={() => captureHotkey(action.id)}
                    title="Click, then press the new key combination"
                  >
                    {#if capturingHotkeyId === action.id}
                      Press keys…
                    {:else}
                      {formatCombo($hotkeys[action.id])}
                    {/if}
                  </button>
                  <button
                    class="icon-btn hotkey-clear"
                    onclick={() => hotkeys.unbind(action.id)}
                    disabled={!$hotkeys[action.id]}
                    title="Remove hotkey"
                    aria-label={`Remove hotkey for ${action.label}`}
                  >
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
                      <line x1="18" y1="6" x2="6" y2="18"></line>
                      <line x1="6" y1="6" x2="18" y2="18"></line>
                    </svg>
                  </button>
                </div>
              </div>
            {/each}

            <div class="setting-description">
              Click a hotkey and press the new key combination — Esc cancels, and assigning a
              combination that is already in use moves it to the new action. Hotkeys without
              Ctrl, Alt or a function key stay inactive while you are typing a message. The
              push-to-talk key is configured in the Voice tab.
            </div>

            <label class="toggle-row">
              <input type="checkbox" bind:checked={$globalHotkeysEnabled} />
              <span class="toggle-text">
                <span class="toggle-label">System-wide voice hotkeys</span>
                <span class="toggle-description">
                  Keep the hotkeys marked "system-wide" working while another application is
                  focused, e.g. to mute your microphone during a game. Note that Murmer then
                  reserves those key combinations for itself while it is running.
                </span>
              </span>
            </label>

            <button class="btn reset-hotkeys" onclick={() => hotkeys.resetAll()}>
              Reset to defaults
            </button>
          </div>
        </div>
        {/if}

        {#if activeTab === 'identity'}
        <div class="settings-section">
          <h3 class="section-title">Identity</h3>
          <div class="setting-group">
            <span class="setting-label">Avatar</span>
            {#if $connection === 'connected' && $session.user}
              <div class="avatar-row">
                <UserAvatar name={$session.user} />
                <button
                  class="btn"
                  onclick={() => avatarFileInput?.click()}
                  disabled={avatarUploading}
                >
                  {avatarUploading ? 'Uploading…' : hasAvatar ? 'Change image' : 'Upload image'}
                </button>
                {#if hasAvatar}
                  <button class="btn btn-danger" onclick={removeAvatar} disabled={avatarUploading}>
                    Remove
                  </button>
                {/if}
                <input
                  bind:this={avatarFileInput}
                  type="file"
                  accept="image/png,image/jpeg,image/gif,image/webp"
                  hidden
                  onchange={uploadAvatar}
                />
              </div>
              {#if avatarError}
                <div class="setting-description avatar-error" role="alert">{avatarError}</div>
              {/if}
              <div class="setting-description">
                Shown next to your messages and in the member list. Stored on this server;
                PNG, JPEG, GIF or WebP up to 1&nbsp;MB.
              </div>
            {:else}
              <div class="setting-description">Connect to a server to set your avatar.</div>
            {/if}
          </div>
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
              <button class="icon-btn copy-btn" onclick={copyPublicKey} title="Copy public key">
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
        {/if}

        {#if activeTab === 'stats'}
        <div class="settings-section">
          <h3 class="section-title">Stats &amp; Privacy</h3>

          <div class="setting-group">
            <label class="toggle-row">
              <input
                type="checkbox"
                checked={$statsConfig?.optedIn ?? false}
                disabled={$statsConfig === null || !$statsConfig.serverEnabled}
                onchange={toggleStatsOptIn}
              />
              <span class="toggle-text">
                <span class="toggle-label">Track my lifetime stats</span>
                <span class="toggle-description">
                  Record activity counters (messages, voice time, reactions, …) and unlock
                  achievements. Nothing is recorded unless both this server and you enable it —
                  only totals are stored, never message contents. Other members can see your
                  stats while you are opted in.
                </span>
              </span>
            </label>
            {#if $statsConfig === null}
              <div class="setting-description">Waiting for the server…</div>
            {:else if !$statsConfig.serverEnabled}
              <div class="setting-description">
                Stat tracking is switched off server-wide. A server Owner or Admin can enable it
                in the server dashboard.
              </div>
            {/if}
            <div>
              <button class="btn btn-danger" onclick={deleteMyStats}>Delete my stats…</button>
            </div>
          </div>

          {#if statsTracking}
            {#if ownSnapshot}
              <UserStatsPanel snapshot={ownSnapshot} />
            {:else}
              <div class="setting-description">Loading your stats…</div>
            {/if}
          {/if}
        </div>
        {/if}

        {#if activeTab === 'about'}
        <div class="settings-section">
          <h3 class="section-title">About</h3>

          <div class="setting-group">
            <div class="about-header">
              <MurmerLogo size={32} />
              <span class="about-name">Murmer</span>
              <span class="setting-value">v{APP_VERSION}</span>
            </div>
            <div class="setting-description">
              A self-hosted voice and text chat prototype. Murmer is open source —
              the client and server live in the same repository on GitHub.
            </div>
          </div>

          <div class="setting-group">
            <span class="setting-label">Updates</span>
            <button class="btn update-btn" onclick={checkUpdates} disabled={updating}>Check for Updates</button>
            {#if updateMessage}
              <div class="update-message" class:success={updateMessage.startsWith('You are running')} class:warning={updateMessage.startsWith('Update available')}>
                {updateMessage}
              </div>
            {/if}
          </div>

          <div class="setting-group">
            <span class="setting-label">Links</span>
            <div class="about-links">
              {#each ABOUT_LINKS as link}
                <a class="btn about-link" href={link.url} target="_blank" rel="noopener noreferrer">
                  {link.label}
                  <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8">
                    <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6"></path>
                    <polyline points="15,3 21,3 21,9"></polyline>
                    <line x1="10" y1="14" x2="21" y2="3"></line>
                  </svg>
                </a>
              {/each}
            </div>
          </div>

          <div class="setting-group">
            <div class="setting-description">
              Built with Tauri, SvelteKit and Rust (Axum). Released under the MIT License.
            </div>
          </div>
        </div>
        {/if}

        {#if activeTab === 'server' && $serverInfo}
          <div class="settings-section">
            <h3 class="section-title">Server</h3>
            <div class="setting-group">
              <div class="setting-label">
                Server version
                <span class="setting-value">{$serverInfo.version}</span>
              </div>
              <div class="setting-description">
                Only visible to users with the Owner or Admin role.
              </div>
            </div>
          </div>
        {/if}
        </div>
      </div>

      <div class="modal-footer">
        <button class="btn btn-primary" onclick={close}>Done</button>
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

  .hex-field {
    gap: var(--space-1);
  }

  .hex-input {
    width: 7rem;
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .hex-input.invalid {
    border-color: var(--color-warning);
    box-shadow: 0 0 0 1px var(--color-warning);
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

  .avatar-row {
    display: flex;
    gap: var(--space-2);
    align-items: center;
  }

  .avatar-error {
    color: var(--color-error);
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

  .hotkey-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
  }

  .hotkey-controls {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-shrink: 0;
  }

  .hotkey-btn {
    min-width: 9.5rem;
    justify-content: center;
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }

  .hotkey-btn.capturing {
    border-color: var(--color-warning);
    color: var(--color-warning);
  }

  .hotkey-btn.unset {
    color: var(--color-muted);
    font-style: italic;
    font-family: inherit;
  }

  .hotkey-clear {
    border: 1px solid var(--color-surface-outline);
  }

  .hotkey-clear:disabled {
    opacity: 0.4;
    cursor: default;
  }

  .global-badge {
    margin-left: var(--space-2);
    padding: 0.0625rem var(--space-2);
    border-radius: var(--radius-pill);
    border: 1px solid var(--color-surface-outline);
    background: var(--color-surface-raised);
    color: var(--color-muted);
    font-size: var(--text-xs);
    font-weight: 500;
    vertical-align: middle;
  }

  .reset-hotkeys {
    justify-self: start;
  }

  .update-btn {
    justify-self: start;
  }

  .about-header {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .about-name {
    font-size: var(--text-lg);
    font-weight: 600;
    color: var(--color-on-surface);
  }

  .about-links {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .about-link {
    text-decoration: none;
    gap: var(--space-2);
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
