/**
 * The server's auto-moderation rules, edited in the Server Dashboard's
 * Moderation tab.
 *
 * Like the profanity word list these are manager-only and never broadcast: a
 * rule's pattern describes what a server is trying to keep out, and handing
 * that to everyone hands it to whoever wants to phrase their way around it.
 * They arrive only in the direct answer to `get-automod-rules`, which the
 * server answers for a manager and nobody else.
 *
 * `null` therefore means "not disclosed to us", never "there are no rules" —
 * the editor waits for the answer rather than offering to save an empty list
 * over a real one. Everything the store enforces is cosmetic; the server
 * validates each rule again and is the only thing that matches messages.
 */
import { writable } from 'svelte/store';
import { chat } from './chat';
import { connection } from './connection';
import {
  AUTOMOD_ACTIONS,
  AUTOMOD_KINDS,
  DEFAULT_AUTOMOD_MUTE_SECONDS,
  MAX_AUTOMOD_RULES,
  MAX_MUTE_SECONDS,
  MIN_MUTE_SECONDS,
  type AutomodAction,
  type AutomodKind
} from '../chat/constants';
import type { Message } from '../types';

export interface AutomodRule {
  /** Operator's label. May be empty; it is all a warned member is told. */
  name: string;
  pattern: string;
  kind: AutomodKind;
  action: AutomodAction;
  /** Mute length in seconds; only meaningful when `action` is `mute`. */
  muteSeconds: number;
  enabled: boolean;
}

const KIND_IDS = AUTOMOD_KINDS.map((kind) => kind.id);
const ACTION_IDS = AUTOMOD_ACTIONS.map((action) => action.id);

/**
 * Parse one rule from a server frame, or `null` if it is not one this build
 * can render. An unknown `kind` or `action` is dropped rather than guessed
 * at — showing a rule as something it is not would let a manager save the
 * wrong action back over it.
 */
function parseRule(entry: unknown): AutomodRule | null {
  if (!entry || typeof entry !== 'object') return null;
  const row = entry as Record<string, unknown>;
  if (typeof row.pattern !== 'string' || !row.pattern) return null;
  if (typeof row.kind !== 'string' || !KIND_IDS.includes(row.kind as AutomodKind)) return null;
  if (typeof row.action !== 'string' || !ACTION_IDS.includes(row.action as AutomodAction)) {
    return null;
  }
  return {
    name: typeof row.name === 'string' ? row.name : '',
    pattern: row.pattern,
    kind: row.kind as AutomodKind,
    action: row.action as AutomodAction,
    muteSeconds: clampMuteSeconds(row.muteSeconds),
    enabled: row.enabled !== false
  };
}

/** The server clamps this too; a frame outside the bounds is a broken server. */
export function clampMuteSeconds(value: unknown): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return DEFAULT_AUTOMOD_MUTE_SECONDS;
  return Math.min(Math.max(Math.round(value), MIN_MUTE_SECONDS), MAX_MUTE_SECONDS);
}

/** null until the server has answered a request on this connection. */
export const automodRules = writable<AutomodRule[] | null>(null);

chat.on('automod-rules', (msg: Message) => {
  const raw = (msg as { rules?: unknown }).rules;
  if (!Array.isArray(raw)) return;
  automodRules.set(
    raw
      .map(parseRule)
      .filter((rule): rule is AutomodRule => rule !== null)
      .slice(0, MAX_AUTOMOD_RULES)
  );
});

connection.subscribe((state) => {
  // Forget this server's rules; the next one discloses its own, or does not.
  if (state !== 'connected') automodRules.set(null);
});

/** Ask the server for its rules (requires Manage Server; enforced there). */
export function requestAutomodRules(): void {
  chat.sendRaw({ type: 'get-automod-rules' });
}

/**
 * Replace the rule list. The confirmation is the `automod-rules` answer
 * carrying what was actually stored — never write the store from the editor,
 * because a rule the server refuses must not look saved.
 */
export function setAutomodRules(rules: AutomodRule[]): void {
  chat.sendRaw({ type: 'set-automod-rules', rules });
}
