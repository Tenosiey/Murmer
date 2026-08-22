/*
  The distribution policy in `channelKeys.ts` is the part of channel encryption
  whose failures are invisible in the UI: a removed member who keeps reading
  looks exactly like a working channel, and so does a key handed to a
  substituted identity key. These tests drive the store with the frames a
  server would send and assert on what it writes back.
*/
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import nacl from 'tweetnacl';
import { toBase64 } from '../keypair';
import { unwrapChannelKey, wrapChannelKey } from '../channel-crypto';

const SERVER = 'wss://one.example/ws';
const CH = 7;

function identity() {
  const pair = nacl.sign.keyPair();
  return { publicKey: toBase64(pair.publicKey), secretKey: toBase64(pair.secretKey) };
}

/** Us. Seeded into storage so `loadKeyPair()` returns this identity. */
const me = identity();
const bob = identity();
const carol = identity();

interface PutFrame {
  type: string;
  channelId: number;
  epoch: number;
  entries: { recipientKey: string; nonce: string; wrappedKey: string }[];
}

/** Fresh store instances plus the frames they send. */
async function loadStore() {
  vi.resetModules();
  localStorage.setItem('murmer_keypair', JSON.stringify(me));
  const { channelKeys } = await import('./channelKeys');
  const { peerKeys } = await import('./peerKeys');
  peerKeys.setServer(SERVER);
  const sent: Record<string, unknown>[] = [];
  channelKeys.setTransport((data) => sent.push(data as Record<string, unknown>));
  return { channelKeys, peerKeys, sent };
}

function puts(sent: Record<string, unknown>[]): PutFrame[] {
  return sent.filter((f) => f.type === 'put-channel-keys') as unknown as PutFrame[];
}

/** A `channel-keys` wrap entry addressed to us, as the server would send it. */
function wrapForMe(channelKey: string, epoch: number, from = bob) {
  const wrapped = wrapChannelKey(channelKey, me.publicKey, from.secretKey)!;
  return { epoch, senderKey: from.publicKey, nonce: wrapped.nonce, wrappedKey: wrapped.wrappedKey };
}

const roster = [
  { user: 'me', publicKey: me.publicKey },
  { user: 'bob', publicKey: bob.publicKey }
];

beforeEach(() => {
  localStorage.clear();
});

describe('channelKeys — opening a channel', () => {
  it('opens epoch 1 and wraps it for every member when nobody has', async () => {
    const { channelKeys, sent } = await loadStore();
    channelKeys.receive({ channelId: CH, epoch: null, keys: [], holders: [], members: roster });

    const [write] = puts(sent);
    expect(write.epoch).toBe(1);
    expect(write.entries.map((e) => e.recipientKey).sort()).toEqual(
      [me.publicKey, bob.publicKey].sort()
    );
    // The key we generated has to be one we can open ourselves, or we would
    // lock ourselves out of the channel we just created.
    const mine = write.entries.find((e) => e.recipientKey === me.publicKey)!;
    expect(unwrapChannelKey(mine.nonce, mine.wrappedKey, me.publicKey, me.secretKey)).not.toBeNull();
  });

  it('does not open a channel with an empty roster', async () => {
    const { channelKeys, sent } = await loadStore();
    channelKeys.receive({ channelId: CH, epoch: null, keys: [], holders: [], members: [] });
    expect(puts(sent)).toHaveLength(0);
  });
});

describe('channelKeys — reading', () => {
  it('opens the wraps addressed to us and offers them by epoch', async () => {
    const { channelKeys } = await loadStore();
    const older = 'AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=';
    const current = 'BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBA=';
    channelKeys.receive({
      channelId: CH,
      epoch: 2,
      keys: [wrapForMe(older, 1), wrapForMe(current, 2)],
      holders: [me.publicKey, bob.publicKey],
      members: roster
    });

    expect(channelKeys.keyFor(CH, 1)).toBe(older);
    expect(channelKeys.keyFor(CH, 2)).toBe(current);
    expect(channelKeys.currentKey(CH)).toEqual({ epoch: 2, key: current });
    expect(channelKeys.isLocked(CH)).toBe(false);
  });

  it('reports a channel we hold no key for as locked, and sends nothing', async () => {
    const { channelKeys, sent } = await loadStore();
    channelKeys.receive({
      channelId: CH,
      epoch: 1,
      keys: [],
      holders: [bob.publicKey],
      members: roster
    });

    expect(channelKeys.isLocked(CH)).toBe(true);
    expect(channelKeys.currentKey(CH)).toBeNull();
    // We cannot hand out a key we do not have; the only correct move is to
    // wait for a member who does.
    expect(puts(sent)).toHaveLength(0);
  });

  it('drops a wrap whose sender key does not authenticate it', async () => {
    const { channelKeys } = await loadStore();
    const key = 'BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBA=';
    const entry = wrapForMe(key, 1);
    channelKeys.receive({
      channelId: CH,
      epoch: 1,
      keys: [{ ...entry, senderKey: carol.publicKey }],
      holders: [me.publicKey, bob.publicKey],
      members: roster
    });

    expect(channelKeys.keyFor(CH, 1)).toBeNull();
    expect(channelKeys.isLocked(CH)).toBe(true);
  });
});

