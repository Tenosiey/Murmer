/**
 * Turning message frames from the server into what the UI renders. Every
 * field is untrusted input, so each normalizer returns a checked shape or
 * nothing at all.
 */

import type { AttachmentInfo, ForwardInfo, Message, ReplyInfo } from './types';

/** Normalize reactions object to ensure consistent structure. */
export function normalizeReactions(value: unknown): Record<string, string[]> {
  if (!value || typeof value !== 'object') return {};
  const result: Record<string, string[]> = {};
  for (const [emoji, users] of Object.entries(value as Record<string, unknown>)) {
    if (!emoji) continue;
    if (Array.isArray(users)) {
      const filtered = users.filter(
        (u): u is string => typeof u === 'string' && u.trim().length > 0
      );
      result[emoji] = Array.from(new Set(filtered));
    }
  }
  return result;
}

/**
 * Validate an attachment payload from the server. Only http(s) URLs pass so a
 * crafted message cannot smuggle javascript: or data: links into the UI.
 * @returns A safe attachment descriptor, or undefined if invalid
 */
export function normalizeAttachment(value: unknown): AttachmentInfo | undefined {
  if (!value || typeof value !== 'object') return undefined;
  const raw = value as Record<string, unknown>;
  if (typeof raw.url !== 'string' || typeof raw.name !== 'string') return undefined;
  let parsed: URL;
  try {
    parsed = new URL(raw.url);
  } catch {
    return undefined;
  }
  if (parsed.protocol !== 'http:' && parsed.protocol !== 'https:') return undefined;
  const name = raw.name.trim();
  if (!name) return undefined;
  const size = typeof raw.size === 'number' && Number.isFinite(raw.size) && raw.size >= 0 ? raw.size : 0;
  return { url: raw.url, name, size };
}

/**
 * Validate the reply metadata attached to a message by the server.
 * @returns A safe reply descriptor, or undefined if invalid
 */
export function normalizeReplyTo(value: unknown): ReplyInfo | undefined {
  if (!value || typeof value !== 'object') return undefined;
  const raw = value as Record<string, unknown>;
  if (typeof raw.id !== 'number' || !Number.isFinite(raw.id)) return undefined;
  const user = typeof raw.user === 'string' ? raw.user : '';
  const text = typeof raw.text === 'string' ? raw.text : '';
  return { id: raw.id, user, text };
}

/**
 * Validate the forwarding metadata the server stamps on a forwarded message.
 *
 * The stamp is the only thing telling a reader that these words are somebody
 * else's, so a malformed one is dropped rather than half-rendered: a chip
 * naming an empty author under real text reads as an attribution failure, and
 * an unattributed forward reads as the forwarder's own words. Both are worse
 * than showing the message plainly.
 * @returns A safe forwarding descriptor, or undefined if invalid
 */
export function normalizeForwardedFrom(value: unknown): ForwardInfo | undefined {
  if (!value || typeof value !== 'object') return undefined;
  const raw = value as Record<string, unknown>;
  if (typeof raw.id !== 'number' || !Number.isFinite(raw.id)) return undefined;
  if (typeof raw.channelId !== 'number' || !Number.isFinite(raw.channelId)) return undefined;
  if (typeof raw.user !== 'string' || raw.user === '') return undefined;
  const channel = typeof raw.channel === 'string' ? raw.channel : '';
  return { id: raw.id, user: raw.user, channel, channelId: raw.channelId };
}

/**
 * Prepare a raw message for display by normalizing timestamps, reactions, and ephemeral status.
 */
