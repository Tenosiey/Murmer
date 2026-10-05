/**
 * The mentions inbox: recent messages that mention the signed-in user, across
 * every channel. A per-channel badge says *where*; this says *what*.
 *
 * Two sources merge here. The server's answer to `load-mentions` covers what
 * arrived while the user was away; mentions that arrive live are added as
 * they come. Live ones matter beyond freshness: in an encrypted channel the
 * server holds no text, so it can only find role pings there, and a name
 * mention in one reaches the inbox solely from the client's own copy.
 *
 * The chat store feeds this rather than it subscribing to frames itself — it
 * owns decryption and already decides what a mention is. Like the drafts,
 * the inbox lives in memory only: it holds the plaintext of encrypted
 * channels, which must never land on disk. It is reset on every connect and
 * disconnect, so one server's mentions never appear on the next.
 */
import { writable } from 'svelte/store';

/** How many entries the inbox keeps; the server answers with as many. */
export const MAX_INBOX_ENTRIES = 50;

export interface InboxEntry {
  id: number;
  channelId: number;
  /** Account name of the author. */
  user: string;
  /** The message text, or null when there is none to show (sealed, file). */
  text: string | null;
  timestamp: string | null;
  /**
   * Found by this client alone: a name mention in an encrypted channel, or
   * an `@here`. The server's inbox cannot list these, so its answer must not
   * drop them — everything else it lists or not is the truth, a deletion
   * included.
   */
  liveOnly: boolean;
}

export interface InboxState {
  entries: InboxEntry[];
  /** Mentions from other channels since the inbox was last opened. */
  unseen: number;
  /** A `load-mentions` request is in flight. */
  loading: boolean;
}

/**
 * Merge entries into the inbox, newest (highest id) first, one entry per
 * message. An incoming copy replaces a held one — the server's is the stored
 * message, a live one may predate an edit — except that a copy without text
 * never replaces one with text: the server cannot read a sealed message, and
 * the live copy the client opened is the only readable one there is.
 */
export function mergeInbox(
  held: InboxEntry[],
  incoming: InboxEntry[],
  cap = MAX_INBOX_ENTRIES
): InboxEntry[] {
  const byId = new Map(held.map((entry) => [entry.id, entry]));
  for (const entry of incoming) {
    const existing = byId.get(entry.id);
    byId.set(entry.id, entry.text === null && existing ? { ...entry, text: existing.text } : entry);
  }
  return [...byId.values()].sort((a, b) => b.id - a.id).slice(0, cap);
}

/**
 * The inbox after the server's answer: what it listed, plus the held entries
 * it could never have listed. A held entry it could have listed but did not
 * is gone — deleted, or in a channel the reader can no longer see.
 */
export function withServerAnswer(
  held: InboxEntry[],
  server: InboxEntry[],
  cap = MAX_INBOX_ENTRIES
): InboxEntry[] {
  const listed = new Set(server.map((entry) => entry.id));
  return mergeInbox(
    held.filter((entry) => entry.liveOnly || listed.has(entry.id)),
    server,
    cap
  );
}

function createMentionInbox() {
  const initial: InboxState = { entries: [], unseen: 0, loading: false };
  const { subscribe, set, update } = writable<InboxState>(initial);

  return {
    subscribe,
    /** The server's answer to `load-mentions`. */
    load(entries: InboxEntry[]) {
      update((state) => ({
        ...state,
        entries: withServerAnswer(state.entries, entries),
        loading: false
      }));
    },
    /**
     * A mention that just arrived. `unseen` is for one the user is not
     * already looking at — a mention in the open channel is read as it lands.
     */
    add(entry: InboxEntry, unseen: boolean) {
      update((state) => {
        const fresh = !state.entries.some((held) => held.id === entry.id);
        return {
          ...state,
          entries: mergeInbox(state.entries, [entry]),
          unseen: state.unseen + (fresh && unseen ? 1 : 0)
        };
      });
    },
    remove(id: number) {
      update((state) => ({ ...state, entries: state.entries.filter((entry) => entry.id !== id) }));
    },
    setLoading(loading: boolean) {
      update((state) => ({ ...state, loading }));
    },
    markSeen() {
      update((state) => ({ ...state, unseen: 0 }));
    },
    reset() {
      set(initial);
    }
  };
}

export const mentionInbox = createMentionInbox();
