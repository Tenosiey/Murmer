/**
 * Turning what somebody types into a moment in time, for `/remind` and
 * `/schedule`.
 *
 * The server takes an RFC 3339 instant and refuses anything outside its
 * window rather than clamping it, because "when" is the whole request. That
 * makes the parse the part worth getting right: a `15m` read as 15 seconds, or
 * a `9:00` that silently lands in the past, is a reminder that arrives at a
 * time nobody asked for. Everything here is pure so it can be tested against
 * a fixed `now` instead of the clock.
 *
 * Times are interpreted in the **viewer's** zone. `17:30` means half past five
 * where the user is sitting; it becomes an absolute instant before it leaves
 * the client, so the server never has to know about zones at all.
 */
import {
  MIN_SCHEDULE_LEAD_SECONDS,
  MAX_SCHEDULE_AHEAD_SECONDS,
  MAX_REMINDER_TEXT_LENGTH
} from './constants';

/**
 * How long a row may sit past its time before the label calls it overdue.
 * Comfortably more than the server's polling interval, so a normal delivery
 * never renders as a problem.
 */
const DUE_NOW_GRACE_SECONDS = 120;

const UNIT_SECONDS: Record<string, number> = {
  s: 1,
  m: 60,
  h: 60 * 60,
  d: 24 * 60 * 60,
  w: 7 * 24 * 60 * 60
};

/** `90s`, `15m`, `1h30m`, `3d` — or a bare number, read as minutes. */
function parseDuration(raw: string): number | null {
  if (/^\d+$/.test(raw)) return Number(raw) * 60;
  if (!/^(\d+[smhdw])+$/i.test(raw)) return null;
  let seconds = 0;
  for (const [, amount, unit] of raw.matchAll(/(\d+)([smhdw])/gi)) {
    seconds += Number(amount) * UNIT_SECONDS[unit.toLowerCase()];
  }
  return seconds;
}

/** `17:30` — today if that is still ahead, tomorrow otherwise. */
function parseClock(raw: string, now: Date): Date | null {
  const match = /^(\d{1,2}):(\d{2})$/.exec(raw);
  if (!match) return null;
  const hours = Number(match[1]);
  const minutes = Number(match[2]);
  if (hours > 23 || minutes > 59) return null;
  const at = new Date(now);
  at.setHours(hours, minutes, 0, 0);
  // A time that has already passed today means the next one, not one in the
  // past — which the server would refuse and the user did not mean.
  if (at.getTime() <= now.getTime()) at.setDate(at.getDate() + 1);
  return at;
}

/** `2026-09-01T09:00` or `2026-09-01 09:00`, in the viewer's zone. */
function parseDateTime(raw: string): Date | null {
  const match = /^(\d{4})-(\d{2})-(\d{2})[T ](\d{1,2}):(\d{2})$/.exec(raw);
  if (!match) return null;
  const [, year, month, day, hours, minutes] = match.map(Number) as unknown as number[];
  if (month < 1 || month > 12 || day < 1 || day > 31 || hours > 23 || minutes > 59) return null;
  const at = new Date(year, month - 1, day, hours, minutes, 0, 0);
  // `new Date(2026, 1, 31)` rolls into March rather than failing, so the
  // round trip is what rejects a day that does not exist.
  if (at.getMonth() !== month - 1 || at.getDate() !== day) return null;
  return at;
}

/**
 * Read a "when" token. Returns `null` when it is not a time at all — the
 * caller shows the usage line rather than guessing.
 */
export function parseWhen(raw: string, now: Date = new Date()): Date | null {
  const token = raw.trim();
  if (!token) return null;
  const seconds = parseDuration(token);
  if (seconds !== null) return new Date(now.getTime() + seconds * 1000);
  return parseClock(token, now) ?? parseDateTime(token);
}

/**
 * Why the server would refuse this instant, or `null` when it would take it.
 * Mirrors the bounds in `ws/constants.rs` so the refusal arrives before the
 * round trip, with the user's text still in the composer.
 */
export function scheduleBoundsError(at: Date, now: Date = new Date()): string | null {
  const seconds = (at.getTime() - now.getTime()) / 1000;
  if (!Number.isFinite(seconds)) return 'That is not a time.';
  if (seconds < MIN_SCHEDULE_LEAD_SECONDS) {
    return `Pick a time at least ${MIN_SCHEDULE_LEAD_SECONDS} seconds from now.`;
  }
  if (seconds > MAX_SCHEDULE_AHEAD_SECONDS) {
    return 'Pick a time within the next year.';
  }
  return null;
}

/** Split `15m take a break` into its "when" and the rest. */
export function splitWhen(rest: string): { when: string; text: string } {
  const trimmed = rest.trim();
  const gap = trimmed.search(/\s/);
  if (gap === -1) return { when: trimmed, text: '' };
  return { when: trimmed.slice(0, gap), text: trimmed.slice(gap + 1).trim() };
}

/** Whether a reminder note is one the server will store. */
export function reminderTextError(text: string): string | null {
  const trimmed = text.trim();
  if (!trimmed) return 'A reminder needs a note.';
  // The server bounds the note in bytes, so a note of emoji hits the limit
  // sooner than its character count suggests.
  if (new TextEncoder().encode(trimmed).length > MAX_REMINDER_TEXT_LENGTH) {
    return `Keep the note under ${MAX_REMINDER_TEXT_LENGTH} bytes.`;
  }
  return null;
}

/**
 * "in 5 minutes", "tomorrow at 09:00", "3 Sept at 14:00" — the same instant
 * the row carries, said the way somebody would say it. Falls back to the raw
 * timestamp for anything unparseable, which is how a malformed server row
 * stays visible instead of rendering as "Invalid Date".
 */
export function describeWhen(iso: string, now: Date = new Date()): string {
  const at = new Date(iso);
  if (Number.isNaN(at.getTime())) return iso;
  const seconds = Math.round((at.getTime() - now.getTime()) / 1000);
  const clock = at.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });

  // The scheduler polls, so a row is briefly past its time before anything
  // has gone wrong. Calling that "overdue" reads as a failure for what is
  // really the next tick arriving.
  if (seconds < -DUE_NOW_GRACE_SECONDS) return `overdue — ${clock}`;
  if (seconds < 0) return 'due now';
  if (seconds < 60) return `in ${seconds}s`;
  if (seconds < 60 * 60) return `in ${Math.round(seconds / 60)} min`;

  const sameDay = at.toDateString() === now.toDateString();
  if (sameDay) return `today at ${clock}`;

  const tomorrow = new Date(now);
  tomorrow.setDate(tomorrow.getDate() + 1);
  if (at.toDateString() === tomorrow.toDateString()) return `tomorrow at ${clock}`;

  const date = at.toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
  return `${date} at ${clock}`;
}

/** Human text for the reason a scheduled message was not posted. */
export function describeScheduleFailure(reason: string): string {
  switch (reason) {
    case 'send-permission-denied':
      return 'You can no longer post in that channel.';
    case 'muted':
      return 'You were muted before it went out.';
    case 'channel-requires-encryption':
      return 'That channel switched to end-to-end encryption after this was written.';
    case 'channel-not-encrypted':
      return 'That channel is no longer encrypted.';
    case 'interrupted':
      return 'The server restarted while it was being sent, so it was not posted.';
    default:
      return `The server refused it: ${reason}`;
  }
}
