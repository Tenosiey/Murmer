/**
 * Reminders and scheduled messages: the two queues the server keeps on this
 * account's behalf.
 *
 * Both are *the server's answer, cached*. Nothing here is written
 * optimistically — the chat store sends a `set-reminder` or a
 * `schedule-message` and this list is replaced by the snapshot that comes
 * back, so two open clients cannot disagree about what is queued. Both are per
 * account, and the chat store resets them when it leaves a server: one
 * server's queue must never be visible on the next.
 *
 * A **due** reminder is one the server has fired and the user has not
 * dismissed. It stays in the list on purpose — a reminder that came due while
 * nobody was connected has to still be there when they come back, or the
 * feature quietly promises something it does not do.
 *
 * Like [`./pins`], this store is fed by the chat store rather than subscribing
 * to frames itself: a scheduled message written for an encrypted channel is
 * sealed, and its preview can only be opened once that channel's key has
 * arrived, which is knowledge the chat store owns.
 */
import { derived, writable } from 'svelte/store';

export interface ScheduledMessageEntry {
  id: number;
  channelId: number;
  /** RFC 3339 instant the server will post it at. */
  scheduledFor: string;
  createdAt: string;
  /** Wire code for why a delivery attempt was refused, or null while pending. */
  failedReason: string | null;
  /** The words, or null in an encrypted channel whose key has not arrived. */
  text: string | null;
  /** The channel was encrypted when this was written. */
  encrypted: boolean;
}

export interface ReminderEntry {
  id: number;
  text: string;
  remindAt: string;
  /** Where it was set, when it was set on a message. */
  channelId: number | null;
  messageId: number | null;
  createdAt: string;
  /** Set once it came due; it stays in the list until dismissed. */
  firedAt: string | null;
}

function asNumber(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) ? value : null;
}

function asString(value: unknown): string | null {
  return typeof value === 'string' ? value : null;
}

/**
 * Parse one scheduled-message row. The id, channel and time are what the list
 * is keyed, routed and sorted by, so a row missing any of them is dropped
 * rather than rendered with an invented value.
 */
export function parseScheduledMessage(raw: unknown): ScheduledMessageEntry | null {
  if (!raw || typeof raw !== 'object') return null;
  const row = raw as Record<string, unknown>;
  const id = asNumber(row.id);
  const channelId = asNumber(row.channelId);
  const scheduledFor = asString(row.scheduledFor);
  if (id === null || channelId === null || !scheduledFor) return null;
  return {
    id,
    channelId,
    scheduledFor,
    createdAt: asString(row.createdAt) ?? scheduledFor,
    failedReason: asString(row.failedReason),
    text: asString(row.text),
    encrypted: row.enc !== undefined && row.enc !== null
  };
}

export function parseReminder(raw: unknown): ReminderEntry | null {
  if (!raw || typeof raw !== 'object') return null;
  const row = raw as Record<string, unknown>;
  const id = asNumber(row.id);
  const remindAt = asString(row.remindAt);
  const text = asString(row.text);
  if (id === null || !remindAt || text === null) return null;
  return {
    id,
    text,
    remindAt,
    channelId: asNumber(row.channelId),
    messageId: asNumber(row.messageId),
    createdAt: asString(row.createdAt) ?? remindAt,
    firedAt: asString(row.firedAt)
  };
}

interface ScheduledState {
  messages: ScheduledMessageEntry[];
  reminders: ReminderEntry[];
}

const EMPTY: ScheduledState = { messages: [], reminders: [] };

function createScheduledStore() {
  const store = writable<ScheduledState>(EMPTY);

  return {
    subscribe: store.subscribe,

    /**
     * Replace the scheduled-message list with a server snapshot. Called by the
     * chat store, which opens the sealed previews first.
     */
    setMessages(items: unknown[]): void {
      const messages = items
        .map(parseScheduledMessage)
        .filter((entry): entry is ScheduledMessageEntry => entry !== null);
      store.update((current) => ({ ...current, messages }));
    },

    /** Replace the reminder list with a server snapshot. */
    setReminders(items: unknown[]): void {
      const reminders = items
        .map(parseReminder)
        .filter((entry): entry is ReminderEntry => entry !== null);
      store.update((current) => ({ ...current, reminders }));
    },

    /** Drop both lists, e.g. when leaving a server. */
    reset(): void {
      store.set(EMPTY);
    }
  };
}

export const scheduled = createScheduledStore();

/** Reminders the server has fired and the user has not dismissed. */
export const dueReminders = derived(scheduled, ($scheduled) =>
  $scheduled.reminders.filter((entry) => entry.firedAt !== null)
);

/**
 * Rows the user is expected to look at: reminders that have fired and messages
 * the server refused. Drives the header badge, which is the only place either
 * is visible without opening the panel.
 */
export const scheduledAttention = derived(
  scheduled,
  ($scheduled) =>
    $scheduled.reminders.filter((entry) => entry.firedAt !== null).length +
    $scheduled.messages.filter((entry) => entry.failedReason !== null).length
);
