import { beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import { MAX_MESSAGE_LENGTH } from '../chat/constants';

/**
 * Stand-in for the chat store, as in the other store tests: records outgoing
 * frames and lets a test push incoming ones into the handlers the stores
 * register at import time.
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

/** The stores wire up their handlers on import, so each test gets a fresh one. */
async function load() {
  bus.reset();
  vi.resetModules();
  const [settings, bansModule, storage, { connection }] = await Promise.all([
    import('./chatSettings'),
    import('./bans'),
    import('./storageUsage'),
    import('./connection')
  ]);
  connection.set('connected');
  return { ...settings, ...bansModule, ...storage, connection };
}

beforeEach(() => {
  bus.reset();
});

describe('chatSettings — parsing the server frame', () => {
  it('starts permissive until the server says otherwise', async () => {
    const { chatSettings } = await load();

    expect(get(chatSettings)).toEqual({
      slowModeSeconds: 0,
      maxMessageLength: MAX_MESSAGE_LENGTH,
      profanityFilter: false,
      words: null
    });
  });

  it('takes the announced policy', async () => {
    const { chatSettings } = await load();

    bus.emit('chat-settings', {
      slowModeSeconds: 15,
      maxMessageLength: 500,
      profanityFilter: true
    });

    expect(get(chatSettings)).toMatchObject({
      slowModeSeconds: 15,
      maxMessageLength: 500,
      profanityFilter: true
    });
  });

  it('never lets a frame raise the message cap above what the build allows', async () => {
    const { chatSettings } = await load();

    bus.emit('chat-settings', {
      slowModeSeconds: 0,
      maxMessageLength: MAX_MESSAGE_LENGTH * 10,
      profanityFilter: false
    });

    expect(get(chatSettings).maxMessageLength).toBe(MAX_MESSAGE_LENGTH);

    // Nonsense falls back to the cap rather than to zero, which would leave
    // the composer refusing every keystroke.
    bus.emit('chat-settings', {
      slowModeSeconds: -5,
      maxMessageLength: 'lots',
      profanityFilter: 'yes'
    });
    expect(get(chatSettings)).toMatchObject({
      slowModeSeconds: 0,
      maxMessageLength: MAX_MESSAGE_LENGTH,
      profanityFilter: false
    });
  });

  it('keeps the word list a broadcast does not carry', async () => {
    const { chatSettings, requestChatSettings } = await load();

    // Only the manager-only answer carries `words`.
    requestChatSettings();
    expect(bus.sent).toEqual([{ type: 'get-chat-settings' }]);
    bus.emit('chat-settings', {
      slowModeSeconds: 0,
      maxMessageLength: 4000,
      profanityFilter: true,
      words: ['damn', 42, 'heck']
    });
    expect(get(chatSettings).words).toEqual(['damn', 'heck']);

    // A later broadcast (someone else changed slow mode) must not wipe the
    // list the editor is holding.
    bus.emit('chat-settings', {
      slowModeSeconds: 30,
      maxMessageLength: 4000,
      profanityFilter: true
    });
    expect(get(chatSettings)).toMatchObject({ slowModeSeconds: 30, words: ['damn', 'heck'] });
  });

  it('forgets the policy when the connection drops', async () => {
    const { chatSettings, connection } = await load();

    bus.emit('chat-settings', {
      slowModeSeconds: 30,
      maxMessageLength: 100,
      profanityFilter: true,
      words: ['damn']
    });
    connection.set('idle');

    expect(get(chatSettings)).toEqual({
      slowModeSeconds: 0,
      maxMessageLength: MAX_MESSAGE_LENGTH,
      profanityFilter: false,
      words: null
    });
  });

  it('counts down the remaining slow mode interval', async () => {
    const { slowModeRemaining } = await load();

    // Nothing to wait for while slow mode is off, whatever was sent when.
    expect(slowModeRemaining(1000, 1000)).toBe(0);

    bus.emit('chat-settings', {
      slowModeSeconds: 10,
      maxMessageLength: 4000,
      profanityFilter: false
    });

    expect(slowModeRemaining(null, 60_000)).toBe(0);
    expect(slowModeRemaining(60_000, 63_000)).toBe(7);
    expect(slowModeRemaining(60_000, 70_000)).toBe(0);
  });

  it('starts the countdown when a message is accepted, not when it is typed', async () => {
    const { slowModeWait } = await load();
    const { session } = await import('./session');
    session.set({ user: 'alice' });

    bus.emit('chat-settings', {
      slowModeSeconds: 10,
      maxMessageLength: 4000,
      profanityFilter: false
    });

    // Nothing sent yet: no wait, even though slow mode is on.
    expect(slowModeWait()).toBe(0);

    // Somebody else's message is not ours, so it starts no countdown.
    bus.emit('chat', { user: 'bob', text: 'hi' });
    expect(slowModeWait()).toBe(0);

    // Our own message coming back from the server is what the server's own
    // timer is keyed on — a send it refused never gets here.
    const sentAt = Date.now();
    bus.emit('chat', { user: 'alice', text: 'hello' });
    expect(slowModeWait(sentAt + 3_000)).toBe(7);
    expect(slowModeWait(sentAt + 10_000)).toBe(0);
  });
});

describe('bans — the dashboard ban list', () => {
  it('is unknown until the server answers', async () => {
    const { bans } = await load();

    expect(get(bans)).toBeNull();
    bans.refresh();
    expect(bus.sent).toEqual([{ type: 'get-ban-list' }]);
  });

  it('parses the list and drops rows without a name', async () => {
    const { bans } = await load();

    bus.emit('ban-list', {
      bans: [
        {
          user: 'spammer',
          publicKey: 'key-a',
          bannedBy: 'mod',
          bannedAt: '2026-08-01T10:00:00Z'
        },
        { publicKey: 'key-b' },
        { user: 'raider' }
      ]
    });

    expect(get(bans)).toEqual([
      {
        user: 'spammer',
        publicKey: 'key-a',
        bannedBy: 'mod',
        bannedAt: '2026-08-01T10:00:00Z'
      },
      { user: 'raider', publicKey: '', bannedBy: '', bannedAt: null }
    ]);
  });

  it('re-asks the server whenever a ban is issued or lifted elsewhere', async () => {
    const { bans } = await load();

    bus.emit('user-unbanned', { user: 'spammer' });
    bus.emit('force-disconnect', { user: 'raider', action: 'banned' });
    // A kick is not a ban and must not cost a round trip.
    bus.emit('force-disconnect', { user: 'someone', action: 'kicked' });

    expect(bus.sent.filter((frame) => frame.type === 'get-ban-list')).toHaveLength(2);
  });

  it('forgets the list when the connection drops', async () => {
    const { bans, connection } = await load();

    bus.emit('ban-list', { bans: [{ user: 'spammer' }] });
    connection.set('idle');

    expect(get(bans)).toBeNull();
  });
});

describe('storageUsage — the upload directory report', () => {
  it('sorts the breakdown by size and tolerates a sparse frame', async () => {
    const { storageUsage } = await load();

    expect(get(storageUsage)).toBeNull();
    storageUsage.refresh();
    expect(bus.sent).toEqual([{ type: 'get-storage-usage' }]);

    bus.emit('storage-usage', {
      totalBytes: 3_000,
      fileCount: 4,
      categories: {
        images: { bytes: 1_000, files: 2 },
        audio: { bytes: 2_000, files: 1 },
        other: { files: 'lots' }
      }
    });

    expect(get(storageUsage)).toEqual({
      totalBytes: 3_000,
      fileCount: 4,
      categories: [
        { id: 'audio', bytes: 2_000, files: 1 },
        { id: 'images', bytes: 1_000, files: 2 },
        { id: 'other', bytes: 0, files: 0 }
      ]
    });
  });

  it('renders byte counts at the unit people read them in', async () => {
    const { formatBytes } = await load();

    expect(formatBytes(512)).toBe('512 B');
    expect(formatBytes(2048)).toBe('2 KB');
    expect(formatBytes(5 * 1024 * 1024)).toBe('5.0 MB');
    expect(formatBytes(3 * 1024 ** 3)).toBe('3.0 GB');
  });
});
