import { writable } from 'svelte/store';
import { browser } from '$app/environment';

/**
 * Customizable app-wide hotkeys. A binding is a combo string like
 * "Ctrl+Shift+M" or "F1" (modifiers ordered Ctrl, Alt, Shift, Meta); `null`
 * means the action is unbound. Bindings persist in localStorage. The
 * push-to-talk key is separate (see stores/settings.ts) because it is
 * hold-to-activate rather than a one-shot action.
 */

export type HotkeyActionId =
  | 'toggleMic'
  | 'toggleDeafen'
  | 'toggleVoice'
  | 'openSearch'
  | 'openSettings'
  | 'openHelp';

export interface HotkeyAction {
  id: HotkeyActionId;
  label: string;
  description: string;
  default: string;
}

export const HOTKEY_ACTIONS: HotkeyAction[] = [
  {
    id: 'toggleMic',
    label: 'Toggle microphone',
    description: 'Mute or unmute your microphone',
    default: 'Ctrl+Shift+M'
  },
  {
    id: 'toggleDeafen',
    label: 'Toggle speakers',
    description: 'Mute or unmute all incoming voice audio',
    default: 'Ctrl+Shift+O'
  },
  {
    id: 'toggleVoice',
    label: 'Join / leave voice',
    description: 'Join the last voice channel or leave the current one',
    default: 'Ctrl+Shift+V'
  },
  {
    id: 'openSearch',
    label: 'Search messages',
    description: 'Open the message search overlay',
    default: 'Ctrl+F'
  },
  {
    id: 'openSettings',
    label: 'Open settings',
    description: 'Open the settings window',
    default: 'Ctrl+Shift+S'
  },
  {
    id: 'openHelp',
    label: 'Show help',
    description: 'Open the slash-command and hotkey reference',
    default: 'F1'
  }
];

export type HotkeyBindings = Record<HotkeyActionId, string | null>;

const STORAGE_KEY = 'murmer_hotkeys';

function defaultBindings(): HotkeyBindings {
  return Object.fromEntries(
    HOTKEY_ACTIONS.map((action) => [action.id, action.default])
  ) as HotkeyBindings;
}

function loadBindings(): HotkeyBindings {
  const bindings = defaultBindings();
  if (!browser) return bindings;
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (!stored) return bindings;
    const parsed = JSON.parse(stored);
    if (!parsed || typeof parsed !== 'object') return bindings;
    for (const action of HOTKEY_ACTIONS) {
      const value = (parsed as Record<string, unknown>)[action.id];
      if (typeof value === 'string' && value.trim()) {
        bindings[action.id] = value;
      } else if (value === null) {
        bindings[action.id] = null;
      }
    }
  } catch (e) {
    console.error('Failed to parse stored hotkeys', e);
  }
  return bindings;
}

function createHotkeyStore() {
  const { subscribe, set, update } = writable<HotkeyBindings>(loadBindings());

  subscribe((value) => {
    if (browser) localStorage.setItem(STORAGE_KEY, JSON.stringify(value));
  });

  /**
   * Bind a combo to an action. A combo can only trigger one action, so any
   * other action currently holding it is unbound.
   */
  function bind(id: HotkeyActionId, combo: string) {
    update((bindings) => {
      const next = { ...bindings };
      for (const action of HOTKEY_ACTIONS) {
        if (next[action.id] === combo) next[action.id] = null;
      }
      next[id] = combo;
      return next;
    });
  }

  function unbind(id: HotkeyActionId) {
    update((bindings) => ({ ...bindings, [id]: null }));
  }

  function resetAll() {
    set(defaultBindings());
  }

  return { subscribe, bind, unbind, resetAll };
}

export const hotkeys = createHotkeyStore();

const MODIFIER_KEYS = new Set(['Control', 'Alt', 'Shift', 'Meta']);

/**
 * Normalize a keyboard event into a combo string, or `null` when only
 * modifiers are held. Used both when capturing a binding and when matching
 * incoming key presses, so the two can never disagree.
 */
export function eventToCombo(event: KeyboardEvent): string | null {
  if (MODIFIER_KEYS.has(event.key)) return null;
  const parts: string[] = [];
  if (event.ctrlKey) parts.push('Ctrl');
  if (event.altKey) parts.push('Alt');
  if (event.shiftKey) parts.push('Shift');
  if (event.metaKey) parts.push('Meta');
  let key = event.key;
  if (key === ' ') key = 'Space';
  if (key.length === 1) key = key.toUpperCase();
  parts.push(key);
  return parts.join('+');
}

/**
 * Whether a combo is safe to trigger while a text field has focus. Without a
 * real modifier (Shift alone doesn't count) a plain key like "M" must not
 * fire mid-sentence; function keys are always fine.
 */
export function firesWhileTyping(combo: string): boolean {
  if (/(^|\+)(Ctrl|Alt|Meta)\+/.test(`${combo}+`)) return true;
  const key = combo.split('+').pop() ?? '';
  return /^F\d{1,2}$/.test(key);
}

/** Whether the event target is an element the user types into. */
export function isTextInputTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  const tag = target.tagName.toLowerCase();
  return (
    tag === 'input' ||
    tag === 'textarea' ||
    tag === 'select' ||
    target.isContentEditable
  );
}

/** Human-friendly rendering of a combo for the settings UI. */
export function formatCombo(combo: string | null): string {
  if (!combo) return 'Not set';
  const pretty: Record<string, string> = {
    ArrowUp: '↑',
    ArrowDown: '↓',
    ArrowLeft: '←',
    ArrowRight: '→',
    Escape: 'Esc',
    Delete: 'Del'
  };
  return combo
    .split('+')
    .map((part) => pretty[part] ?? part)
    .join(' + ');
}