describe('channelKeys — distribution', () => {
  const key = 'BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBA=';

  it('hands the current key to a member who is missing it', async () => {
    const { channelKeys, sent } = await loadStore();
    channelKeys.receive({
      channelId: CH,
      epoch: 1,
      keys: [wrapForMe(key, 1)],
      holders: [me.publicKey, bob.publicKey],
      members: [...roster, { user: 'carol', publicKey: carol.publicKey }]
    });

    const [write] = puts(sent);
    expect(write.epoch).toBe(1);
    expect(write.entries.map((e) => e.recipientKey)).toEqual([carol.publicKey]);
    // It must be the *existing* key, or Carol would be unable to read anything
    // the others already sent under it.
    expect(
      unwrapChannelKey(write.entries[0].nonce, write.entries[0].wrappedKey, me.publicKey, carol.secretKey)
    ).toBe(key);
  });

  it('rotates to a new epoch when a holder is no longer a member', async () => {
    const { channelKeys, sent } = await loadStore();
    channelKeys.receive({
      channelId: CH,
      epoch: 1,
      keys: [wrapForMe(key, 1)],
      // Carol still holds epoch 1 but has been removed from the channel.
      holders: [me.publicKey, bob.publicKey, carol.publicKey],
      members: roster
    });

    const [write] = puts(sent);
    expect(write.epoch).toBe(2);
    expect(write.entries.map((e) => e.recipientKey).sort()).toEqual(
      [me.publicKey, bob.publicKey].sort()
    );
    // A rotation that reused the key would leave Carol reading everything sent
    // after her removal — the whole point of the new epoch.
    const mine = write.entries.find((e) => e.recipientKey === me.publicKey)!;
    expect(unwrapChannelKey(mine.nonce, mine.wrappedKey, me.publicKey, me.secretKey)).not.toBe(key);
  });

  it('sends nothing when every member already holds the current key', async () => {
    const { channelKeys, sent } = await loadStore();
    channelKeys.receive({
      channelId: CH,
      epoch: 1,
      keys: [wrapForMe(key, 1)],
      holders: [me.publicKey, bob.publicKey],
      members: roster
    });
    expect(puts(sent)).toHaveLength(0);
  });

  it('does not resend the same wraps when the server repeats a frame', async () => {
    const { channelKeys, sent } = await loadStore();
    const frame = {
      channelId: CH,
      epoch: 1,
      keys: [wrapForMe(key, 1)],
      holders: [me.publicKey],
      members: roster
    };
    channelKeys.receive(frame);
    channelKeys.receive(frame);
    channelKeys.receive(frame);
    // Every member runs this policy, so a frame that reports an unchanged
    // situation must not turn into a write loop between them.
    expect(puts(sent)).toHaveLength(1);
  });

  it('refuses to wrap the key for a member whose identity key changed', async () => {
    const { channelKeys, peerKeys, sent } = await loadStore();
    // Bob was pinned under one key; the server now claims another for him.
    peerKeys.observe('bob', identity().publicKey);

    channelKeys.receive({
      channelId: CH,
      epoch: 1,
      keys: [wrapForMe(key, 1)],
      holders: [me.publicKey],
      members: roster
    });

    const writes = puts(sent);
    // Handing the channel key to a substituted key is exactly how a malicious
    // server would read the channel, so Bob is left out until the user
    // resolves the conflict.
    expect(writes.flatMap((w) => w.entries).map((e) => e.recipientKey)).not.toContain(
      bob.publicKey
    );
    expect(get(channelKeys)[CH].untrusted).toEqual(['bob']);
  });
});

describe('channelKeys — lifecycle', () => {
  it('re-reads every encrypted channel and forgets the others', async () => {
    const { channelKeys, sent } = await loadStore();
    channelKeys.syncChannels([CH, 9]);
    expect(sent.map((f) => f.channelId)).toEqual([CH, 9]);
    expect(sent.every((f) => f.type === 'get-channel-keys')).toBe(true);

    channelKeys.receive({
      channelId: CH,
      epoch: 1,
      keys: [wrapForMe('BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBA=', 1)],
      holders: [me.publicKey],
      members: [{ user: 'me', publicKey: me.publicKey }]
    });
    expect(channelKeys.keyFor(CH, 1)).not.toBeNull();

    // Encryption switched off (or we lost access): the key must not linger.
    channelKeys.syncChannels([9]);
    expect(channelKeys.keyFor(CH, 1)).toBeNull();
    expect(get(channelKeys)[CH]).toBeUndefined();
  });

  it('re-reads a channel it already tracks, because that is when membership changed', async () => {
    const { channelKeys, sent } = await loadStore();
    channelKeys.receive({
      channelId: CH,
      epoch: 1,
      keys: [wrapForMe('BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBA=', 1)],
      holders: [me.publicKey],
      members: [{ user: 'me', publicKey: me.publicKey }]
    });
    sent.length = 0;

    // The server re-sends the channel list on every permission change; only a
    // re-read tells us somebody was added or removed.
    channelKeys.syncChannels([CH]);
    expect(sent).toEqual([{ type: 'get-channel-keys', channelId: CH }]);
    expect(channelKeys.keyFor(CH, 1)).not.toBeNull();
  });

  it('drops everything on reset, so keys never cross servers', async () => {
    const { channelKeys } = await loadStore();
    channelKeys.receive({
      channelId: CH,
      epoch: 1,
      keys: [wrapForMe('BBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBBA=', 1)],
      holders: [me.publicKey],
      members: [{ user: 'me', publicKey: me.publicKey }]
    });
    channelKeys.reset();
    expect(get(channelKeys)).toEqual({});
    expect(channelKeys.currentKey(CH)).toBeNull();
  });
});
