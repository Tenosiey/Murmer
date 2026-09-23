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
    <h3 class="section-title">Microphone</h3>

    <div class="setting-group">
      <label for="input-select" class="setting-label">Input device (microphone)</label>
      <div class="select-container">
        <select id="input-select" class="device-select" bind:value={$inputDeviceId}>
          <option value="">Default</option>
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
        Input volume
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
      >Reset to 100%</button>
      <div class="setting-description">
        Amplifies your microphone before it is sent. Turn it up for a quiet headset —
        unlike automatic gain control this is a fixed factor, so it does not chase your
        voice, but above 100% it lifts background noise just as much and loud peaks may
        distort. The level meter further down shows the result.
      </div>
    </div>

    <div class="setting-group">
      <label for="noise-suppression-select" class="setting-label">Noise suppression</label>
      <div class="select-container">
        <select
          id="noise-suppression-select"
          class="device-select"
          bind:value={$noiseSuppressionMode}
        >
          <option value="rnnoise">RNNoise (recommended)</option>
          <option value="browser">Built-in</option>
          <option value="off">Off</option>
        </select>
        <div class="select-arrow">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
            <polyline points="6,9 12,15 18,9"></polyline>
          </svg>
        </div>
      </div>
      <div class="setting-description">
        RNNoise is a neural filter that runs on your own machine. It removes keyboards,
        fans and background voices that the built-in suppression leaves in, at the cost of
        about a percent of one CPU core. Use the microphone test below to compare them.
      </div>
    </div>

    <div class="setting-group">
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

    <div class="setting-group">
      <span class="setting-label">Microphone test</span>
      <MicTestPanel />
    </div>
  </div>

  <div class="settings-section">
    <h3 class="section-title">Transmission</h3>

    <div class="setting-group">
      <label for="voice-mode-select" class="setting-label">Voice mode</label>
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

    {#if $voiceMode === 'ptt'}
      <div class="setting-group">
        <label class="setting-label" for="ptt-key-button">Push-to-talk key</label>
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
          Click the button above and press the key you want to use for push-to-talk.
          {#if PushToTalkManager.isGlobalCapable($pttKey)}
            This combination also works while another application is focused.
          {:else}
            A plain key only works while Murmer is focused — it cannot be reserved
            system-wide without swallowing it in every other program. Add Ctrl or Alt,
            or pick a function key, to talk while gaming.
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
            <span class="toggle-label">Automatic sensitivity</span>
            <span class="toggle-description">
              Measure the background noise and set the threshold just above it
            </span>
          </span>
        </label>
        {#if !$vadAutoSensitivity}
          <label for="vad-sensitivity-slider" class="setting-label">
            VAD sensitivity
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
            The marker follows your room: stay quiet for a moment and it settles just
            above the noise, then rises again if a fan or a fridge kicks in. Turn this
            off if it clips the start of your words or lets noise through.
          {:else}
            Higher sensitivity detects quieter speech but may pick up background noise.
            Speak normally and drag the slider until the bar passes the marker only
            when you talk.
          {/if}
        </div>

        <label for="vad-release-slider" class="setting-label">
          Release delay
          <span class="setting-value">{$vadReleaseDelay} ms</span>
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
          How long you keep transmitting after you stop talking. Longer keeps the
          pauses between words intact; shorter cuts the room off sooner, at the risk
          of clipping the ends of your sentences. Takes effect immediately, including
          mid-call.
        </div>
      {:else}
        <span class="setting-label">Input level</span>
        <MicLevelMeter min={VAD_MIN} max={VAD_MAX} />
        <div class="setting-description">
          Speak to check that the selected microphone is picking you up.
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
