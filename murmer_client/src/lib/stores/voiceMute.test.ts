import { beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';

/**
 * The mute snapshot is untrusted server input: one malformed entry must not
 * throw and cost the channel everyone else's mute indicators.
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

describe('voiceMute — the channel snapshot', () => {
  it('reads each entry and treats a null one as unmuted', async () => {
    const { voiceMuteStates } = await import('./voiceMute');
    bus.emit('voice-mute-active', {
      channelId: 1,
      states: { alice: { micMuted: true, outputMuted: false }, bob: null }
    });
    expect(get(voiceMuteStates)).toEqual({
      alice: { micMuted: true, outputMuted: false },
      bob: { micMuted: false, outputMuted: false }
    });
  });
});
