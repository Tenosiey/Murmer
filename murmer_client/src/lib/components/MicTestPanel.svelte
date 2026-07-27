<!--
  Microphone test: records a few seconds and plays them straight back, so the
  effect of the processing settings above can be heard instead of guessed at
  from a level bar.

  The recording is dropped when the panel unmounts (closing settings or
  switching tabs) — it is a throwaway check, never something that is stored.
-->
<script lang="ts">
  import { onDestroy } from 'svelte';
  import { MicTest, MIC_TEST_SECONDS } from '$lib/voice/micTest';

  type Phase = 'idle' | 'recording' | 'playing';

  const test = new MicTest();

  let phase: Phase = $state('idle');
  let elapsed = $state(0);
  let error = $state('');
  let recording: AudioBuffer | null = $state(null);

  async function startRecording() {
    error = '';
    recording = null;
    elapsed = 0;
    phase = 'recording';
    try {
      const buffer = await test.record((seconds) => {
        elapsed = seconds;
      });
      // Abandoned (panel unmounted or a new test started).
      if (buffer === null) {
        if (phase === 'recording') phase = 'idle';
        return;
      }
      recording = buffer;
      playRecording();
    } catch (e) {
      console.warn('Microphone test failed:', e);
      error = 'Could not record — check the input device and microphone permissions.';
      phase = 'idle';
    }
  }

  function playRecording() {
    if (!recording) return;
    phase = 'playing';
    test.play(recording, () => {
      phase = 'idle';
    });
  }

  function stop() {
    if (phase === 'recording') {
      // Keeps what was captured so far, which then plays back as usual.
      test.stopRecording();
    } else if (phase === 'playing') {
      test.stopPlayback();
      phase = 'idle';
    }
  }

  onDestroy(() => test.dispose());

  let remaining = $derived(Math.max(0, MIC_TEST_SECONDS - elapsed));
  let progress = $derived(Math.min(100, (elapsed / MIC_TEST_SECONDS) * 100));
</script>

<div class="mic-test">
  <div class="controls">
    {#if phase === 'idle'}
      <button class="btn" onclick={startRecording}>
        {recording ? 'Record again' : `Record ${MIC_TEST_SECONDS} seconds`}
      </button>
      {#if recording}
        <button class="btn" onclick={playRecording}>Play back</button>
      {/if}
    {:else}
      <button class="btn" onclick={stop}>
        {phase === 'recording' ? 'Stop and play back' : 'Stop playback'}
      </button>
    {/if}
  </div>

  {#if phase === 'recording'}
    <div class="progress" role="progressbar" aria-label="Recording progress" aria-valuenow={Math.round(progress)} aria-valuemin="0" aria-valuemax="100">
      <div class="progress-fill" style="width: {progress}%"></div>
    </div>
  {/if}

  <span class="status" class:failed={Boolean(error)} role="status">
    {#if error}
      {error}
    {:else if phase === 'recording'}
      Recording — speak normally ({Math.ceil(remaining)}s left)
    {:else if phase === 'playing'}
      Playing back — only you hear this
    {:else if recording}
      Adjust the settings above and record again to compare.
    {:else}
      Records what the others would hear, including the input volume, and plays it
      straight back to you.
    {/if}
  </span>
</div>

<style>
  .mic-test {
    display: grid;
    gap: var(--space-2);
    justify-items: start;
  }

  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
  }

  .progress {
    width: 100%;
    height: 6px;
    background: var(--color-surface-raised);
    border-radius: var(--radius-pill);
    overflow: hidden;
  }

  .progress-fill {
    height: 100%;
    background: var(--color-primary);
    border-radius: var(--radius-pill);
  }

  .status {
    font-size: var(--text-sm);
    color: var(--color-muted);
    line-height: 1.5;
  }

  .status.failed {
    color: var(--color-error);
  }
</style>
