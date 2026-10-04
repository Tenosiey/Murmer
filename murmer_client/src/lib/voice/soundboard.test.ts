import { describe, expect, it } from 'vitest';
import { othersSpeaking } from './soundboard';

/**
 * A duck that never lifts leaves the soundboard stuck at a quarter volume,
 * and one that reacts to the listener's own voice ducks every clip they play
 * over themselves. Neither throws, so only this notices.
 */
describe('othersSpeaking', () => {
  it('ducks for somebody else talking', () => {
    expect(othersSpeaking({ bob: true }, 'alice')).toBe(true);
  });

  it('ignores the local user and lifts once nobody else talks', () => {
    expect(othersSpeaking({ alice: true }, 'alice')).toBe(false);
    expect(othersSpeaking({}, 'alice')).toBe(false);
    expect(othersSpeaking({ bob: false }, 'alice')).toBe(false);
  });
});
