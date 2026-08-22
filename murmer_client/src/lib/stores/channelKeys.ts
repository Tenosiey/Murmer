/*
  Key material for end-to-end encrypted channels, and the policy that keeps it
  distributed.

  The server stores one wrap of each channel key per member and hands each
  client the wraps addressed to it (see `channel-crypto.ts` for the crypto and
  the threat model). Nothing else about key management is server-side, so this
  store is where the decisions live:

  - No key yet, and we can see the channel → we open epoch 1 and wrap it for
    every member.
  - A member is missing a wrap of the current epoch → we hand them one.
  - Somebody holds the current epoch who is no longer a member → we rotate to a
    new epoch and do not wrap it for them. This is what makes removing somebody
    from a private channel actually take effect.
  - We hold none of the epochs → the channel is locked for us until a member
    who does holds it out. Nothing to do but say so.

  Every member runs this, so any online member repairs the distribution; two
  doing it at once is a race the server settles (`channel-key-epoch-conflict`),
  after which the loser re-reads and re-decides. Keys are held in memory only:
  the wraps are the durable copy, and re-fetching them on connect is cheaper
  than another secret in `localStorage`.

  The one thing this refuses to do is wrap the channel key for a member whose
  identity key changed under the pin in `peerKeys.ts`. That is the point where
  a malicious server would substitute a key it controls, and handing it the
  channel key would hand it every message.
*/
import { get, writable } from 'svelte/store';
import { loadKeyPair } from '../keypair';
import { generateChannelKey, unwrapChannelKey, wrapChannelKey } from '../channel-crypto';
import { peerKeys } from './peerKeys';

/** One member of an encrypted channel, as the server reports the roster. */
export interface ChannelKeyMember {
  user: string;
  publicKey: string;
}

/** What this client knows about one encrypted channel's keys. */
export interface ChannelKeyState {
  /** Epoch → base64 channel key, for the epochs we can open. */
  keys: Record<number, string>;
  /** The channel's current epoch, or null when it has no key at all yet. */
  epoch: number | null;
  /** The channel has a current epoch that we hold no key for. */
  locked: boolean;
  /** Members whose identity key changed under the pin; never wrapped for. */
  untrusted: string[];
}

type ChannelKeyMap = Record<number, ChannelKeyState>;

/** Frame payload of `channel-keys`, before validation. */
interface RawChannelKeysFrame {
  channelId?: unknown;
  epoch?: unknown;
  keys?: unknown;
  holders?: unknown;
  members?: unknown;
}

const EMPTY: ChannelKeyState = { keys: {}, epoch: null, locked: false, untrusted: [] };

function parseMembers(value: unknown): ChannelKeyMember[] {
  if (!Array.isArray(value)) return [];
  const members: ChannelKeyMember[] = [];
  for (const raw of value) {
    if (!raw || typeof raw !== 'object') continue;
    const item = raw as Record<string, unknown>;
    if (typeof item.user !== 'string' || typeof item.publicKey !== 'string') continue;
    if (!item.user || !item.publicKey) continue;
    members.push({ user: item.user, publicKey: item.publicKey });
  }
  return members;
}

function parseStrings(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((v): v is string => typeof v === 'string') : [];
}

