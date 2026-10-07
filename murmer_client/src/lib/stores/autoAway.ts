/**
 * Auto-away: switch the user's status to away after a stretch without input,
 * and back to online on their return.
 *
 * Only a status this module set is ever undone. Someone who picked "away" or
 * "busy" by hand meant it, and moving the mouse must not overrule them; the
 * same goes for a manual change made while auto-away was in effect.
 *
 * "Input" is input to Murmer itself — the browser cannot see the rest of the
 * system, so a user typing in another window counts as idle here.
 */
import { get, writable } from 'svelte/store';
import { browser } from '$app/environment';
import { session } from './session';
import { statuses } from './status';
import type { UserStatus } from '../types';

/** The choices offered in settings, in minutes; 0 turns auto-away off. */
export const AUTO_AWAY_OPTIONS = [0, 5, 10, 15, 30, 60] as const;
const DEFAULT_MINUTES = 10;
const STORAGE_KEY = 'murmer_auto_away_minutes';
const CHECK_INTERVAL_MS = 15_000;
const INPUT_EVENTS = ['pointermove', 'pointerdown', 'keydown', 'wheel', 'touchstart'] as const;

function loadMinutes(): number {
  if (!browser) return DEFAULT_MINUTES;
  const stored = Number(localStorage.getItem(STORAGE_KEY) ?? NaN);
  return (AUTO_AWAY_OPTIONS as readonly number[]).includes(stored) ? stored : DEFAULT_MINUTES;
}

export const autoAwayMinutes = writable<number>(loadMinutes());

autoAwayMinutes.subscribe((value) => {
  if (browser) localStorage.setItem(STORAGE_KEY, String(value));
});

/**
 * The status auto-away should switch to, or null to leave it alone.
 * `autoSet` is whether the current "away" is one auto-away set itself.
 */
export function autoAwayStep(
  current: UserStatus,
  idle: boolean,
  autoSet: boolean
): UserStatus | null {
  if (idle && current === 'online') return 'away';
  if (!idle && autoSet && current === 'away') return 'online';
  return null;
}

/** Watch for input while connected; returns the function that stops it. */
export function startAutoAway(): () => void {
  let lastInput = Date.now();
  let autoSet = false;

  function check() {
    const user = get(session).user;
    if (!user) return;
    // No entry yet means the server has us as plainly online.
    const current = get(statuses)[user] ?? 'online';
    // Any status other than our own "away" means the user took over.
    if (current !== 'away') autoSet = false;
    const minutes = get(autoAwayMinutes);
    const idle = minutes > 0 && Date.now() - lastInput >= minutes * 60_000;
    const next = autoAwayStep(current, idle, autoSet);
    if (!next) return;
    statuses.setSelf(next);
    autoSet = next === 'away';
  }

  // Coming back is answered at once rather than on the next tick; while
  // auto-away is not in effect an input only moves the timestamp.
  function onInput() {
    lastInput = Date.now();
    if (autoSet) check();
  }

  for (const type of INPUT_EVENTS) {
    window.addEventListener(type, onInput, { capture: true, passive: true });
  }
  const timer = window.setInterval(check, CHECK_INTERVAL_MS);

  return () => {
    window.clearInterval(timer);
    for (const type of INPUT_EVENTS) {
      window.removeEventListener(type, onInput, { capture: true });
    }
  };
}
