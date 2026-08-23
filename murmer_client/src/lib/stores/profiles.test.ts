import { beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { MAX_DISPLAY_NAME_LENGTH, MAX_NICKNAME_LENGTH } from '../chat/constants';

/**
 * The name-resolution rule is the part of this store whose failure is
 * invisible: rendering the wrong one of three names looks like a name, and a
 * nickname a moderator set silently losing to the user's own display name is
 * exactly the case the feature exists to prevent.
 */
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

/** The store registers its handlers on import, so each test gets a fresh one. */
async function load() {
  bus.reset();
  vi.resetModules();
  return import('./profiles');
}

beforeEach(() => {
  bus.reset();
});

describe('name resolution', () => {
  it('falls back to the account name when nothing is set', async () => {
    const { displayNames } = await load();
    bus.emit('profile-snapshot', {
      profiles: [{ user: 'alice', displayName: '', nickname: '', about: '' }]
    });
    expect(get(displayNames)('alice')).toBe('alice');
    // Never seen at all — the member list still has to render something.
    expect(get(displayNames)('stranger')).toBe('stranger');
  });

  it('prefers the display name over the account name', async () => {
    const { displayNames } = await load();
    bus.emit('profile-snapshot', {
      profiles: [{ user: 'alice', displayName: 'Alice A.', nickname: '' }]
    });
    expect(get(displayNames)('alice')).toBe('Alice A.');
  });

  it('lets the nickname win over the display name', async () => {
    const { displayNames } = await load();
    bus.emit('profile-snapshot', {
      profiles: [{ user: 'alice', displayName: 'Alice A.', nickname: 'Sparky' }]
    });
    // The nickname is this server's label and may have been set by a
    // moderator, so editing the display name must not shake it off.
    expect(get(displayNames)('alice')).toBe('Sparky');

    bus.emit('profile-update', {
      profile: { user: 'alice', displayName: 'Alice B.', nickname: 'Sparky' }
    });
    expect(get(displayNames)('alice')).toBe('Sparky');
  });

  it('falls back again once the nickname is cleared', async () => {
    const { displayNames } = await load();
    bus.emit('profile-snapshot', {
      profiles: [{ user: 'alice', displayName: 'Alice A.', nickname: 'Sparky' }]
    });
    bus.emit('profile-update', {
      profile: { user: 'alice', displayName: 'Alice A.', nickname: '' }
    });
    expect(get(displayNames)('alice')).toBe('Alice A.');
  });
});

describe('parsing server frames', () => {
  it('clamps both names to the limits the server enforces', async () => {
    const { profiles } = await load();
    bus.emit('profile-update', {
      profile: {
        user: 'alice',
        displayName: 'd'.repeat(MAX_DISPLAY_NAME_LENGTH + 20),
        nickname: 'n'.repeat(MAX_NICKNAME_LENGTH + 20)
      }
    });
    const profile = get(profiles).alice;
    expect(profile.displayName).toHaveLength(MAX_DISPLAY_NAME_LENGTH);
    expect(profile.nickname).toHaveLength(MAX_NICKNAME_LENGTH);
  });

  it('treats a missing or non-string nickname as unset', async () => {
    const { profiles } = await load();
    bus.emit('profile-update', { profile: { user: 'alice', displayName: 'Alice A.' } });
    expect(get(profiles).alice.nickname).toBe('');
    bus.emit('profile-update', { profile: { user: 'bob', nickname: 42 } });
    expect(get(profiles).bob.nickname).toBe('');
  });

  it('drops entries without an account name', async () => {
    const { profiles } = await load();
    bus.emit('profile-snapshot', {
      profiles: [{ nickname: 'Nameless' }, { user: '', nickname: 'Empty' }, { user: 'alice' }]
    });
    expect(Object.keys(get(profiles))).toEqual(['alice']);
  });
});

describe('sending changes', () => {
  it('names the target so a moderator can relabel somebody else', async () => {
    const { profiles } = await load();
    profiles.setNickname('bob', 'Bobby');
    expect(bus.sent).toEqual([{ type: 'set-nickname', user: 'bob', nickname: 'Bobby' }]);
  });

  it('sends an empty nickname to clear one', async () => {
    const { profiles } = await load();
    profiles.setNickname('bob', '');
    expect(bus.sent[0]).toMatchObject({ nickname: '' });
  });

  it('keeps the profile edit separate from the nickname', async () => {
    const { profiles } = await load();
    profiles.saveSelf({ displayName: 'Alice A.', about: 'Builds things.' });
    // The two are authorized differently server-side, so they never travel in
    // one frame — a nickname edit must not be able to smuggle a profile edit.
    expect(bus.sent[0]).toEqual({
      type: 'set-profile',
      displayName: 'Alice A.',
      about: 'Builds things.'
    });
  });
});
