import { describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';

const MODE_KEY = 'murmer_noise_suppression_mode';

/**
 * The settings store reads `localStorage` once at import time, and
 * `micProcessingConstraints` reads that store, so both come from one fresh
 * module instance per test.
 */
async function loadCapture(stored?: string) {
  vi.resetModules();
  if (stored !== undefined) localStorage.setItem(MODE_KEY, stored);
  const settings = await import('../stores/settings');
  const capture = await import('./capture');
  return { ...settings, ...capture };
}

/**
 * Which suppressor the capture asks the platform for is invisible in the UI —
 * both modes look like "noise suppression is on" — but getting it wrong is
 * audible: the platform filter running ahead of RNNoise hands the better filter
 * a signal it has already mangled.
 */
describe('micProcessingConstraints — noise suppression mode', () => {
  it('asks the platform for its filter only in browser mode', async () => {
    const { noiseSuppressionMode, micProcessingConstraints } = await loadCapture();

    noiseSuppressionMode.set('browser');
    expect(micProcessingConstraints().noiseSuppression).toBe(true);

    noiseSuppressionMode.set('rnnoise');
    expect(micProcessingConstraints().noiseSuppression).toBe(false);

    noiseSuppressionMode.set('off');
    expect(micProcessingConstraints().noiseSuppression).toBe(false);
  });

  it('leaves the other two processing settings alone', async () => {
    const { echoCancellation, autoGainControl, noiseSuppressionMode, micProcessingConstraints } =
      await loadCapture();

    noiseSuppressionMode.set('rnnoise');
    echoCancellation.set(false);
    autoGainControl.set(true);

    expect(micProcessingConstraints()).toMatchObject({
      echoCancellation: false,
      autoGainControl: true
    });
  });
});

describe('noiseSuppressionMode — persistence', () => {
  it('defaults to RNNoise when nothing is stored', async () => {
    const { noiseSuppressionMode, micProcessingConstraints } = await loadCapture();

    expect(get(noiseSuppressionMode)).toBe('rnnoise');
    expect(micProcessingConstraints().noiseSuppression).toBe(false);
  });

  it('restores a stored mode', async () => {
    const { noiseSuppressionMode } = await loadCapture('browser');
    expect(get(noiseSuppressionMode)).toBe('browser');
  });

  it('falls back to the default for a value it does not know', async () => {
    // A hand-edited entry, or the boolean the old setting used to persist under
    // its own key — neither may leave the app in a mode nothing handles.
    const { noiseSuppressionMode } = await loadCapture('true');
    expect(get(noiseSuppressionMode)).toBe('rnnoise');
  });

  it('persists the mode', async () => {
    const { noiseSuppressionMode } = await loadCapture();

    noiseSuppressionMode.set('off');
    expect(localStorage.getItem(MODE_KEY)).toBe('off');
  });
});
