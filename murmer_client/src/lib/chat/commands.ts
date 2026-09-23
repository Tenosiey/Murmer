/**
 * The composer's slash commands, parsed into what the chat page should do.
 *
 * Parsing and validation live here, away from the page, so every usage
 * error and clamp is reachable from a test; the page only carries out the
 * result.
 */
import type { UserStatus } from '../types';
import {
  MAX_EPHEMERAL_SECONDS,
  MAX_TOPIC_LENGTH,
  MIN_EPHEMERAL_SECONDS,
  USER_STATUS_VALUES
} from './constants';
import { describeDuration } from './helpers';
import { parseWhen, reminderTextError, scheduleBoundsError, splitWhen } from './schedule';

export type SlashCommand =
  | { kind: 'none' }
  | { kind: 'error'; message: string }
  | { kind: 'help' }
  | { kind: 'reminders' }
  | { kind: 'send'; text: string }
  | { kind: 'topic'; topic: string }
  | { kind: 'status'; status: UserStatus }
  | { kind: 'ephemeral'; text: string; seconds: number; clampNote: string | null }
  | { kind: 'search'; query: string }
  | { kind: 'remind'; text: string; at: Date }
  | { kind: 'schedule'; text: string; at: Date };

// Backslash-escaped so markdown does not italicize the face.
const SHRUG = '¯\\\\\\_(ツ)\\_/¯';

const error = (message: string): SlashCommand => ({ kind: 'error', message });

/** The `<when> <text>` tail shared by /remind and /schedule. */
function parseTimed(rest: string, usage: string, now: Date): { at: Date; text: string } | string {
  const { when, text } = splitWhen(rest);
  if (!when || !text) return usage;
  const at = parseWhen(when, now);
  if (!at) return `“${when}” is not a time. Try 15m, 2h, 3d or 17:30.`;
  return scheduleBoundsError(at, now) ?? { at, text };
}

function parseEphemeral(rest: string): SlashCommand {
  const usage = 'Usage: /ephemeral <seconds> <message>';
  const [durationPart, ...words] = rest.split(/\s+/);
  const text = words.join(' ').trim();
  if (!durationPart || !text) return error(usage);
  const parsed = Number(durationPart);
  if (!Number.isFinite(parsed)) return error('Ephemeral duration must be a number of seconds.');
  const requested = Math.round(parsed);
  if (requested <= 0) return error('Ephemeral duration must be positive.');
  const seconds = Math.min(Math.max(requested, MIN_EPHEMERAL_SECONDS), MAX_EPHEMERAL_SECONDS);
  let clampNote: string | null = null;
  if (requested < MIN_EPHEMERAL_SECONDS) {
    clampNote = `Minimum duration is ${describeDuration(MIN_EPHEMERAL_SECONDS)}.`;
  } else if (requested > MAX_EPHEMERAL_SECONDS) {
    clampNote = `Maximum duration is ${describeDuration(MAX_EPHEMERAL_SECONDS)}.`;
  }
  return { kind: 'ephemeral', text, seconds, clampNote };
}

/** Parse a composer line that starts with `/`. */
export function parseSlashCommand(raw: string, now: Date = new Date()): SlashCommand {
  const content = raw.slice(1).trim();
  if (!content) return { kind: 'none' };
  const [command] = content.split(/\s+/);
  const name = command.toLowerCase();
  const rest = content.slice(command.length).trim();

  switch (name) {
    case 'help':
      return { kind: 'help' };
    case 'reminders':
      return { kind: 'reminders' };
    case 'me':
      return rest ? { kind: 'send', text: `_${rest}_` } : error('Usage: /me <action>');
    case 'shrug':
      return { kind: 'send', text: rest ? `${rest} ${SHRUG}` : SHRUG };
    case 'topic':
      return rest.length > MAX_TOPIC_LENGTH
        ? error(`Topics are limited to ${MAX_TOPIC_LENGTH} characters.`)
        : { kind: 'topic', topic: rest };
    case 'status': {
      if (!rest) return error('Usage: /status <online|away|busy|offline>');
      const status = USER_STATUS_VALUES.find((value) => value === rest.toLowerCase());
      return status
        ? { kind: 'status', status }
        : error(`Unknown status "${rest}". Options: ${USER_STATUS_VALUES.join(', ')}.`);
    }
    case 'ephemeral':
    case 'temp':
      return parseEphemeral(rest);
    case 'search':
      return { kind: 'search', query: rest };
    case 'remind':
    case 'remindme': {
      const timed = parseTimed(rest, 'Usage: /remind <when> <note> — e.g. /remind 15m stretch', now);
      if (typeof timed === 'string') return error(timed);
      const invalid = reminderTextError(timed.text);
      return invalid ? error(invalid) : { kind: 'remind', ...timed };
    }
    case 'schedule': {
      const timed = parseTimed(
        rest,
        'Usage: /schedule <when> <message> — e.g. /schedule 2h notes are up',
        now
      );
      return typeof timed === 'string' ? error(timed) : { kind: 'schedule', ...timed };
    }
    default:
      return error(`Unknown command: /${name}`);
  }
}
