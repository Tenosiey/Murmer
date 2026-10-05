/*
 * Auto-away runs unattended, so getting the decision wrong is invisible until
 * someone notices they have been "away" all day — or that the app flipped a
 * status they chose by hand. The step function is the whole policy.
 */
import { describe, expect, it, vi } from 'vitest';

vi.mock('./chat', () => ({ chat: { on: vi.fn(), sendRaw: vi.fn() } }));

const { autoAwayStep } = await import('./autoAway');

describe('autoAwayStep', () => {
  it('goes away when an online user turns idle', () => {
    expect(autoAwayStep('online', true, false)).toBe('away');
  });

  it('comes back online only from an away it set itself', () => {
    expect(autoAwayStep('away', false, true)).toBe('online');
    expect(autoAwayStep('away', false, false)).toBeNull();
  });

  it('never touches busy or offline', () => {
    for (const status of ['busy', 'offline'] as const) {
      expect(autoAwayStep(status, true, false)).toBeNull();
      expect(autoAwayStep(status, false, true)).toBeNull();
    }
  });

  it('leaves an active online user alone', () => {
    expect(autoAwayStep('online', false, false)).toBeNull();
  });
});
