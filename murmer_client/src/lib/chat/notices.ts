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
import { t } from '../i18n';

export interface Notice {
  text: string;
  type: 'info' | 'error';
}

type Describe = (msg: Message, currentUser: string | null) => Notice | null;

/** The moderator the frame names, if any. */
function actor(msg: Message): string | null {
  return typeof msg.by === 'string' && msg.by ? msg.by : null;
}

/** Describers keyed by the frame type they answer. */
export const NOTICES: Record<string, Describe> = {
  'user-muted': (msg, currentUser) => {
    if (typeof msg.user !== 'string') return null;
    if (msg.user === currentUser) {
      const text =
        typeof msg.until === 'string'
          ? t('notice.youMutedUntil', { until: new Date(msg.until).toLocaleString() })
          : t('notice.youMuted');
      return { text, type: 'error' };
    }
    return { text: t('notice.userMuted', { name: msg.user }), type: 'info' };
  },
  'user-unmuted': (msg, currentUser) => {
    if (typeof msg.user !== 'string') return null;
    if (msg.user === currentUser) return { text: t('notice.youUnmuted'), type: 'info' };
    return { text: t('notice.userUnmuted', { name: msg.user }), type: 'info' };
  },
  // A `warn` rule lets the message through and tells the sender privately.
  // It carries the rule's name, never its pattern — what the server filters
  // is not something everyone gets to read.
  'automod-warning': (msg) => {
    const name = typeof msg.rule === 'string' ? msg.rule.trim() : '';
    const text = name ? t('notice.flaggedByRule', { rule: name }) : t('notice.flagged');
    return { text, type: 'error' };
  },
  'user-unbanned': (msg) => {
    if (typeof msg.user !== 'string') return null;
    return { text: t('notice.userUnbanned', { name: msg.user }), type: 'info' };
  },
  // Danger Zone actions rearrange the app under everyone at once: without a
  // word, the history and half the channels simply vanish mid-sentence.
  'messages-purged': (msg) => {
    const by = actor(msg);
    return { text: by ? t('notice.purgedBy', { by }) : t('notice.purged'), type: 'error' };
  },
  // The channel this client was viewing may be gone. The server re-sends the
  // channel lists, and the chat page drops back to `general` and rejoins on
  // its own, so all that is left here is saying why.
  'server-reset': (msg) => {
    const by = actor(msg);
    return { text: by ? t('notice.resetBy', { by }) : t('notice.reset'), type: 'error' };
  }
};

/**
 * Why this client is being thrown off the server, or `null` when the
 * `force-disconnect` frame names somebody else.
 */
export function describeForceDisconnect(msg: Message, currentUser: string | null): string | null {
  if (!msg.user || msg.user !== currentUser) return null;
  const by = actor(msg);
  if (msg.action === 'banned') return by ? t('notice.bannedBy', { by }) : t('notice.banned');
  return by ? t('notice.kickedBy', { by }) : t('notice.kicked');
}
