import { beforeEach, describe, expect, it, vi } from 'vitest';

/**
 * A poke pops up whatever the mute settings say, so the one way to stop one
 * is the block list — a poke from a blocked member must never reach a dialog.
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
const alert = vi.hoisted(() => vi.fn(() => Promise.resolve()));

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
vi.mock('./dialogs', () => ({ dialogs: { alert } }));
vi.mock('../notify', () => ({ notify: vi.fn() }));
vi.mock('./blocks', () => ({ isBlocked: (user: string) => user === 'mallory' }));

beforeEach(async () => {
  bus.handlers.clear();
  alert.mockClear();
  vi.resetModules();
  await import('./pokes');
});

describe('pokes', () => {
  it('pops up a poke from a member', () => {
    bus.emit('poke', { from: 'alice' });
    expect(alert).toHaveBeenCalledWith({ title: 'Poke', message: 'alice poked you.' });
  });

  it('drops a poke from a blocked member or a malformed frame', () => {
    bus.emit('poke', { from: 'mallory' });
    bus.emit('poke', { from: 42 });
    bus.emit('poke', {});
    expect(alert).not.toHaveBeenCalled();
  });
});
