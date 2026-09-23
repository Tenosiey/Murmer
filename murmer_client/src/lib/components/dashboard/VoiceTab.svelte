<script lang="ts">
  import { onMount, onDestroy } from 'svelte';
  import { chat } from '$lib/stores/chat';
  import { describeServerError } from '$lib/errors';
  import {
    VOICE_QUALITY_PRESETS,
    MAX_VOICE_BITRATE
  } from '$lib/chat/constants';
  import { voiceDefaults, setVoiceDefaults } from '$lib/stores/voiceDefaults';
  import type { Message } from '$lib/types';

  interface Props {
    active: boolean;
  }

  let { active }: Props = $props();

  // Filled from the server's answer when the dashboard opens, then left
  // alone: incoming broadcasts must not clobber what the user is editing.
  let voiceQuality = $state($voiceDefaults.quality);
  let voiceBitrateKbps = $state(($voiceDefaults.bitrate ?? 0) / 1000);
  let voiceFeedback: { text: string; kind: 'error' | 'info' } | null = $state(null);
  let voiceSavePending = $state(false);

  let voiceBitrateDraft = $derived(
    Math.round((Number.isFinite(voiceBitrateKbps) ? voiceBitrateKbps : 0) * 1000)
  );
  let voiceDirty = $derived(
    voiceQuality !== $voiceDefaults.quality ||
      (voiceBitrateDraft > 0 ? voiceBitrateDraft : null) !== $voiceDefaults.bitrate
  );

  // A broadcast matching the submitted values confirms the save.
  $effect(() => {
    if (voiceSavePending && !voiceDirty) {
      voiceSavePending = false;
      voiceFeedback = { text: 'Changes saved.', kind: 'info' };
    }
  });

  /** Picking a preset fills in its bitrate; 0 kbps means "lossless". */
  function pickVoicePreset(event: Event) {
    const quality = (event.currentTarget as HTMLSelectElement).value;
    voiceQuality = quality;
    const preset = VOICE_QUALITY_PRESETS.find((entry) => entry.quality === quality);
    if (preset) voiceBitrateKbps = (preset.bitrate ?? 0) / 1000;
  }

  function saveVoiceDefaults() {
    if (!voiceQuality.trim()) {
      voiceFeedback = { text: 'Pick a quality preset.', kind: 'error' };
      return;
    }
    if (voiceBitrateDraft < 0 || voiceBitrateDraft > MAX_VOICE_BITRATE) {
      voiceFeedback = {
        text: `Enter a bitrate up to ${MAX_VOICE_BITRATE / 1000} kbps, or 0 for uncompressed.`,
        kind: 'error'
      };
      return;
    }
    voiceFeedback = null;
    voiceSavePending = true;
    // Role-checked server-side; the confirmation arrives as a broadcast
    // voice-defaults frame which updates the store.
    setVoiceDefaults(voiceQuality.trim(), voiceBitrateDraft > 0 ? voiceBitrateDraft : null);
  }

  const VOICE_DEFAULTS_ERROR_CODES = new Set([
    'voice-defaults-permission-denied',
    'voice-defaults-update-failed',
    'invalid-voice-quality',
    'invalid-voice-bitrate'
  ]);

  function handleServerError(msg: Message) {
    const code = msg.message;
    if (typeof code !== 'string') return;
    if (VOICE_DEFAULTS_ERROR_CODES.has(code)) {
      voiceSavePending = false;
      voiceFeedback = { text: describeServerError(code), kind: 'error' };
    }
  }

  onMount(() => chat.on('error', handleServerError));
  onDestroy(() => chat.off('error', handleServerError));
</script>

{#if active}
  <div class="settings-section">
    <h3 class="section-title">Voice</h3>
    <div class="setting-description">
      What a newly created voice channel starts with. These are defaults, not a cap:
      existing channels keep their own setting, and whoever creates a channel can pick
      a different preset.
    </div>
    <div class="setting-group">
      <label class="setting-label" for="voice-default-quality">Default quality</label>
      <select id="voice-default-quality" value={voiceQuality} onchange={pickVoicePreset}>
        {#each VOICE_QUALITY_PRESETS as preset (preset.quality)}
          <option value={preset.quality}>{preset.label}</option>
        {/each}
        {#if !VOICE_QUALITY_PRESETS.some((preset) => preset.quality === voiceQuality)}
          <!-- A server may be configured with a label this build does
               not know; keep it selectable instead of silently
               switching it to something else. -->
          <option value={voiceQuality}>{voiceQuality}</option>
        {/if}
      </select>
      <div class="setting-description">
        Quality preset assigned to new voice channels. Picking one fills in its bitrate.
      </div>
    </div>
    <div class="setting-group">
      <label class="setting-label" for="voice-default-bitrate">Default bitrate</label>
      <input
        id="voice-default-bitrate"
        type="number"
        bind:value={voiceBitrateKbps}
        min="0"
        max={MAX_VOICE_BITRATE / 1000}
        step="1"
      />
      <div class="setting-description">
        Bitrate in kbps for new voice channels, up to {MAX_VOICE_BITRATE / 1000} kbps.
        0 means uncompressed audio.
      </div>
      <div>
        <button class="btn btn-primary" onclick={saveVoiceDefaults} disabled={!voiceDirty}>
          Save changes
        </button>
      </div>
      {#if voiceFeedback}
        <div class="identity-feedback" class:error={voiceFeedback.kind === 'error'}>
          {voiceFeedback.text}
        </div>
      {/if}
    </div>
  </div>
{/if}
