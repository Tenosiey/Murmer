/*
  Trust-on-first-use pinning of peers' identity keys for DM encryption.

  The server is the key directory, so a malicious server could hand out a
  substituted key. To limit that to first contact, the first key seen for a
  peer is pinned (persisted per server URL — names are only unique per
  server). When the server later reports a different key, the conversation
  shows a warning and sending is blocked until the user explicitly trusts
  the new key, e.g. after comparing fingerprints out of band.
*/
import { get, writable } from 'svelte/store';
import { browser } from '$app/environment';

const STORAGE_KEY = 'murmer_peer_keys';

/** server URL -> peer name -> pinned public key (base64 Ed25519) */
type PinnedKeys = Record<string, Record<string, string>>;

function loadAll(): PinnedKeys {
  if (!browser) return {};
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? (JSON.parse(raw) as PinnedKeys) : {};
  } catch {
    return {};
  }
}

function saveAll(all: PinnedKeys) {
  if (!browser) return;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(all));
}

function createPeerKeysStore() {
  let serverUrl = '';
  /** Keys reported by the server that differ from the pin, awaiting the
   *  user's explicit trust decision. In-memory only: a restart re-detects
   *  the conflict from the (unchanged) pin. peer name -> new key */
  const conflicts = writable<Record<string, string>>({});

  function pinnedFor(peer: string): string | null {
    return loadAll()[serverUrl]?.[peer] ?? null;
  }

  function pin(peer: string, key: string) {
    const all = loadAll();
    all[serverUrl] = { ...(all[serverUrl] ?? {}), [peer]: key };
    saveAll(all);
  }

  return {
    /** Unconfirmed key changes for the current server; keyed by peer name. */
    conflicts: { subscribe: conflicts.subscribe },

    /** Switch to a server's slice of pins. Call when connecting. */
    setServer(url: string) {
      serverUrl = url;
      conflicts.set({});
    },

    pinned(peer: string): string | null {
      return pinnedFor(peer);
    },

    /**
     * Record a key reported by the server. Pins it on first sight; flags a
     * conflict when it differs from the pin. Returns true when the key is
     * trusted (first sight or matching the pin).
     */
    observe(peer: string, key: string): boolean {
      const pinned = pinnedFor(peer);
      if (pinned === null) {
        pin(peer, key);
        return true;
      }
      if (pinned === key) {
        conflicts.update((current) => {
          if (!(peer in current)) return current;
          const next = { ...current };
          delete next[peer];
          return next;
        });
        return true;
      }
      conflicts.update((current) => ({ ...current, [peer]: key }));
      return false;
    },

    hasConflict(peer: string): boolean {
      return peer in get(conflicts);
    },

    /** Accept a peer's changed key after the user confirmed it. */
    trust(peer: string) {
      const key = get(conflicts)[peer];
      if (!key) return;
      pin(peer, key);
      conflicts.update((current) => {
        const next = { ...current };
        delete next[peer];
        return next;
      });
    }
  };
}

export const peerKeys = createPeerKeysStore();
