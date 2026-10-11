import { beforeEach, describe, expect, it, vi } from 'vitest';

/**
 * The remembered status rides in every presence frame. If auto-away wrote
 * it, a member would reconnect as "away"; if it were lost, a member
 * appearing offline would be announced as online on every reconnect.
 */
const sendRaw = vi.hoisted(() => vi.fn());
vi.mock('./chat', () => ({ chat: { sendRaw, on() {}, off() {} } }));
vi.mock('./session', async () => {
  const { writable } = await import('svelte/store');
  return { session: writable({ user: 'alice' }) };
});

beforeEach(() => {
  localStorage.clear();
  sendRaw.mockClear();
});

describe('statuses.chosen', () => {
  it('defaults to online', async () => {
    const { statuses } = await import('./status');
    expect(statuses.chosen()).toBe('online');
  });

  it('remembers a status the user picked, not one auto-away set', async () => {
    const { statuses } = await import('./status');
    statuses.setSelf('offline', true);
    statuses.setSelf('away');
    expect(statuses.chosen()).toBe('offline');
    expect(sendRaw).toHaveBeenLastCalledWith({ type: 'status-update', status: 'away' });
  });

  it('ignores a stored value that is not a status', async () => {
    const { statuses } = await import('./status');
    localStorage.setItem('murmer_chosen_status', 'lurking');
    expect(statuses.chosen()).toBe('online');
  });
});
