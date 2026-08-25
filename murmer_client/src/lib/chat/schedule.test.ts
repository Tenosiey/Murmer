/**
 * The "when" parser behind `/remind` and `/schedule`.
 *
 * This is the one piece of the feature whose mistakes are silent. A `15m`
 * read as fifteen seconds still schedules something, a `9:00` that resolves
 * into this morning is refused by the server with a message about bounds
 * rather than about the typo, and a `2026-02-31` that JavaScript quietly
 * rolls into March books a reminder for a day the user never named. None of
 * those look wrong in the UI — the row appears, with the wrong time on it.
 *
 * Every case pins a fixed `now`, so the assertions describe the parse rather
 * than whatever the clock happened to say.
 */
import { describe, it, expect } from 'vitest';
import {
  parseWhen,
  splitWhen,
  scheduleBoundsError,
  reminderTextError,
  describeWhen,
  describeScheduleFailure
} from './schedule';
import { MAX_REMINDER_TEXT_LENGTH } from './constants';

/** A Tuesday, mid-afternoon, in whatever zone the test host is in. */
const NOW = new Date(2026, 8, 1, 14, 30, 0, 0);

describe('parseWhen', () => {
  it('reads a duration with a unit', () => {
    expect(parseWhen('90s', NOW)?.getTime()).toBe(NOW.getTime() + 90_000);
    expect(parseWhen('15m', NOW)?.getTime()).toBe(NOW.getTime() + 15 * 60_000);
    expect(parseWhen('2h', NOW)?.getTime()).toBe(NOW.getTime() + 2 * 3_600_000);
    expect(parseWhen('3d', NOW)?.getTime()).toBe(NOW.getTime() + 3 * 86_400_000);
    expect(parseWhen('1w', NOW)?.getTime()).toBe(NOW.getTime() + 7 * 86_400_000);
  });

  it('adds up a compound duration', () => {
    expect(parseWhen('1h30m', NOW)?.getTime()).toBe(NOW.getTime() + 90 * 60_000);
  });

  it('reads a bare number as minutes', () => {
    // The unit people leave off is the one they mean most often; reading it as
    // seconds would fire a "/remind 20" twenty seconds later.
    expect(parseWhen('20', NOW)?.getTime()).toBe(NOW.getTime() + 20 * 60_000);
  });

  it('is not case sensitive about units', () => {
    expect(parseWhen('2H', NOW)?.getTime()).toBe(parseWhen('2h', NOW)?.getTime());
  });

  it('reads a clock time later today', () => {
    const at = parseWhen('17:30', NOW);
    expect(at?.getHours()).toBe(17);
    expect(at?.getMinutes()).toBe(30);
    expect(at?.getDate()).toBe(NOW.getDate());
  });

  it('rolls a clock time that has already passed into tomorrow', () => {
    // Otherwise "/remind 9:00 standup" at half past two books a time in the
    // past, and the user hears about bounds instead of about tomorrow.
    const at = parseWhen('09:00', NOW);
    expect(at?.getDate()).toBe(NOW.getDate() + 1);
    expect(at?.getHours()).toBe(9);
  });

  it('reads an explicit date and time in the viewer’s zone', () => {
    const at = parseWhen('2026-09-04T09:15', NOW);
    expect(at?.getFullYear()).toBe(2026);
    expect(at?.getMonth()).toBe(8);
    expect(at?.getDate()).toBe(4);
    expect(at?.getHours()).toBe(9);
    expect(at?.getMinutes()).toBe(15);
    expect(parseWhen('2026-09-04 09:15', NOW)?.getTime()).toBe(at?.getTime());
  });

  it('refuses a day that does not exist instead of rolling into the next month', () => {
    expect(parseWhen('2026-02-31T09:00', NOW)).toBeNull();
  });

  it('refuses an out-of-range clock time', () => {
    expect(parseWhen('25:00', NOW)).toBeNull();
    expect(parseWhen('12:70', NOW)).toBeNull();
  });

  it('returns null for anything that is not a time', () => {
    for (const raw of ['', '   ', 'soon', 'later today', '5x', '1h2', '--']) {
      expect(parseWhen(raw, NOW), raw).toBeNull();
    }
  });
});

