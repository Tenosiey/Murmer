<script lang="ts">
  import {
    inputDeviceId,
    voiceMode,
    vadSensitivity,
    vadAutoSensitivity,
    vadReleaseDelay,
    VAD_RELEASE_MIN_MS,
    VAD_RELEASE_MAX_MS,
    pttKey,
    echoCancellation,
    noiseSuppressionMode,
    autoGainControl,
    micGain,
    MAX_MIC_GAIN
  } from '$lib/stores/settings';
  import { audioInputs } from '$lib/stores/audioDevices';
  import { PushToTalkManager } from '$lib/voice/ptt';
  import { VAD_THRESHOLD_MIN, VAD_THRESHOLD_MAX } from '$lib/voice/vad';
  import { suspendGlobalHotkeys, resumeGlobalHotkeys } from '$lib/stores/globalHotkeys';
  import MicLevelMeter from '$lib/components/MicLevelMeter.svelte';
  import MicTestPanel from '$lib/components/MicTestPanel.svelte';
  import { t } from '$lib/i18n';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  let capturingPttKey = $state(false);

  // Ends of the VAD sensitivity scale, shared by the slider and the level meter
  // drawn underneath it so the threshold marker lines up with the slider thumb.
  // They come from the detector, which clamps automatic thresholds to the same
  // range — a marker off the end of the meter would be a lie.
  const VAD_MIN = VAD_THRESHOLD_MIN;
  const VAD_MAX = VAD_THRESHOLD_MAX;

  // Where the release delay sits on its scale, for the filled part of the
  // slider track. Derived rather than inlined so the markup stays readable.
  const vadReleaseFill = $derived(
    ($vadReleaseDelay - VAD_RELEASE_MIN_MS) / (VAD_RELEASE_MAX_MS - VAD_RELEASE_MIN_MS)
  );

  async function capturePttKey() {
    capturingPttKey = true;
    // The current binding may itself be a registered global shortcut, which
    // the OS would consume before the capture handler ever saw it.
    suspendGlobalHotkeys();
    try {
      pttKey.set(await PushToTalkManager.captureKey());
    } catch (error) {
      console.error('Failed to capture PTT key:', error);
    } finally {
      capturingPttKey = false;
      resumeGlobalHotkeys();
    }
  }
</script>

