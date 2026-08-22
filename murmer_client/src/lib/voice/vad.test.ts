import { describe, it, expect } from 'vitest';
import { ReleaseGate } from './vad';
import {
  clampVadRelease,
  VAD_RELEASE_DEFAULT_MS,
  VAD_RELEASE_MAX_MS,
  VAD_RELEASE_MIN_MS
} from '../stores/settings';

/**
 * The gate's timings are invisible in the UI — a release applied a burst too
 * late, or one that ignores a change made mid-sentence, still looks and sounds
 * like a working microphone to the person holding it. Hence a fake clock.
 */
describe('ReleaseGate', () => {
  it('opens on the first sample above the threshold', () => {
    const gate = new ReleaseGate();
    expect(gate.update(false, 0, 900)).toBe(false);
    expect(gate.update(true, 16, 900)).toBe(true);
    expect(gate.isOpen).toBe(true);
  });

  it('holds through a pause and closes once the release has elapsed', () => {
    const gate = new ReleaseGate();
    gate.update(true, 1000, 900);
    expect(gate.update(false, 1500, 900)).toBe(true);
    expect(gate.update(false, 1899, 900)).toBe(true);
    expect(gate.update(false, 1900, 900)).toBe(false);
  });

  it('re-arms the release on every burst of speech', () => {
    const gate = new ReleaseGate();
    gate.update(true, 0, 900);
    gate.update(false, 800, 900);
    // A word lands inside the release window: the gate must stay open for a
    // full release measured from *this* word, not from the previous one.
    gate.update(true, 850, 900);
    expect(gate.update(false, 1700, 900)).toBe(true);
    expect(gate.update(false, 1750, 900)).toBe(false);
  });

  it('applies a release changed mid-release instead of the one in flight', () => {
    const gate = new ReleaseGate();
    gate.update(true, 0, 2000);
    expect(gate.update(false, 500, 2000)).toBe(true);
    // Slider dragged down while the gate is coasting: the shorter release is
    // already past, so it closes on this tick rather than at the old deadline.
    expect(gate.update(false, 516, 200)).toBe(false);
  });

  it('closes on the first quiet sample with no release at all', () => {
    const gate = new ReleaseGate();
    gate.update(true, 0, 0);
    expect(gate.update(false, 16, 0)).toBe(false);
  });

  it('closes and forgets the last burst on reset', () => {
    const gate = new ReleaseGate();
    gate.update(true, 1000, 900);
    gate.reset();
    expect(gate.isOpen).toBe(false);
    // Without forgetting `lastAbove`, a restart whose clock is past the old
    // deadline would report the gate open for one tick.
    expect(gate.update(false, 5000, 900)).toBe(false);
  });
});

describe('clampVadRelease', () => {
  it('keeps a value inside the scale, rounded to whole milliseconds', () => {
    expect(clampVadRelease(450.4)).toBe(450);
    expect(clampVadRelease(VAD_RELEASE_DEFAULT_MS)).toBe(VAD_RELEASE_DEFAULT_MS);
  });

  it('clamps a stored value from outside the scale to its ends', () => {
    expect(clampVadRelease(-1)).toBe(VAD_RELEASE_MIN_MS);
    expect(clampVadRelease(60_000)).toBe(VAD_RELEASE_MAX_MS);
  });

  it('falls back to the default for a value that is not a number', () => {
    expect(clampVadRelease(Number.NaN)).toBe(VAD_RELEASE_DEFAULT_MS);
    expect(clampVadRelease(Number.POSITIVE_INFINITY)).toBe(VAD_RELEASE_DEFAULT_MS);
  });
});