describe('splitWhen', () => {
  it('takes the first token as the time and the rest as the text', () => {
    expect(splitWhen('  15m  take a break  ')).toEqual({ when: '15m', text: 'take a break' });
  });

  it('reports an empty text when only a time was given', () => {
    expect(splitWhen('15m')).toEqual({ when: '15m', text: '' });
  });
});

describe('scheduleBoundsError', () => {
  it('accepts a time inside the server’s window', () => {
    expect(scheduleBoundsError(new Date(NOW.getTime() + 60_000), NOW)).toBeNull();
  });

  it('refuses a time inside the minimum lead', () => {
    // The server refuses rather than clamping, so catching it here keeps the
    // user's text in the composer instead of bouncing it back as an error.
    expect(scheduleBoundsError(new Date(NOW.getTime() + 5_000), NOW)).not.toBeNull();
    expect(scheduleBoundsError(new Date(NOW.getTime() - 60_000), NOW)).not.toBeNull();
  });

  it('refuses a time beyond a year', () => {
    const twoYears = new Date(NOW.getTime() + 2 * 365 * 86_400_000);
    expect(scheduleBoundsError(twoYears, NOW)).not.toBeNull();
  });
});

describe('reminderTextError', () => {
  it('requires a note', () => {
    expect(reminderTextError('   ')).not.toBeNull();
    expect(reminderTextError('check the deploy')).toBeNull();
  });

  it('measures the note in bytes, the way the server does', () => {
    // 'é' is two bytes, so a note of them hits the cap at half the character
    // count — a client counting characters would send one the server refuses.
    const justOver = 'é'.repeat(MAX_REMINDER_TEXT_LENGTH / 2 + 1);
    expect(justOver.length).toBeLessThan(MAX_REMINDER_TEXT_LENGTH);
    expect(reminderTextError(justOver)).not.toBeNull();
  });
});

describe('describeWhen', () => {
  it('says how long is left while it is close', () => {
    expect(describeWhen(new Date(NOW.getTime() + 45_000).toISOString(), NOW)).toBe('in 45s');
    expect(describeWhen(new Date(NOW.getTime() + 20 * 60_000).toISOString(), NOW)).toBe(
      'in 20 min'
    );
  });

  it('names the day once it is further out', () => {
    const tomorrow = new Date(NOW);
    tomorrow.setDate(tomorrow.getDate() + 1);
    tomorrow.setHours(9, 0, 0, 0);
    expect(describeWhen(tomorrow.toISOString(), NOW)).toMatch(/^tomorrow at /);

    const today = new Date(NOW);
    today.setHours(23, 0, 0, 0);
    expect(describeWhen(today.toISOString(), NOW)).toMatch(/^today at /);
  });

  it('says "due now" while a row is only waiting for the next tick', () => {
    // The server polls, so a row is briefly past its time before anything has
    // gone wrong; calling that overdue reads as a failure it is not.
    expect(describeWhen(new Date(NOW.getTime() - 5_000).toISOString(), NOW)).toBe('due now');
    expect(describeWhen(new Date(NOW.getTime() - 60_000).toISOString(), NOW)).toBe('due now');
  });

  it('marks a time that is genuinely late', () => {
    expect(describeWhen(new Date(NOW.getTime() - 600_000).toISOString(), NOW)).toMatch(/^overdue/);
  });

  it('shows an unparseable timestamp rather than "Invalid Date"', () => {
    expect(describeWhen('not a timestamp', NOW)).toBe('not a timestamp');
  });
});

describe('describeScheduleFailure', () => {
  it('explains the reasons the server sends', () => {
    expect(describeScheduleFailure('muted')).toMatch(/muted/i);
    expect(describeScheduleFailure('interrupted')).toMatch(/restart/i);
  });

  it('shows an unknown reason rather than dropping it', () => {
    // A reason the client has no wording for is still the only explanation the
    // author gets, so it is surfaced verbatim.
    expect(describeScheduleFailure('some-new-code')).toContain('some-new-code');
  });
});
