<!--
  Live microphone level meter drawn on the same scale as the VAD sensitivity
  slider it sits under: the threshold marker lines up with the slider thumb, so
  the threshold can be dialled in while watching one's own voice level.

  The meter only measures while it is mounted; unmounting releases the
  microphone it opened (unless it borrowed a running voice session's capture).
-->
<script lang="ts">
  import { onDestroy } from 'svelte';
  import { MicLevelMonitor } from '$lib/voice/micLevel';
  import { captureStream } from '$lib/stores/voiceCapture';
  import {
    inputDeviceId,
    echoCancellation,
    noiseSuppression,
    autoGainControl
  } from '$lib/stores/settings';

  interface Props {
    /**
     * Threshold the level is compared against, on the same 0-1 scale. Omitted
     * outside voice-activity mode, where the meter is only a "does my
     * microphone work" check and has no marker to line up with.
     */
    threshold?: number;
    /** Lower end of the displayed scale (the slider's `min`). */
    min: number;
    /** Upper end of the displayed scale (the slider's `max`). */
    max: number;
  }

  let { threshold, min, max }: Props = $props();

  /** Floor separating silence from signal when there is no VAD threshold. */
  const SIGNAL_FLOOR = 0.02;

  let level = $state(0);
  let failed = $state(false);

  const monitor = new MicLevelMonitor();

  // Restart whenever the capture configuration changes so the meter always
  // shows the signal a voice session would actually use.
  $effect(() => {
    // Tracked dependencies: any of these changes the captured signal.
    void $inputDeviceId;
    void $echoCancellation;
    void $noiseSuppression;
    void $autoGainControl;
    // A running session's capture is borrowed rather than reopened, so the
    // meter has to re-attach when the manager swaps it — otherwise switching
    // the input device mid-call kept metering the old microphone.
    void $captureStream;

    let current = true;
    failed = false;
    level = 0;
    monitor
      .start((value) => {
        if (current) level = value;
      })
      .catch((error) => {
        if (current) failed = true;
        console.warn('Failed to start microphone level meter:', error);
      });

    return () => {
      current = false;
      monitor.stop();
    };
  });

  onDestroy(() => monitor.stop());

  function position(value: number): number {
    return Math.min(100, Math.max(0, ((value - min) / (max - min)) * 100));
  }

  const levelPercent = $derived(position(level));
  const thresholdPercent = $derived(position(threshold ?? 0));
  const speaking = $derived(level > (threshold ?? SIGNAL_FLOOR));
</script>

<div class="meter">
  <div class="meter-track">
    <div class="meter-fill" class:speaking style="width: {levelPercent}%"></div>
    {#if threshold !== undefined}
      <div class="meter-threshold" style="left: {thresholdPercent}%"></div>
    {/if}
  </div>
  <span class="meter-status" class:failed>
    {#if failed}
      Microphone unavailable — check the input device and permissions
    {:else if threshold === undefined}
      {speaking ? 'Picking up sound' : 'Silent'}
    {:else if speaking}
      Transmitting
    {:else}
      Silent
    {/if}
  </span>
</div>

<style>
  .meter {
    display: grid;
    gap: var(--space-1);
    /* Match the range input's thumb travel so the threshold marker lines up
       with the slider thumb above it. */
    padding: 0 8px;
  }

  .meter-track {
    position: relative;
    height: 6px;
    background: var(--color-surface-raised);
    border-radius: var(--radius-pill);
    overflow: hidden;
  }

  .meter-fill {
    height: 100%;
    background: var(--color-muted);
    border-radius: var(--radius-pill);
    transition: background 0.1s linear;
  }

  .meter-fill.speaking {
    background: var(--color-success);
  }

  .meter-threshold {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: var(--color-on-surface);
  }

  .meter-status {
    font-size: var(--text-xs);
    color: var(--color-muted);
  }

  .meter-status.failed {
    color: var(--color-error);
  }
</style>
