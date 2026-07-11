import { get } from 'svelte/store';
import {
  hotkeys,
  HOTKEY_ACTIONS,
  globalHotkeysEnabled,
  firesWhileTyping,
  type HotkeyActionId
} from './hotkeys';

/**
 * Registers the voice hotkeys (actions flagged `global` in HOTKEY_ACTIONS)
 * as OS-level global shortcuts via the Tauri global-shortcut plugin, so they
 * keep working while another application is focused.
 *
 * Note that the OS consumes a registered combo before it reaches the webview,
 * so for these combos this module is the only trigger path — the DOM keydown
 * handler in the chat page never sees them. In the plain browser (dev
 * without the Tauri shell) nothing is registered and the DOM handler covers
 * everything as before.
 */

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

/**
 * Our combo format matches the plugin's accelerator format except for the
 * Windows/Cmd key, which the plugin calls "Super" instead of "Meta".
 */
function comboToAccelerator(combo: string): string {
  return combo
    .split('+')
    .map((part) => (part === 'Meta' ? 'Super' : part))
    .join('+');
}

type ActionCallbacks = Partial<Record<HotkeyActionId, () => void>>;

let callbacks: ActionCallbacks = {};
let suspended = false;

// register/unregister are async; funnel every resync through one promise
// chain so overlapping updates can't interleave their plugin calls.
let queue: Promise<void> = Promise.resolve();

function resync() {
  if (!isTauri) return;
  queue = queue
    .then(applyBindings)
    .catch((e) => console.error('Failed to sync global hotkeys:', e));
}

async function applyBindings() {
  const { register, unregisterAll } = await import('@tauri-apps/plugin-global-shortcut');
  await unregisterAll();
  if (suspended || !get(globalHotkeysEnabled)) return;

  const bindings = get(hotkeys);
  for (const action of HOTKEY_ACTIONS) {
    if (!action.global) continue;
    const combo = bindings[action.id];
    const callback = callbacks[action.id];
    if (!combo || !callback) continue;
    // A combo without a real modifier or function key (e.g. plain "M")
    // would swallow that key in every application — keep it in-app only.
    if (!firesWhileTyping(combo)) continue;
    try {
      await register(comboToAccelerator(combo), (event) => {
        if (event.state === 'Pressed') callback();
      });
    } catch (e) {
      console.warn(`Could not register global hotkey "${combo}":`, e);
    }
  }
}

/**
 * Provide the action implementations (called by the chat page on mount).
 * Global shortcuts are only active while callbacks are set, so they die
 * with the chat page and never fire on the login/server screens.
 */
export function setGlobalHotkeyActions(map: ActionCallbacks) {
  callbacks = map;
  resync();
}

export function clearGlobalHotkeyActions() {
  callbacks = {};
  resync();
}

/**
 * Temporarily release all global shortcuts. Used while the settings modal
 * captures a new binding — a registered combo would otherwise be consumed
 * by the OS and both trigger its action and never reach the capture.
 */
export function suspendGlobalHotkeys() {
  suspended = true;
  resync();
}

export function resumeGlobalHotkeys() {
  suspended = false;
  resync();
}

// Re-register whenever bindings or the system-wide toggle change.
hotkeys.subscribe(resync);
globalHotkeysEnabled.subscribe(resync);
