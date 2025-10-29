/**
 * Utility functions for message processing and validation.
 */

import type { Message } from './types';

/**
 * Normalize reactions object to ensure consistent structure.
 * @param value - Raw reactions data
 * @returns Normalized reactions object with emoji keys and user arrays
 */
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
 * Prepare a raw message for display by normalizing timestamps, reactions, and ephemeral status.
 * @param raw - Raw message from server
 * @returns Prepared message ready for display
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

  // Normalize expiry
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
    delete (msg as any).expiresAt;
  }

  // Set ephemeral flag
  if (raw.ephemeral === true || Boolean(normalizedExpiry)) {
    msg.ephemeral = true;
  } else {
    delete (msg as any).ephemeral;
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

/**
 * Escape special characters in a string for use in a regular expression.
 * @param value - String to escape
 * @returns Escaped string safe for regex use
 */
export function escapeRegex(value: string): string {
  let escaped = '';
  for (const char of value) {
    escaped += REGEX_SPECIALS.has(char) ? `\\${char}` : char;
  }
  return escaped;
}

/**
 * Check if a text contains a mention of the given username.
 * @param text - Text to search
 * @param username - Username to look for
 * @returns True if the text mentions the username
 */
export function containsMention(
  text: string | undefined,
  username: string | null | undefined
): boolean {
  if (!text || !username) return false;
  const pattern = new RegExp(`(^|[^\\w@])@${escapeRegex(username)}(?=$|[^\\w-])`, 'i');
  return pattern.test(text);
}