function createChannelKeyStore() {
  const { subscribe, update, set } = writable<ChannelKeyMap>({});
  /** Injected by the chat store; this module must not import it back. */
  let send: (data: unknown) => void = () => {};
  /**
   * Signatures of writes already sent, so a `channel-keys` frame that reports
   * the same situation twice does not resend the same wraps in a loop.
   */
  const sentWrites = new Set<string>();

  function stateOf(map: ChannelKeyMap, channelId: number): ChannelKeyState {
    return map[channelId] ?? EMPTY;
  }

  function put(channelId: number, epoch: number, entries: unknown[]): void {
    if (entries.length === 0) return;
    send({ type: 'put-channel-keys', channelId, epoch, entries });
  }

  /**
   * Wrap `channelKey` for each member, skipping anyone whose identity key does
   * not match its pin. Returns the entries to submit; a member left out simply
   * stays without a key until the user resolves the conflict.
   */
  function wrapFor(
    channelKey: string,
    members: ChannelKeyMember[],
    secretKey: string
  ): { entries: unknown[]; untrusted: string[] } {
    const entries: unknown[] = [];
    const untrusted: string[] = [];
    for (const member of members) {
      if (!peerKeys.observe(member.user, member.publicKey)) {
        untrusted.push(member.user);
        continue;
      }
      const wrapped = wrapChannelKey(channelKey, member.publicKey, secretKey);
      if (!wrapped) {
        untrusted.push(member.user);
        continue;
      }
      entries.push({
        recipientKey: member.publicKey,
        nonce: wrapped.nonce,
        wrappedKey: wrapped.wrappedKey
      });
    }
    return { entries, untrusted };
  }

  /** Send a write once per distinct situation. */
  function putOnce(channelId: number, epoch: number, entries: unknown[]): void {
    if (entries.length === 0) return;
    const recipients = entries
      .map((e) => (e as { recipientKey: string }).recipientKey)
      .sort()
      .join(',');
    const signature = `${channelId}:${epoch}:${recipients}`;
    if (sentWrites.has(signature)) return;
    sentWrites.add(signature);
    put(channelId, epoch, entries);
  }

  return {
    subscribe,

    /** Hand the store its way to talk to the server. Called by the chat store. */
    setTransport(fn: (data: unknown) => void) {
      send = fn;
    },

    /** Ask the server for our wraps and the channel's roster. */
    request(channelId: number) {
      send({ type: 'get-channel-keys', channelId });
    },

    /**
     * Reconcile against the set of channels that are currently encrypted.
     *
     * Every encrypted channel is re-read, not just the new ones: the server
     * re-sends the channel list whenever channel permissions change, and that
     * is exactly the moment a member was added or removed and the key needs
     * handing out or rotating. Channels that stopped being encrypted, or that
     * we lost sight of, drop their keys — holding on to a key for a channel we
     * are no longer in is what rotation exists to prevent.
     */
    syncChannels(encryptedChannelIds: number[]) {
      const wanted = new Set(encryptedChannelIds);
      update((map) => {
        const next = { ...map };
        for (const id of Object.keys(next).map(Number)) {
          if (!wanted.has(id)) delete next[id];
        }
        return next;
      });
      for (const id of wanted) this.request(id);
    },

    /**
     * Absorb a `channel-keys` frame: open every wrap addressed to us, then act
     * on whatever the roster and the current epoch call for.
     */
    receive(frame: RawChannelKeysFrame) {
      const channelId = typeof frame.channelId === 'number' ? frame.channelId : null;
      if (channelId === null) return;
      const epoch =
        typeof frame.epoch === 'number' && Number.isInteger(frame.epoch) && frame.epoch >= 1
          ? frame.epoch
          : null;
      const members = parseMembers(frame.members);
      const holders = new Set(parseStrings(frame.holders));
      const pair = loadKeyPair();

      const keys: Record<number, string> = {};
      if (Array.isArray(frame.keys)) {
        for (const raw of frame.keys) {
          if (!raw || typeof raw !== 'object') continue;
          const item = raw as Record<string, unknown>;
          if (
            typeof item.epoch !== 'number' ||
            typeof item.senderKey !== 'string' ||
            typeof item.nonce !== 'string' ||
            typeof item.wrappedKey !== 'string'
          ) {
            continue;
          }
          const key = unwrapChannelKey(
            item.nonce,
            item.wrappedKey,
            item.senderKey,
            pair.secretKey
          );
          if (key) keys[item.epoch] = key;
        }
      }

      const current = epoch === null ? null : (keys[epoch] ?? null);
      let untrusted: string[] = [];

      if (epoch === null && members.length > 0) {
        // Nobody has opened this channel yet. Whoever gets here first does.
        const fresh = generateChannelKey();
        const wrapped = wrapFor(fresh, members, pair.secretKey);
        untrusted = wrapped.untrusted;
        putOnce(channelId, 1, wrapped.entries);
      } else if (epoch !== null && current) {
        const memberKeys = new Set(members.map((m) => m.publicKey));
        const departed = [...holders].filter((key) => !memberKeys.has(key));
        if (departed.length > 0) {
          // Somebody lost access. A new epoch they are not wrapped for is the
          // only thing that actually takes the channel away from them.
          const fresh = generateChannelKey();
          const wrapped = wrapFor(fresh, members, pair.secretKey);
          untrusted = wrapped.untrusted;
          putOnce(channelId, epoch + 1, wrapped.entries);
        } else {
          const missing = members.filter((m) => !holders.has(m.publicKey));
          if (missing.length > 0) {
            const wrapped = wrapFor(current, missing, pair.secretKey);
            untrusted = wrapped.untrusted;
            putOnce(channelId, epoch, wrapped.entries);
          }
        }
      }

      update((map) => ({
        ...map,
        [channelId]: {
          keys,
          epoch,
          locked: epoch !== null && !current,
          untrusted
        }
      }));
    },

    /** The key a message of `epoch` was sealed with, if we hold it. */
    keyFor(channelId: number, epoch: number): string | null {
      return stateOf(get({ subscribe }), channelId).keys[epoch] ?? null;
    },

    /** The epoch new messages must be sealed under, if we can send at all. */
    currentKey(channelId: number): { epoch: number; key: string } | null {
      const state = stateOf(get({ subscribe }), channelId);
      if (state.epoch === null) return null;
      const key = state.keys[state.epoch];
      return key ? { epoch: state.epoch, key } : null;
    },

    /** Whether the channel has a key we cannot open (nothing to render with). */
    isLocked(channelId: number): boolean {
      return stateOf(get({ subscribe }), channelId).locked;
    },

    /** Drop everything, e.g. when leaving a server. */
    reset() {
      sentWrites.clear();
      set({});
    }
  };
}

export const channelKeys = createChannelKeyStore();
