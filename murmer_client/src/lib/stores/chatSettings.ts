/**
 * Server-wide chat policy: slow mode, the per-message length cap and the
 * profanity filter.
 *
 * The server announces the three public values after authentication and
 * broadcasts them on change; it enforces all of them itself, so what this
 * store drives is cosmetic — the composer's `maxlength`, the slow mode hint
 * and the Server Dashboard's editor.
 *
 * The filtered word list is deliberately *not* broadcast: it only arrives in
 * the direct answer to `get-chat-settings`, which the server only gives to a
 * manager. `words: null` therefore means "not disclosed to us", never "the
 * list is empty" — the editor waits for the answer instead of offering to
 * save an empty list over a real one.
 */
import { writable, get } from 'svelte/store';
import { chat } from './chat';
import { connection } from './connection';
import { session } from './session';
import { MAX_MESSAGE_LENGTH } from '../chat/constants';
import type { Message } from '../types';

export interface ChatSettings {
  /** Seconds between messages; 0 means slow mode is off. */
  slowModeSeconds: number;
  /** Largest accepted message length in characters. */
  maxMessageLength: number;
  /** Whether the server masks filtered words in new messages. */
  profanityFilter: boolean;
  /** Filtered words, or null while the server has not disclosed them. */
  words: string[] | null;
}

function defaultSettings(): ChatSettings {
  return {
    slowModeSeconds: 0,
    maxMessageLength: MAX_MESSAGE_LENGTH,
    profanityFilter: false,
    words: null
  };
}

/** Clamp a frame's number to something the UI can render sensibly. */
function positiveInt(value: unknown, fallback: number, max: number): number {
  return typeof value === 'number' && Number.isFinite(value) && value > 0
    ? Math.min(Math.round(value), max)
    : fallback;
}

export const chatSettings = writable<ChatSettings>(defaultSettings());

chat.on('chat-settings', (msg: Message) => {
  const payload = msg as any;
  chatSettings.update((current) => ({
    slowModeSeconds:
      typeof payload.slowModeSeconds === 'number' && payload.slowModeSeconds >= 0
        ? Math.round(payload.slowModeSeconds)
        : 0,
    // A server can only ever lower the hard cap, so a frame claiming more
    // than the build knows about is clamped rather than trusted.
    maxMessageLength: positiveInt(payload.maxMessageLength, MAX_MESSAGE_LENGTH, MAX_MESSAGE_LENGTH),
    profanityFilter: payload.profanityFilter === true,
    // Only a manager's `get-chat-settings` answer carries the word list; a
    // broadcast without it must not wipe what the editor is holding.
    words: Array.isArray(payload.words)
      ? payload.words.filter((word: unknown): word is string => typeof word === 'string')
      : current.words
  }));
});

/**
 * When the server last accepted a message of ours, which is what its own slow
 * mode timer is keyed on. Taken from our message coming back rather than from
 * the moment we sent it: a send the server refused (or a slash command that
 * never became a message) must not start a countdown, and a send it accepted
 * always comes back.
 */
const lastOwnMessageAt = writable<number | null>(null);

chat.on('chat', (msg: Message) => {
  const user = (msg as any).user;
  if (typeof user === 'string' && user === get(session).user) lastOwnMessageAt.set(Date.now());
});

connection.subscribe((state) => {
  // Forget the previous server's policy; the next one announces its own.
  if (state !== 'connected') {
    chatSettings.set(defaultSettings());
    lastOwnMessageAt.set(null);
  }
});

/** Ask the server for the policy including the filtered word list. */
export function requestChatSettings(): void {
  chat.sendRaw({ type: 'get-chat-settings' });
}

/**
 * Update the chat policy (requires Manage Server; enforced server-side). The
 * confirmation arrives as a broadcast `chat-settings` frame plus a direct one
 * carrying the stored word list.
 */
export function setChatSettings(settings: {
  slowModeSeconds: number;
  maxMessageLength: number;
  profanityFilter: boolean;
  words: string[];
}): void {
  chat.sendRaw({ type: 'set-chat-settings', ...settings });
}

/**
 * Seconds still to wait under slow mode, given when a message was last
 * accepted. Zero while slow mode is off or the interval has passed.
 */
export function slowModeRemaining(lastSentAt: number | null, now = Date.now()): number {
  const interval = get(chatSettings).slowModeSeconds;
  if (interval <= 0 || lastSentAt === null) return 0;
  const elapsed = (now - lastSentAt) / 1000;
  return elapsed >= interval ? 0 : Math.ceil(interval - elapsed);
}

/**
 * Seconds this client still has to wait before its next message. Cosmetic:
 * the server enforces slow mode itself and exempts members who can manage
 * messages, so the composer only uses this to say so before clearing what
 * somebody typed.
 */
export function slowModeWait(now = Date.now()): number {
  return slowModeRemaining(get(lastOwnMessageAt), now);
}
