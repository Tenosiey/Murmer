/**
 * What the chat page says when the server announces moderation or
 * maintenance. These frames change nothing the client holds — the server has
 * already acted, and the lists it re-sends carry the result — so all that is
 * left is telling the user why the app just moved under them.
 *
 * Every field read here comes from an untrusted frame, so each describer
 * checks its shape and answers `null` rather than rendering `undefined`.
 */
import type { Message } from '../types';

export interface Notice {
  text: string;
  type: 'info' | 'error';
}

type Describe = (msg: Message, currentUser: string | null) => Notice | null;

/** " by <moderator>" when the frame names one. */
function byline(msg: Message): string {
  return typeof msg.by === 'string' && msg.by ? ` by ${msg.by}` : '';
}

/** Describers keyed by the frame type they answer. */
export const NOTICES: Record<string, Describe> = {
  'user-muted': (msg, currentUser) => {
    if (typeof msg.user !== 'string') return null;
    if (msg.user === currentUser) {
      const until =
        typeof msg.until === 'string' ? ` until ${new Date(msg.until).toLocaleString()}` : '';
      return { text: `You have been muted${until}.`, type: 'error' };
    }
    return { text: `${msg.user} has been muted.`, type: 'info' };
  },
  'user-unmuted': (msg, currentUser) => {
    if (typeof msg.user !== 'string') return null;
    if (msg.user === currentUser) return { text: 'You are no longer muted.', type: 'info' };
    return { text: `${msg.user} has been unmuted.`, type: 'info' };
  },
  // A `warn` rule lets the message through and tells the sender privately.
  // It carries the rule's name, never its pattern — what the server filters
  // is not something everyone gets to read.
  'automod-warning': (msg) => {
    const name = typeof msg.rule === 'string' ? msg.rule.trim() : '';
    const rule = name ? `the “${name}” rule` : 'an auto-moderation rule';
    return { text: `Your message was flagged by ${rule}.`, type: 'error' };
  },
  'user-unbanned': (msg) => {
    if (typeof msg.user !== 'string') return null;
    return { text: `${msg.user} has been unbanned.`, type: 'info' };
  },
  // Danger Zone actions rearrange the app under everyone at once: without a
  // word, the history and half the channels simply vanish mid-sentence.
  'messages-purged': (msg) => ({
    text: `Every message on this server was deleted${byline(msg)}.`,
    type: 'error'
  }),
  // The channel this client was viewing may be gone. The server re-sends the
  // channel lists, and the chat page drops back to `general` and rejoins on
  // its own, so all that is left here is saying why.
  'server-reset': (msg) => ({ text: `This server was reset${byline(msg)}.`, type: 'error' })
};

/**
 * Why this client is being thrown off the server, or `null` when the
 * `force-disconnect` frame names somebody else.
 */
export function describeForceDisconnect(msg: Message, currentUser: string | null): string | null {
  if (!msg.user || msg.user !== currentUser) return null;
  const action = msg.action === 'banned' ? 'banned from' : 'kicked from';
  return `You were ${action} this server${byline(msg)}.`;
}
