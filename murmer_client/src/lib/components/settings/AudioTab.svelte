<script lang="ts">
  import {
    volume,
    appSoundVolume,
    outputDeviceId,
    screenShareVolume,
    screenShareMuted
  } from '$lib/stores/settings';
  import { soundboardEnabled, soundboardVolume } from '$lib/stores/soundboardSettings';
  import { audioOutputs, refreshAudioDevices } from '$lib/stores/audioDevices';
  import { onMount } from 'svelte';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  // Re-read each time settings open: a headset plugged in since the last
  // look should be offered without reloading the app.
  onMount(() => void refreshAudioDevices());
</script>

{#if active}
  <div class="settings-section">
    <h3 class="section-title">Playback</h3>

    <div class="setting-group">
      <label for="volume-slider" class="setting-label">
        Voice volume
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
      <div class="setting-description">
        How loud other members are. Individual people can be turned up or down in the
        voice channel's context menu.
      </div>
    </div>

    <div class="setting-group">
      <label for="app-sound-volume" class="setting-label">
        App sound volume
        <span class="setting-value">{Math.round($appSoundVolume * 100)}%</span>
      </label>
      <div class="slider-container">
        <input
          id="app-sound-volume"
          class="volume-slider"
          type="range"
          min="0"
          max="1"
          step="0.01"
          bind:value={$appSoundVolume}
        />
        <div class="slider-track-fill" style="width: {$appSoundVolume * 100}%"></div>
      </div>
      <div class="setting-description">
        The blips Murmer itself plays when somebody joins or leaves your voice channel
        and when you mute or unmute. Set it to 0% to silence them.
      </div>
    </div>

    <div class="setting-group">
      <label for="screenshare-volume" class="setting-label">
        Screen share volume
        <span class="setting-value">{Math.round($screenShareVolume * 100)}%</span>
      </label>
      <div class="slider-container">
        <input
          id="screenshare-volume"
          class="volume-slider"
          type="range"
          min="0"
          max="1"
          step="0.01"
          bind:value={$screenShareVolume}
          disabled={$screenShareMuted}
        />
        <div class="slider-track-fill" style="width: {$screenShareVolume * 100}%"></div>
      </div>
      <label class="toggle-row">
        <input type="checkbox" bind:checked={$screenShareMuted} />
        <span class="toggle-text">
          <span class="toggle-label">Mute screen share audio</span>
          <span class="toggle-description">
            Sharers can include their system audio. This silences it without touching the
            voices. It is the starting point for every share you open; each one you are
            watching also carries its own volume and mute in its title bar.
          </span>
        </span>
      </label>
    </div>

    <div class="setting-group">
      <label for="output-select" class="setting-label">Output device (speakers)</label>
      <div class="select-container">
        <select id="output-select" class="device-select" bind:value={$outputDeviceId}>
          <option value="">Default</option>
          {#each $audioOutputs as dev}
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
    <h3 class="section-title">Soundboard</h3>

    <div class="setting-group">
      <label for="soundboard-volume" class="setting-label">
        Soundboard volume
        <span class="setting-value">{Math.round($soundboardVolume * 100)}%</span>
      </label>
      <div class="slider-container">
        <input
          id="soundboard-volume"
          class="volume-slider"
          type="range"
          min="0"
          max="1"
          step="0.01"
          bind:value={$soundboardVolume}
          disabled={!$soundboardEnabled}
        />
        <div class="slider-track-fill" style="width: {$soundboardVolume * 100}%"></div>
      </div>
      <label class="toggle-row">
        <input type="checkbox" bind:checked={$soundboardEnabled} />
        <span class="toggle-text">
          <span class="toggle-label">Hear soundboard sounds</span>
          <span class="toggle-description">
            Play sounds others trigger in your voice channel. Individual sounds and people
            can be muted from the soundboard panel.
          </span>
        </span>
      </label>
    </div>
  </div>
{/if}