{#if active}
  <div class="settings-section">
    <h3 class="section-title">{t('voice.microphone')}</h3>

    <div class="setting-group">
      <label for="input-select" class="setting-label">{t('voice.inputDevice')}</label>
      <div class="select-container">
        <select id="input-select" class="device-select" bind:value={$inputDeviceId}>
          <option value="">{t('audio.default')}</option>
          {#each $audioInputs as dev}
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
      <label for="mic-gain-slider" class="setting-label">
        {t('voice.inputVolume')}
        <span class="setting-value">{Math.round($micGain * 100)}%</span>
      </label>
      <div class="slider-container">
        <input
          id="mic-gain-slider"
          class="volume-slider"
          type="range"
          min="0"
          max={MAX_MIC_GAIN}
          step="0.05"
          bind:value={$micGain}
        />
        <div class="slider-track-fill" style="width: {($micGain / MAX_MIC_GAIN) * 100}%"></div>
      </div>
      <button
        class="btn reset-gain"
        onclick={() => micGain.set(1)}
        disabled={$micGain === 1}
      >{t('voice.resetGain')}</button>
      <div class="setting-description">
        {t('voice.inputVolumeHint')}
      </div>
    </div>

    <div class="setting-group">
      <label for="noise-suppression-select" class="setting-label">{t('voice.noiseSuppression')}</label>
      <div class="select-container">
        <select
          id="noise-suppression-select"
          class="device-select"
          bind:value={$noiseSuppressionMode}
        >
          <option value="rnnoise">{t('voice.rnnoise')}</option>
          <option value="browser">{t('voice.builtIn')}</option>
          <option value="off">{t('voice.off')}</option>
        </select>
        <div class="select-arrow">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="6,9 12,15 18,9"></polyline>
          </svg>
        </div>
      </div>
      <div class="setting-description">
        {t('voice.noiseHint')}
      </div>
    </div>

    <div class="setting-group">
      <label class="toggle-row">
        <input type="checkbox" bind:checked={$echoCancellation} />
        <span class="toggle-text">
          <span class="toggle-label">{t('voice.echo')}</span>
          <span class="toggle-description">{t('voice.echoHint')}</span>
        </span>
      </label>
      <label class="toggle-row">
        <input type="checkbox" bind:checked={$autoGainControl} />
        <span class="toggle-text">
          <span class="toggle-label">{t('voice.agc')}</span>
          <span class="toggle-description">{t('voice.agcHint')}</span>
        </span>
      </label>
      <div class="setting-description">
        {t('voice.appliesNow')}
      </div>
    </div>

    <div class="setting-group">
      <span class="setting-label">{t('voice.micTest')}</span>
      <MicTestPanel />
    </div>
  </div>

  <div class="settings-section">
    <h3 class="section-title">{t('voice.transmission')}</h3>

    <div class="setting-group">
      <label for="voice-mode-select" class="setting-label">{t('voice.mode')}</label>
      <div class="select-container">
        <select id="voice-mode-select" class="device-select" bind:value={$voiceMode}>
          <option value="continuous">{t('voice.modeContinuous')}</option>
          <option value="vad">{t('voice.modeVad')}</option>
          <option value="ptt">{t('voice.modePtt')}</option>
        </select>
        <div class="select-arrow">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="6,9 12,15 18,9"></polyline>
          </svg>
        </div>
      </div>
    </div>

    {#if $voiceMode === 'ptt'}
      <div class="setting-group">
        <label class="setting-label" for="ptt-key-button">{t('voice.pttKey')}</label>
        <button
          id="ptt-key-button"
          class="btn ptt-key-button"
          class:capturing={capturingPttKey}
          onclick={capturePttKey}
          disabled={capturingPttKey}
        >
          {#if capturingPttKey}
            {t('voice.pressAnyKey')}
          {:else}
            {PushToTalkManager.getKeyDisplayName($pttKey)}
          {/if}
        </button>
        <div class="setting-description">
          {t('voice.pttHint')}
          {#if PushToTalkManager.isGlobalCapable($pttKey)}
            {t('voice.pttGlobal')}
          {:else}
            {t('voice.pttLocal')}
          {/if}
        </div>
      </div>
    {/if}

    <!-- One meter for the whole tab: it doubles as the VAD threshold
         display and as a plain "is my microphone working" check in the
         other modes. Only ever one instance, so only one capture. -->
    <div class="setting-group">
      {#if $voiceMode === 'vad'}
        <label class="toggle-row">
          <input type="checkbox" bind:checked={$vadAutoSensitivity} />
          <span class="toggle-text">
            <span class="toggle-label">{t('voice.autoSensitivity')}</span>
            <span class="toggle-description">
              {t('voice.autoSensitivityHint')}
            </span>
          </span>
        </label>
        {#if !$vadAutoSensitivity}
          <label for="vad-sensitivity-slider" class="setting-label">
            {t('voice.vadSensitivity')}
            <span class="setting-value">{Math.round((1 - $vadSensitivity) * 100)}%</span>
          </label>
          <div class="slider-container">
            <input
              id="vad-sensitivity-slider"
              class="volume-slider"
              type="range"
              min={VAD_MIN}
              max={VAD_MAX}
              step="0.01"
              bind:value={$vadSensitivity}
            />
            <div class="slider-track-fill" style="width: {(1 - ($vadSensitivity / VAD_MAX)) * 100}%"></div>
          </div>
        {/if}
        <MicLevelMeter
          threshold={$vadAutoSensitivity ? undefined : $vadSensitivity}
          automatic={$vadAutoSensitivity}
          min={VAD_MIN}
          max={VAD_MAX}
        />
        <div class="setting-description">
          {#if $vadAutoSensitivity}
            {t('voice.autoHint')}
          {:else}
            {t('voice.manualHint')}
          {/if}
        </div>

        <label for="vad-release-slider" class="setting-label">
          {t('voice.releaseDelay')}
          <span class="setting-value">{t('voice.milliseconds', { ms: $vadReleaseDelay })}</span>
        </label>
        <div class="slider-container">
          <input
            id="vad-release-slider"
            class="volume-slider"
            type="range"
            min={VAD_RELEASE_MIN_MS}
            max={VAD_RELEASE_MAX_MS}
            step="50"
            bind:value={$vadReleaseDelay}
          />
          <div class="slider-track-fill" style="width: {vadReleaseFill * 100}%"></div>
        </div>
        <div class="setting-description">
          {t('voice.releaseHint')}
        </div>
      {:else}
        <span class="setting-label">{t('voice.inputLevel')}</span>
        <MicLevelMeter min={VAD_MIN} max={VAD_MAX} />
        <div class="setting-description">
          {t('voice.inputLevelHint')}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>

  .reset-gain {
    justify-self: start;
  }

  .ptt-key-button {
    justify-self: start;
  }

  .ptt-key-button.capturing {
    border-color: var(--color-warning);
    color: var(--color-warning);
  }
</style>
