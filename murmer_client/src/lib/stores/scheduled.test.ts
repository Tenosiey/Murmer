/**
 * Parsing of the two queue snapshots the server sends.
 *
 * These frames are untrusted input like any other, and the failure mode is
 * quiet: a row missing its id cannot be cancelled, a row missing its time
 * renders as "Invalid Date", and a row whose `firedAt` is misread either hides
 * a reminder that has come due or shows one that has not. All three look like
 * a working list.
 *
 * The sealed case is the one worth pinning by hand: a scheduled message
 * written for an encrypted channel arrives with `enc` set and `text` null
 * until its channel key does, and "waiting for a key" has to stay
 * distinguishable from "no words".
 */
import { describe, it, expect, beforeEach, vi } from 'vitest';
import type { ReminderEntry, ScheduledMessageEntry } from './scheduled';

type Store = typeof import('./scheduled');

let mod: Store;

beforeEach(async () => {
  vi.resetModules();
  mod = await import('./scheduled');
});

function messages(): ScheduledMessageEntry[] {
  let current: ScheduledMessageEntry[] = [];
  mod.scheduled.subscribe((state) => (current = state.messages))();
  return current;
}

function reminders(): ReminderEntry[] {
  let current: ReminderEntry[] = [];
  mod.scheduled.subscribe((state) => (current = state.reminders))();
  return current;
}

describe('parseScheduledMessage', () => {
  it('keeps a well-formed pending row', () => {
    expect(
      mod.parseScheduledMessage({
        id: 7,
        channelId: 3,
        scheduledFor: '2026-09-01T09:00:00Z',
        createdAt: '2026-08-25T09:00:00Z',
        failedReason: null,
        text: 'the notes are up',
        enc: null
      })
    ).toEqual({
      id: 7,
      channelId: 3,
      scheduledFor: '2026-09-01T09:00:00Z',
      createdAt: '2026-08-25T09:00:00Z',
      failedReason: null,
      text: 'the notes are up',
      encrypted: false
    });
  });

  it('marks a sealed row as encrypted with no preview', () => {
    const row = mod.parseScheduledMessage({
      id: 8,
      channelId: 4,
      scheduledFor: '2026-09-01T09:00:00Z',
      text: null,
      enc: { epoch: 2, nonce: 'n', ciphertext: 'c' }
    });
    expect(row?.encrypted).toBe(true);
    expect(row?.text).toBeNull();
  });

  it('drops a row that cannot be keyed, routed or sorted', () => {
    const base = { id: 1, channelId: 2, scheduledFor: '2026-09-01T09:00:00Z' };
    expect(mod.parseScheduledMessage({ ...base, id: undefined })).toBeNull();
    expect(mod.parseScheduledMessage({ ...base, channelId: '2' })).toBeNull();
    expect(mod.parseScheduledMessage({ ...base, scheduledFor: null })).toBeNull();
    expect(mod.parseScheduledMessage(null)).toBeNull();
    expect(mod.parseScheduledMessage('a row')).toBeNull();
  });

  it('falls back to the scheduled time when the server sends no createdAt', () => {
    expect(
      mod.parseScheduledMessage({ id: 1, channelId: 2, scheduledFor: '2026-09-01T09:00:00Z' })
        ?.createdAt
    ).toBe('2026-09-01T09:00:00Z');
  });
});

describe('parseReminder', () => {
  it('keeps a row anchored to a message', () => {
    expect(
      mod.parseReminder({
        id: 4,
        text: 'follow up',
        remindAt: '2026-08-25T18:00:00Z',
        channelId: 3,
        messageId: 91,
        createdAt: '2026-08-25T09:00:00Z',
        firedAt: null
      })
    ).toEqual({
      id: 4,
      text: 'follow up',
      remindAt: '2026-08-25T18:00:00Z',
      channelId: 3,
      messageId: 91,
      createdAt: '2026-08-25T09:00:00Z',
      firedAt: null
    });
  });

  it('reads an unanchored reminder as having no target', () => {
    const row = mod.parseReminder({
      id: 5,
      text: 'stretch',
      remindAt: '2026-08-25T18:00:00Z',
      channelId: null,
      messageId: null
    });
    expect(row?.channelId).toBeNull();
    expect(row?.messageId).toBeNull();
  });

  it('keeps an empty note rather than dropping the row', () => {
    // The server refuses to store one, but a row that exists and cannot be
    // rendered is still a row its owner has to be able to dismiss.
    expect(mod.parseReminder({ id: 6, text: '', remindAt: '2026-08-25T18:00:00Z' })?.id).toBe(6);
  });

  it('drops a row missing its id or time', () => {
    expect(mod.parseReminder({ text: 'x', remindAt: '2026-08-25T18:00:00Z' })).toBeNull();
    expect(mod.parseReminder({ id: 1, text: 'x' })).toBeNull();
    expect(mod.parseReminder({ id: 1, remindAt: '2026-08-25T18:00:00Z' })).toBeNull();
  });
});

describe('the store', () => {
  it('replaces a list rather than merging into it', () => {
    mod.scheduled.setReminders([
      { id: 1, text: 'first', remindAt: '2026-08-25T18:00:00Z' },
      { id: 2, text: 'second', remindAt: '2026-08-25T19:00:00Z' }
    ]);
    expect(reminders()).toHaveLength(2);
    // A snapshot is the whole truth: a dismissed reminder disappears by not
    // being in the next one, so merging would keep it forever.
    mod.scheduled.setReminders([{ id: 2, text: 'second', remindAt: '2026-08-25T19:00:00Z' }]);
    expect(reminders().map((entry) => entry.id)).toEqual([2]);
  });

  it('keeps the two lists independent', () => {
    mod.scheduled.setReminders([{ id: 1, text: 'x', remindAt: '2026-08-25T18:00:00Z' }]);
    mod.scheduled.setMessages([{ id: 9, channelId: 1, scheduledFor: '2026-08-25T18:00:00Z' }]);
    expect(reminders()).toHaveLength(1);
    expect(messages()).toHaveLength(1);
  });

  it('drops both lists on reset', () => {
    mod.scheduled.setReminders([{ id: 1, text: 'x', remindAt: '2026-08-25T18:00:00Z' }]);
    mod.scheduled.setMessages([{ id: 9, channelId: 1, scheduledFor: '2026-08-25T18:00:00Z' }]);
    mod.scheduled.reset();
    expect(reminders()).toEqual([]);
    expect(messages()).toEqual([]);
  });

  it('counts only what is asking for attention', () => {
    let attention = 0;
    const stop = mod.scheduledAttention.subscribe((value) => (attention = value));
    mod.scheduled.setReminders([
      { id: 1, text: 'pending', remindAt: '2026-08-25T18:00:00Z', firedAt: null },
      { id: 2, text: 'due', remindAt: '2026-08-25T17:00:00Z', firedAt: '2026-08-25T17:00:01Z' }
    ]);
    mod.scheduled.setMessages([
      { id: 9, channelId: 1, scheduledFor: '2026-08-25T18:00:00Z', failedReason: null },
      { id: 10, channelId: 1, scheduledFor: '2026-08-25T17:00:00Z', failedReason: 'muted' }
    ]);
    expect(attention).toBe(2);

    let due: ReminderEntry[] = [];
    const stopDue = mod.dueReminders.subscribe((value) => (due = value));
    expect(due.map((entry) => entry.id)).toEqual([2]);

    stop();
    stopDue();
  });
});
