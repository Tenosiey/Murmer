import { beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';

/**
 * Who joins a call is decided here, on frames every device of both accounts
 * receives. Getting it wrong is invisible until two people talk: a second
 * device of the callee joining alongside the first, or a caller's laptop
 * opening its microphone because their phone placed the call.
 */
const bus = vi.hoisted(() => {
  const handlers = new Map<string, ((msg: any) => void)[]>();
  return {
    handlers,
    sent: [] as Record<string, unknown>[],
    emit(type: string, payload: Record<string, unknown>) {
      for (const handler of handlers.get(type) ?? []) handler({ type, ...payload });
    }
  };
});

vi.mock('./chat', () => ({
  chat: {
    sendRaw(frame: Record<string, unknown>) {
      bus.sent.push(frame);
    },
    on(type: string, callback: (msg: any) => void) {
      const list = bus.handlers.get(type) ?? [];
      list.push(callback);
      bus.handlers.set(type, list);
    },
    off() {}
  }
}));
vi.mock('../notify', () => ({ notify: vi.fn() }));
vi.mock('../voice/ringtone', () => ({ startRingtone: vi.fn(), stopRingtone: vi.fn() }));
vi.mock('./blocks', () => ({ isBlocked: (user: string) => user === 'mallory' }));

let calls: typeof import('./calls');
const join = vi.fn();
const leave = vi.fn();

beforeEach(async () => {
  bus.handlers.clear();
  bus.sent = [];
  join.mockClear();
  leave.mockClear();
  vi.resetModules();
  const { session } = await import('./session');
  session.set({ user: 'bob' });
  const { connection } = await import('./connection');
  connection.set('connected');
  calls = await import('./calls');
  calls.setCallVoiceHooks({ join, leave });
});

const ringing = (caller: string, callee: string) =>
  bus.emit('call-ringing', { callId: -1, caller, callee });

describe('calls', () => {
  it('joins an outgoing call only on the device that placed it', () => {
    ringing('bob', 'carol');
    expect(get(calls.call)?.outgoing).toBe(true);
    expect(join).not.toHaveBeenCalled();
    bus.emit('call-ended', { callId: -1, reason: 'cancelled' });

    calls.startCall('carol');
    expect(bus.sent).toContainEqual({ type: 'call-start', target: 'carol' });
    ringing('bob', 'carol');
    expect(join).toHaveBeenCalledWith(-1);
  });

  it('answers on this device and leaves when the call ends', () => {
    ringing('carol', 'bob');
    expect(get(calls.call)).toMatchObject({ peer: 'carol', outgoing: false, accepted: false });
    calls.acceptCall();
    expect(bus.sent).toContainEqual({ type: 'call-accept', callId: -1 });
    expect(join).toHaveBeenCalledWith(-1);
    bus.emit('call-accepted', { callId: -1 });
    bus.emit('call-ended', { callId: -1, reason: 'ended' });
    expect(leave).toHaveBeenCalledWith(-1);
    expect(get(calls.call)).toBeNull();
  });

  it('stops ringing when another device answers, without joining', () => {
    ringing('carol', 'bob');
    bus.emit('call-accepted', { callId: -1 });
    expect(get(calls.call)).toBeNull();
    bus.emit('call-ended', { callId: -1, reason: 'ended' });
    expect(join).not.toHaveBeenCalled();
    expect(leave).not.toHaveBeenCalled();
  });

  it('ignores blocked callers, other people’s calls and malformed frames', () => {
    ringing('mallory', 'bob');
    ringing('carol', 'dave');
    bus.emit('call-ringing', { callId: 3, caller: 'carol', callee: 'bob' });
    bus.emit('call-ringing', { callId: '-1', caller: 'carol', callee: 'bob' });
    bus.emit('call-ringing', { callId: -1, caller: 7, callee: 'bob' });
    expect(get(calls.call)).toBeNull();
  });

  it('ignores frames for a different call', () => {
    ringing('carol', 'bob');
    bus.emit('call-ended', { callId: -2, reason: 'ended' });
    expect(get(calls.call)?.id).toBe(-1);
  });
});
