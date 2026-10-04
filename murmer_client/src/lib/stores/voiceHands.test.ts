import { beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { handQueue } from './voiceHands';

/**
 * The queue order is the whole point of raising a hand, and a hand that never
 * comes down — a lost lowered frame, a stale snapshot — sits in the sidebar
 * looking like somebody is still waiting.
 */
const bus = vi.hoisted(() => {
  const handlers = new Map<string, ((msg: any) => void)[]>();
  return {
    handlers,
    emit(type: string, payload: Record<string, unknown>) {
      for (const handler of handlers.get(type) ?? []) handler({ type, ...payload });
    }
  };
});

vi.mock('./chat', () => ({
  chat: {
    sendRaw() {},
    on(type: string, callback: (msg: any) => void) {
      const list = bus.handlers.get(type) ?? [];
      list.push(callback);
      bus.handlers.set(type, list);
    },
    off() {}
  }
}));

beforeEach(() => {
  bus.handlers.clear();
  vi.resetModules();
});

describe('handQueue', () => {
  it('orders one channel by when each hand went up', () => {
    const hands = {
      carol: { channelId: 1, at: 30 },
      alice: { channelId: 1, at: 10 },
      bob: { channelId: 2, at: 5 }
    };
    expect(handQueue(hands, 1)).toEqual(['alice', 'carol']);
  });
});

describe('voiceHands', () => {
  it('raises and lowers, ignoring a frame without a timestamp', async () => {
    const { voiceHands } = await import('./voiceHands');
    bus.emit('voice-hand', { user: 'bob', channelId: 1, raised: true, at: 7 });
    bus.emit('voice-hand', { user: 'eve', channelId: 1, raised: true });
    expect(get(voiceHands)).toEqual({ bob: { channelId: 1, at: 7 } });
    bus.emit('voice-hand', { user: 'bob', channelId: 1, raised: false, at: null });
    expect(get(voiceHands)).toEqual({});
  });

  it('lets the snapshot replace its own channel and keep the others', async () => {
    const { voiceHands } = await import('./voiceHands');
    bus.emit('voice-hand', { user: 'stale', channelId: 1, raised: true, at: 1 });
    bus.emit('voice-hand', { user: 'elsewhere', channelId: 2, raised: true, at: 2 });
    bus.emit('voice-hands-active', { channelId: 1, hands: { bob: 9, junk: 'x' } });
    expect(get(voiceHands)).toEqual({
      elsewhere: { channelId: 2, at: 2 },
      bob: { channelId: 1, at: 9 }
    });
  });
});