export function prepareMessage(raw: Message): Message {
  const msg: Message = { ...raw };

  // Normalize timestamp
  if (typeof msg.timestamp === 'string') {
    const parsed = Date.parse(msg.timestamp);
    if (!Number.isNaN(parsed)) {
      const date = new Date(parsed);
      msg.timestamp = date.toISOString();
      if (!msg.time) {
        msg.time = date.toLocaleTimeString();
      }
    } else {
      msg.timestamp = undefined;
    }
  } else if (msg.timestamp !== undefined) {
    msg.timestamp = undefined;
  }

  // Ensure time field exists
  if (!msg.time) {
    msg.time = new Date().toLocaleTimeString();
  }

  // Normalize reactions
  msg.reactions = normalizeReactions(raw.reactions);

  // Normalize attachment
  const attachment = normalizeAttachment(raw.attachment);
  if (attachment) {
    msg.attachment = attachment;
  } else {
    delete msg.attachment;
  }

  // Normalize reply metadata
  const replyTo = normalizeReplyTo(raw.replyTo);
  if (replyTo) {
    msg.replyTo = replyTo;
  } else {
    delete msg.replyTo;
  }
  if (typeof raw.threadId !== 'number' || !Number.isFinite(raw.threadId)) {
    delete msg.threadId;
  }

  // Normalize forwarding metadata
  const forwardedFrom = normalizeForwardedFrom(raw.forwardedFrom);
  if (forwardedFrom) {
    msg.forwardedFrom = forwardedFrom;
  } else {
    delete msg.forwardedFrom;
  }

  let normalizedExpiry: string | undefined;
  if (typeof raw.expiresAt === 'string') {
    const parsed = Date.parse(raw.expiresAt);
    if (!Number.isNaN(parsed)) {
      normalizedExpiry = new Date(parsed).toISOString();
    }
  }

  if (normalizedExpiry) {
    msg.expiresAt = normalizedExpiry;
  } else {
    delete msg.expiresAt;
  }

  // Set ephemeral flag
  if (raw.ephemeral === true || Boolean(normalizedExpiry)) {
    msg.ephemeral = true;
  } else {
    delete msg.ephemeral;
  }

  return msg;
}

/** Set of characters that need escaping in regular expressions */
const REGEX_SPECIALS = new Set([
  '\\',
  '.',
  '+',
  '*',
  '?',
  '^',
  '$',
  '{',
  '}',
  '(',
  ')',
  '|',
  '[',
  ']',
  '/',
  '-'
]);

/** Escape special characters in a string for use in a regular expression. */
export function escapeRegex(value: string): string {
  let escaped = '';
  for (const char of value) {
    escaped += REGEX_SPECIALS.has(char) ? `\\${char}` : char;
  }
  return escaped;
}

/** Check if a text contains a mention of the given username. */
export function containsMention(
  text: string | undefined,
  username: string | null | undefined
): boolean {
  if (!text || !username) return false;
  const pattern = new RegExp(`(^|[^\\w@])@${escapeRegex(username)}(?=$|[^\\w-])`, 'i');
  return pattern.test(text);
}


/**
 * Merge a `history` page into the flat message store.
 *
 * A page is not only ever older than what is held: rejoining a channel, or
 * a connection that fell behind its channel broadcast (see
 * `docs/protocol.md`), brings the *newest* page, which can hold messages
 * newer than ones already here. So the result is sorted by id rather than
 * the page being prepended.
 *
 * Pages are contiguous per channel, which decides the rest:
 * - The page is the server's current answer, so it wins where both hold a
 *   message — that is how a missed edit or reaction lands — and a held
 *   message of that channel inside the page's id range but absent from it
 *   was deleted meanwhile.
 * - A page entirely newer than everything held for its channel may have
 *   left a gap below it that scrolling up would never fill, since that
 *   loads from the oldest held id. Those held messages are dropped, and
 *   scrolling up loads them back contiguously.
 */
export function mergeHistory(existing: Message[], page: Message[]): Message[] {
  const ids = page.map((m) => m.id).filter((id): id is number => typeof id === 'number');
  if (ids.length === 0) return existing;
  const channelId = page[0].channelId;
  const low = Math.min(...ids);
  const high = Math.max(...ids);
  const fresh = new Set(ids);
  const held = existing.filter((m) => m.channelId === channelId && typeof m.id === 'number');
  const gap = held.length > 0 && held.every((m) => (m.id as number) < low);
  const kept = existing.filter((m) => {
    if (typeof m.id !== 'number' || m.channelId !== channelId) return true;
    return !gap && !fresh.has(m.id) && (m.id < low || m.id > high);
  });
  const sortKey = (m: Message) => (typeof m.id === 'number' ? m.id : Number.MAX_SAFE_INTEGER);
  return [...kept, ...page].sort((a, b) => sortKey(a) - sortKey(b));
}
