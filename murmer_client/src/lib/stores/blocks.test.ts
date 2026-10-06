/**
 * A block is only a name in a list, so the failure modes are the list
 * leaking to a different server — where the same account name is somebody
 * else — or not surviving a restart.
 */
import { describe, expect, it, vi } from 'vitest';

async function loadBlocks() {
  vi.resetModules();
  return await import('./blocks');
}

describe('blockedUsers', () => {
  it('keeps each server its own list and survives a reload', async () => {
    localStorage.clear();
    let { blockedUsers, isBlocked } = await loadBlocks();
    blockedUsers.setServer('wss://one.example/ws');
    blockedUsers.setBlocked('mallory', true);
    expect(isBlocked('mallory')).toBe(true);

    blockedUsers.setServer('wss://two.example/ws');
    expect(isBlocked('mallory')).toBe(false);

    ({ blockedUsers, isBlocked } = await loadBlocks());
    blockedUsers.setServer('wss://one.example/ws');
    expect(isBlocked('mallory')).toBe(true);
    blockedUsers.setBlocked('mallory', false);
    expect(isBlocked('mallory')).toBe(false);
  });

  it('silences a blocked member’s soundboard clips', async () => {
    localStorage.clear();
    const { blockedUsers } = await loadBlocks();
    const { effectiveGain } = await import('./soundboardSettings');
    blockedUsers.setServer('wss://one.example/ws');
    expect(effectiveGain(1, 'mallory')).not.toBeNull();
    blockedUsers.setBlocked('mallory', true);
    expect(effectiveGain(1, 'mallory')).toBeNull();
  });
});
