/*
  Unsent composer text, parked per conversation so switching channels
  mid-sentence does not throw the sentence away.

  Session-local on purpose. `channelKeys.ts` keeps an encrypted channel's key
  material in memory rather than adding "another secret in `localStorage`";
  persisting drafts would undo that from the other end, writing to disk the
  plaintext of a message whose ciphertext never gets there.

  Drafts are still namespaced per server URL: channel ids and account names
  are only unique within one server, and reconnecting to the same server must
  hand back what was parked rather than drop it.
*/
import { get, writable } from 'svelte/store';

/** conversation key -> unsent text. Only non-empty drafts are held. */
type Drafts = Record<string, string>;

/** Composer key for a text channel. */
export function channelDraft(channelId: number): string {
  return `channel:${channelId}`;
}

/** Composer key for a direct-message conversation. Keyed by the peer's
 *  account name — the identity — never by a display name or nickname. */
export function dmDraft(peer: string): string {
  return `dm:${peer}`;
}

/** Composer key for a thread panel, by root message id. */
export function threadDraft(rootId: number): string {
  return `thread:${rootId}`;
}

function createDraftStore() {
  const store = writable<Drafts>({});
  const byServer = new Map<string, Drafts>();
  let activeServer: string | null = null;

  store.subscribe((value) => {
    if (activeServer) byServer.set(activeServer, value);
  });

  /** Remember what a composer holds for a conversation being left behind. */
  function park(key: string, text: string) {
    store.update((current) => {
      // An emptied composer drops its entry, so the map holds one key per
      // conversation actually left mid-sentence rather than one per
      // conversation ever opened.
      if (text === '') {
        if (!(key in current)) return current;
        const next = { ...current };
        delete next[key];
        return next;
      }
      if (current[key] === text) return current;
      return { ...current, [key]: text };
    });
  }

  /**
   * Hand a parked draft back to its composer and forget it. The live
   * composer owns the text from here and parks it again on the way out, so
   * the store only ever describes conversations the user is *not* looking
   * at.
   */
  function take(key: string): string {
    const text = get(store)[key] ?? '';
    if (text !== '') park(key, '');
    return text;
  }

  return {
    subscribe: store.subscribe,
    park,
    take,
    /** Switch to the given server's drafts. Called on connect, so a
     *  reconnect restores what was parked instead of discarding it. */
    setServer(url: string) {
      activeServer = url;
      store.set(byServer.get(url) ?? {});
    }
  };
}

export const drafts = createDraftStore();
