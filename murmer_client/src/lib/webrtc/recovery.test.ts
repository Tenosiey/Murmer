import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { PeerRecovery, type PeerRecoveryTimings } from './recovery';

/** Round numbers, so the assertions below read as the timeline they describe. */
const TIMINGS: PeerRecoveryTimings = {
  graceMs: 3_000,
  retryMs: 5_000,
  deadlineMs: 30_000
};

function setup(timings: PeerRecoveryTimings = TIMINGS) {
  const hooks = {
    restart: vi.fn(),
    rebuild: vi.fn(),
    setReconnecting: vi.fn()
  };
  return { hooks, recovery: new PeerRecovery(hooks, timings) };
}

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe('PeerRecovery', () => {
  it('rides out a blip that heals itself inside the grace period', () => {
    const { hooks, recovery } = setup();

    // The exact case the old "close it on `disconnected`" behaviour got wrong:
    // a two-second interruption is a roam, not a lost peer.
    recovery.observe('bob', 'disconnected');
    vi.advanceTimersByTime(2_000);
    recovery.observe('bob', 'connected');
    vi.advanceTimersByTime(60_000);

    expect(hooks.restart).not.toHaveBeenCalled();
    expect(hooks.rebuild).not.toHaveBeenCalled();
    expect(recovery.isRecovering('bob')).toBe(false);
  });

  it('reports the repair to the UI once in each direction', () => {
    const { hooks, recovery } = setup();

    recovery.observe('bob', 'disconnected');
    // Repeated `disconnected` events must not restate what is already showing.
    recovery.observe('bob', 'disconnected');
    vi.advanceTimersByTime(10_000);
    recovery.observe('bob', 'connected');
    recovery.observe('bob', 'connected');

    expect(hooks.setReconnecting.mock.calls).toEqual([
      ['bob', true],
      ['bob', false]
    ]);
  });

  it('restarts ICE once the grace period passes without recovery', () => {
    const { hooks, recovery } = setup();

    recovery.observe('bob', 'disconnected');
    vi.advanceTimersByTime(2_999);
    expect(hooks.restart).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(hooks.restart).toHaveBeenCalledExactlyOnceWith('bob');
  });

  it('acts immediately on `failed` instead of waiting out a grace period', () => {
    const { hooks, recovery } = setup();

    recovery.observe('bob', 'failed');
    vi.advanceTimersByTime(0);

    expect(hooks.restart).toHaveBeenCalledExactlyOnceWith('bob');
  });

  it('brings the attempt forward when a disconnect turns into a failure', () => {
    const { hooks, recovery } = setup();

    recovery.observe('bob', 'disconnected');
    vi.advanceTimersByTime(1_000);
    recovery.observe('bob', 'failed');
    vi.advanceTimersByTime(0);

    expect(hooks.restart).toHaveBeenCalledExactlyOnceWith('bob');

    // The deadline still runs from the original breakage, not from the
    // failure — otherwise a flapping connection could postpone it forever.
    vi.advanceTimersByTime(29_000);
    expect(hooks.rebuild).toHaveBeenCalledExactlyOnceWith('bob');
  });

  it('keeps retrying on the retry interval while the peer stays broken', () => {
    const { hooks, recovery } = setup();

    recovery.observe('bob', 'disconnected');
    vi.advanceTimersByTime(3_000);
    expect(hooks.restart).toHaveBeenCalledTimes(1);

    vi.advanceTimersByTime(5_000);
    expect(hooks.restart).toHaveBeenCalledTimes(2);

    vi.advanceTimersByTime(5_000);
    expect(hooks.restart).toHaveBeenCalledTimes(3);
  });

  it('rebuilds the connection exactly when the deadline expires', () => {
    const { hooks, recovery } = setup();

    recovery.observe('bob', 'disconnected');
    vi.advanceTimersByTime(29_999);
    expect(hooks.rebuild).not.toHaveBeenCalled();

    vi.advanceTimersByTime(1);
    expect(hooks.rebuild).toHaveBeenCalledExactlyOnceWith('bob');
    expect(recovery.isRecovering('bob')).toBe(false);
    // The rebuild owns the peer from here; nothing may keep firing behind it.
    const restarts = hooks.restart.mock.calls.length;
    vi.advanceTimersByTime(60_000);
    expect(hooks.restart).toHaveBeenCalledTimes(restarts);
    expect(hooks.rebuild).toHaveBeenCalledTimes(1);
  });

  it('clears the reconnecting flag before handing the peer to the rebuild', () => {
    const { hooks, recovery } = setup();

    recovery.observe('bob', 'disconnected');
    vi.advanceTimersByTime(30_000);

    expect(hooks.setReconnecting).toHaveBeenLastCalledWith('bob', false);
    expect(hooks.setReconnecting).toHaveBeenCalledBefore(hooks.rebuild);
  });

  it('recovers a peer that comes back between two retries', () => {
    const { hooks, recovery } = setup();

    recovery.observe('bob', 'disconnected');
    vi.advanceTimersByTime(3_000);
    expect(hooks.restart).toHaveBeenCalledTimes(1);

    recovery.observe('bob', 'connected');
    vi.advanceTimersByTime(60_000);

    expect(hooks.restart).toHaveBeenCalledTimes(1);
    expect(hooks.rebuild).not.toHaveBeenCalled();
  });

  it('tracks peers independently', () => {
    const { hooks, recovery } = setup();

    recovery.observe('bob', 'disconnected');
    vi.advanceTimersByTime(1_000);
    recovery.observe('carol', 'failed');
    vi.advanceTimersByTime(2_000);

    expect(hooks.restart.mock.calls).toEqual([['carol'], ['bob']]);
    recovery.observe('bob', 'connected');
    vi.advanceTimersByTime(60_000);
    expect(hooks.rebuild).toHaveBeenCalledExactlyOnceWith('carol');
  });

  it('stops working on a peer that was closed deliberately', () => {
    const { hooks, recovery } = setup();

    recovery.observe('bob', 'disconnected');
    recovery.observe('bob', 'closed');
    vi.advanceTimersByTime(60_000);

    expect(hooks.restart).not.toHaveBeenCalled();
    expect(hooks.rebuild).not.toHaveBeenCalled();
  });

  it('cancels everything on clear, so leaving a channel repairs nothing', () => {
    const { hooks, recovery } = setup();

    recovery.observe('bob', 'disconnected');
    recovery.observe('carol', 'failed');
    recovery.clear();
    vi.advanceTimersByTime(60_000);

    expect(hooks.restart).not.toHaveBeenCalled();
    expect(hooks.rebuild).not.toHaveBeenCalled();
    expect(recovery.isRecovering('bob')).toBe(false);
  });

  it('forgets a single peer without disturbing the others', () => {
    const { hooks, recovery } = setup();

    recovery.observe('bob', 'disconnected');
    recovery.observe('carol', 'disconnected');
    recovery.forget('bob');
    vi.advanceTimersByTime(3_000);

    expect(hooks.restart).toHaveBeenCalledExactlyOnceWith('carol');
  });
});
