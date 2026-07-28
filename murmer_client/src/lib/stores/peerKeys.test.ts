import { describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';

const STORAGE_KEY = 'murmer_peer_keys';

const SERVER_A = 'wss://one.example/ws';
const SERVER_B = 'wss://two.example/ws';
const KEY_ONE = 'AAAAKEYONE=';
const KEY_TWO = 'BBBBKEYTWO=';

/** Fresh module instance; the store reads `localStorage` on every call. */
async function loadPeerKeys() {
  vi.resetModules();
  return (await import('./peerKeys')).peerKeys;
}

function persisted(): Record<string, Record<string, string>> {
  const raw = localStorage.getItem(STORAGE_KEY);
  return raw ? JSON.parse(raw) : {};
}

describe('peerKeys — trust on first use', () => {
  it('pins the first key it sees for a peer', async () => {
    const peerKeys = await loadPeerKeys();
    peerKeys.setServer(SERVER_A);

    expect(peerKeys.pinned('bob')).toBeNull();
    expect(peerKeys.observe('bob', KEY_ONE)).toBe(true);
    expect(peerKeys.pinned('bob')).toBe(KEY_ONE);
    expect(peerKeys.hasConflict('bob')).toBe(false);
    expect(persisted()).toEqual({ [SERVER_A]: { bob: KEY_ONE } });
  });

  it('accepts the pinned key on every later sighting', async () => {
    const peerKeys = await loadPeerKeys();
    peerKeys.setServer(SERVER_A);
    peerKeys.observe('bob', KEY_ONE);

    expect(peerKeys.observe('bob', KEY_ONE)).toBe(true);
    expect(peerKeys.observe('bob', KEY_ONE)).toBe(true);
    expect(peerKeys.hasConflict('bob')).toBe(false);
  });

  it('rejects a changed key and never overwrites the pin', async () => {
    // This is the whole point of the store: a server that swaps a peer's key
    // must not be able to redirect the next DM to a key the user never saw.
    const peerKeys = await loadPeerKeys();
    peerKeys.setServer(SERVER_A);
    peerKeys.observe('bob', KEY_ONE);

    expect(peerKeys.observe('bob', KEY_TWO)).toBe(false);
    expect(peerKeys.pinned('bob')).toBe(KEY_ONE);
    expect(persisted()[SERVER_A].bob).toBe(KEY_ONE);
    expect(peerKeys.hasConflict('bob')).toBe(true);
    expect(get(peerKeys.conflicts)).toEqual({ bob: KEY_TWO });
  });

  it('keeps rejecting the new key until the user trusts it', async () => {
    const peerKeys = await loadPeerKeys();
    peerKeys.setServer(SERVER_A);
    peerKeys.observe('bob', KEY_ONE);

    expect(peerKeys.observe('bob', KEY_TWO)).toBe(false);
    expect(peerKeys.observe('bob', KEY_TWO)).toBe(false);
    expect(peerKeys.pinned('bob')).toBe(KEY_ONE);
  });

  it('re-detects a conflict after a restart, since only the pin persists', async () => {
    let peerKeys = await loadPeerKeys();
    peerKeys.setServer(SERVER_A);
    peerKeys.observe('bob', KEY_ONE);
    peerKeys.observe('bob', KEY_TWO);

    // Conflicts are in-memory; the pin outlives the process.
    peerKeys = await loadPeerKeys();
    peerKeys.setServer(SERVER_A);
    expect(peerKeys.hasConflict('bob')).toBe(false);
    expect(peerKeys.observe('bob', KEY_TWO)).toBe(false);
    expect(peerKeys.pinned('bob')).toBe(KEY_ONE);
  });

  it('tracks conflicts for several peers independently', async () => {
    const peerKeys = await loadPeerKeys();
    peerKeys.setServer(SERVER_A);
    peerKeys.observe('bob', KEY_ONE);
    peerKeys.observe('carol', KEY_ONE);

    peerKeys.observe('bob', KEY_TWO);
    expect(peerKeys.hasConflict('bob')).toBe(true);
    expect(peerKeys.hasConflict('carol')).toBe(false);
    expect(peerKeys.observe('carol', KEY_ONE)).toBe(true);
  });
});

describe('peerKeys — trusting a changed key', () => {
  it('adopts the new key and clears the conflict', async () => {
    const peerKeys = await loadPeerKeys();
    peerKeys.setServer(SERVER_A);
    peerKeys.observe('bob', KEY_ONE);
    peerKeys.observe('bob', KEY_TWO);

    peerKeys.trust('bob');
    expect(peerKeys.pinned('bob')).toBe(KEY_TWO);
    expect(peerKeys.hasConflict('bob')).toBe(false);
    expect(get(peerKeys.conflicts)).toEqual({});
    expect(peerKeys.observe('bob', KEY_TWO)).toBe(true);
    // The old key is now the stranger.
    expect(peerKeys.observe('bob', KEY_ONE)).toBe(false);
  });

  it('does nothing when there is no conflict to trust', async () => {
    const peerKeys = await loadPeerKeys();
    peerKeys.setServer(SERVER_A);
    peerKeys.observe('bob', KEY_ONE);

    peerKeys.trust('bob');
    peerKeys.trust('nobody');
    expect(peerKeys.pinned('bob')).toBe(KEY_ONE);
    expect(peerKeys.pinned('nobody')).toBeNull();
  });

  it('clears the key back to trusted when the server reverts on its own', async () => {
    const peerKeys = await loadPeerKeys();
    peerKeys.setServer(SERVER_A);
    peerKeys.observe('bob', KEY_ONE);
    peerKeys.observe('bob', KEY_TWO);
    expect(peerKeys.hasConflict('bob')).toBe(true);

    expect(peerKeys.observe('bob', KEY_ONE)).toBe(true);
    expect(peerKeys.hasConflict('bob')).toBe(false);
  });
});

describe('peerKeys — per-server namespacing', () => {
  it('does not carry a pin from one server to another', async () => {
    // Account names are only unique per server, so `bob` on one server is an
    // unrelated person on the next.
    const peerKeys = await loadPeerKeys();
    peerKeys.setServer(SERVER_A);
    peerKeys.observe('bob', KEY_ONE);

    peerKeys.setServer(SERVER_B);
    expect(peerKeys.pinned('bob')).toBeNull();
    expect(peerKeys.observe('bob', KEY_TWO)).toBe(true);

    peerKeys.setServer(SERVER_A);
    expect(peerKeys.pinned('bob')).toBe(KEY_ONE);
    expect(peerKeys.observe('bob', KEY_TWO)).toBe(false);

    expect(persisted()).toEqual({
      [SERVER_A]: { bob: KEY_ONE },
      [SERVER_B]: { bob: KEY_TWO }
    });
  });

  it('drops pending conflicts when switching servers', async () => {
    const peerKeys = await loadPeerKeys();
    peerKeys.setServer(SERVER_A);
    peerKeys.observe('bob', KEY_ONE);
    peerKeys.observe('bob', KEY_TWO);

    peerKeys.setServer(SERVER_B);
    expect(get(peerKeys.conflicts)).toEqual({});
    expect(peerKeys.hasConflict('bob')).toBe(false);
  });
});

describe('peerKeys — persistence', () => {
  it('reads pins written by an earlier session', async () => {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({ [SERVER_A]: { bob: KEY_ONE }, [SERVER_B]: { bob: KEY_TWO } })
    );
    const peerKeys = await loadPeerKeys();

    peerKeys.setServer(SERVER_A);
    expect(peerKeys.pinned('bob')).toBe(KEY_ONE);
    expect(peerKeys.observe('bob', KEY_TWO)).toBe(false);

    peerKeys.setServer(SERVER_B);
    expect(peerKeys.pinned('bob')).toBe(KEY_TWO);
  });

  it('starts from empty when the stored payload is corrupt', async () => {
    // Falling back to "no pins" means the next key is pinned fresh, which is
    // the same exposure as first contact — not a silent accept of any key.
    localStorage.setItem(STORAGE_KEY, 'not json');
    const peerKeys = await loadPeerKeys();
    peerKeys.setServer(SERVER_A);

    expect(peerKeys.pinned('bob')).toBeNull();
    expect(peerKeys.observe('bob', KEY_ONE)).toBe(true);
    expect(peerKeys.observe('bob', KEY_TWO)).toBe(false);
  });

  it('keeps other servers intact when pinning a new peer', async () => {
    localStorage.setItem(STORAGE_KEY, JSON.stringify({ [SERVER_B]: { dave: KEY_TWO } }));
    const peerKeys = await loadPeerKeys();
    peerKeys.setServer(SERVER_A);
    peerKeys.observe('bob', KEY_ONE);

    expect(persisted()).toEqual({
      [SERVER_A]: { bob: KEY_ONE },
      [SERVER_B]: { dave: KEY_TWO }
    });
  });
});
