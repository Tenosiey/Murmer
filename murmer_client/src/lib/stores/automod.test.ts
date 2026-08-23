/**
 * The auto-moderation rule store is worth testing for one reason: everything
 * it gets wrong is invisible.
 *
 * "Not disclosed yet" and "no rules" are the same shape on screen and mean
 * opposite things — confusing them lets the editor offer to save an empty
 * list over a server's real one. A rule whose action this build does not
 * recognise must be dropped rather than shown as something else, because a
 * manager who then saves would replace the real action with the guess. And a
 * list left behind on disconnect would show one server's moderation policy
 * while connected to another.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { DEFAULT_AUTOMOD_MUTE_SECONDS, MAX_MUTE_SECONDS, MIN_MUTE_SECONDS } from '../chat/constants';

/** Stand-in for the chat store, as in the other store tests. */
const bus = vi.hoisted(() => {
  const sent: Record<string, any>[] = [];
  const handlers = new Map<string, ((msg: any) => void)[]>();
  return {
    sent,
    handlers,
    reset() {
      sent.length = 0;
      handlers.clear();
    },
    emit(type: string, payload: Record<string, unknown>) {
      for (const handler of handlers.get(type) ?? []) handler({ type, ...payload });
    }
  };
});

vi.mock('./chat', () => ({
  chat: {
    sendRaw(data: Record<string, any>) {
      bus.sent.push(data);
    },
    on(type: string, callback: (msg: any) => void) {
      const list = bus.handlers.get(type) ?? [];
      list.push(callback);
      bus.handlers.set(type, list);
    },
    off() {}
  }
}));

/** The store wires up its handlers on import, so each test gets a fresh one. */
async function load() {
  bus.reset();
  vi.resetModules();
  const [automod, { connection }] = await Promise.all([
    import('./automod'),
    import('./connection')
  ]);
  connection.set('connected');
  return { ...automod, connection };
}

const rule = (over: Record<string, unknown> = {}) => ({
  name: 'Invites',
  pattern: 'discord.gg/',
  kind: 'substring',
  action: 'delete',
  muteSeconds: DEFAULT_AUTOMOD_MUTE_SECONDS,
  enabled: true,
  ...over
});

beforeEach(() => {
  bus.reset();
});

describe('automodRules — the undisclosed state', () => {
  it('starts null rather than empty', async () => {
    const { automodRules } = await load();
    expect(get(automodRules)).toBeNull();
  });

  it('takes an empty list as a real answer', async () => {
    const { automodRules } = await load();
    bus.emit('automod-rules', { rules: [] });
    expect(get(automodRules)).toEqual([]);
  });

  it('forgets the rules when the connection drops', async () => {
    const { automodRules, connection } = await load();
    bus.emit('automod-rules', { rules: [rule()] });
    expect(get(automodRules)).toHaveLength(1);

    connection.set('disconnected');
    expect(get(automodRules)).toBeNull();
  });

  it('ignores a frame carrying no rule array at all', async () => {
    const { automodRules } = await load();
    bus.emit('automod-rules', { rules: [rule()] });
    bus.emit('automod-rules', {});
    expect(get(automodRules)).toHaveLength(1);
  });
});

describe('automodRules — parsing an untrusted frame', () => {
  it('keeps a well-formed rule as sent', async () => {
    const { automodRules } = await load();
    bus.emit('automod-rules', { rules: [rule()] });
    expect(get(automodRules)).toEqual([
      {
        name: 'Invites',
        pattern: 'discord.gg/',
        kind: 'substring',
        action: 'delete',
        muteSeconds: DEFAULT_AUTOMOD_MUTE_SECONDS,
        enabled: true
      }
    ]);
  });

  it('drops a rule whose kind or action this build does not know', async () => {
    const { automodRules } = await load();
    bus.emit('automod-rules', {
      rules: [rule({ kind: 'fuzzy' }), rule({ action: 'ban' }), rule({ name: 'Kept' })]
    });
    // Showing an unknown action as something else would let a manager save
    // the guess back over the real one.
    expect(get(automodRules)?.map((entry) => entry.name)).toEqual(['Kept']);
  });

  it('drops a rule with no pattern, and anything that is not an object', async () => {
    const { automodRules } = await load();
    bus.emit('automod-rules', {
      rules: [rule({ pattern: '' }), rule({ pattern: 7 }), null, 'nonsense', rule({ name: 'Kept' })]
    });
    expect(get(automodRules)?.map((entry) => entry.name)).toEqual(['Kept']);
  });

  it('clamps a mute duration outside the bounds this build issues', async () => {
    const { automodRules } = await load();
    bus.emit('automod-rules', {
      rules: [
        rule({ action: 'mute', muteSeconds: MAX_MUTE_SECONDS * 10 }),
        rule({ action: 'mute', muteSeconds: 0 }),
        rule({ action: 'mute', muteSeconds: 'soon' })
      ]
    });
    expect(get(automodRules)?.map((entry) => entry.muteSeconds)).toEqual([
      MAX_MUTE_SECONDS,
      MIN_MUTE_SECONDS,
      DEFAULT_AUTOMOD_MUTE_SECONDS
    ]);
  });

  it('treats a missing name as empty and a missing enabled as on', async () => {
    const { automodRules } = await load();
    const { name, enabled, ...rest } = rule();
    bus.emit('automod-rules', { rules: [rest] });
    expect(get(automodRules)?.[0]).toMatchObject({ name: '', enabled: true });
  });
});

describe('automodRules — what it asks the server', () => {
  it('requests and saves through the frames the server dispatches on', async () => {
    const { requestAutomodRules, setAutomodRules, automodRules } = await load();

    requestAutomodRules();
    expect(bus.sent).toEqual([{ type: 'get-automod-rules' }]);

    const rules = [rule()] as any;
    setAutomodRules(rules);
    expect(bus.sent[1]).toEqual({ type: 'set-automod-rules', rules });
    // Saving never writes the store: a rule the server refuses must not look
    // saved, so only its answer moves the cached list.
    expect(get(automodRules)).toBeNull();
  });
});
