/**
 * Saved messages: a personal bookmark list, separate from the server-wide
 * pins. Pins are the channel's; these are one reader's, so they never leave
 * the device — the server is not told what anyone saved.
 *
 * The list is kept in `localStorage` per server URL, newest save first.
 * Unlike the mentions inbox it survives a restart, which is the point of a
 * bookmark, and that is why an entry from an encrypted channel holds no text:
 * that plaintext must never land on disk. It keeps where the message is, and
 * jumping there reads it.
 */
import { derived, get, writable } from 'svelte/store';
import { browser } from '$app/environment';
import { selectedServer } from './servers';
import type { Message } from '../types';

/** Entries kept per server; saving past it drops the oldest save. */
export const MAX_SAVED_MESSAGES = 200;

const STORAGE_KEY = 'murmer_saved_messages';

export interface SavedMessage {
  id: number;
  channelId: number;
  /** Account name of the author. */
  user: string;
  /** The text, or null when there is none to keep (encrypted, a file). */
  text: string | null;
  timestamp: string | null;
}

type SavedByServer = Record<string, SavedMessage[]>;

/** The entry to keep for `msg` in `channelId`, or null if it has no id. */
export function savedEntry(msg: Message, channelId: number, encrypted: boolean): SavedMessage | null {
  if (typeof msg.id !== 'number' || typeof msg.user !== 'string') return null;
  const time = msg.timestamp ?? msg.time;
  return {
    id: msg.id,
    channelId,
    user: msg.user,
    text: !encrypted && typeof msg.text === 'string' && msg.text !== '' ? msg.text : null,
    timestamp: typeof time === 'string' ? time : null
  };
}

/** Save `entry` at the front, or unsave it when already saved. */
export function toggleSaved(
  list: SavedMessage[],
  entry: SavedMessage,
  cap = MAX_SAVED_MESSAGES
): SavedMessage[] {
  if (list.some((saved) => saved.id === entry.id)) {
    return list.filter((saved) => saved.id !== entry.id);
  }
  return [entry, ...list].slice(0, cap);
}

function isSavedMessage(value: unknown): value is SavedMessage {
  if (!value || typeof value !== 'object') return false;
  const entry = value as Record<string, unknown>;
  return (
    typeof entry.id === 'number' &&
    typeof entry.channelId === 'number' &&
    typeof entry.user === 'string' &&
    (entry.text === null || typeof entry.text === 'string') &&
    (entry.timestamp === null || typeof entry.timestamp === 'string')
  );
}

/** Parse the stored map, dropping anything malformed rather than failing. */
export function parseSaved(raw: string | null): SavedByServer {
  let parsed: unknown;
  try {
    parsed = raw === null ? {} : JSON.parse(raw);
  } catch {
    return {};
  }
  if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return {};
  const result: SavedByServer = {};
  for (const [server, list] of Object.entries(parsed)) {
    if (Array.isArray(list)) result[server] = list.filter(isSavedMessage);
  }
  return result;
}

const all = writable<SavedByServer>(parseSaved(browser ? localStorage.getItem(STORAGE_KEY) : null));

all.subscribe((value) => {
  if (browser) localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
});

/** The current server's saved messages, newest save first. */
export const savedMessages = derived([all, selectedServer], ([$all, $server]) =>
  $server ? ($all[$server] ?? []) : []
);

/** Save or unsave a message on the current server. */
export function toggleSavedMessage(entry: SavedMessage): void {
  const server = get(selectedServer);
  if (!server) return;
  all.update((map) => ({ ...map, [server]: toggleSaved(map[server] ?? [], entry) }));
}
