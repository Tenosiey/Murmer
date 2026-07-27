/**
 * Push-to-Talk (PTT) key binding system.
 *
 * Two independent paths can hold the key down:
 *
 *  - DOM `keydown`/`keyup`, which only fire while the Murmer window has
 *    focus, and
 *  - an OS-level global shortcut registered through the Tauri plugin, which
 *    fires while any other application is focused.
 *
 * The second one is what makes push-to-talk usable at all while gaming; it is
 * only registered for combos carrying a real modifier or a function key,
 * because grabbing a bare key like Space system-wide would swallow it in
 * every other program. Both paths funnel into `setPressed`, which is
 * idempotent, so a combo handled by both never double-fires.
 */
import { eventToCombo, firesWhileTyping } from '../stores/hotkeys';
import { setGlobalPushToTalk } from '../stores/globalHotkeys';

/** Modifier key names as they appear in a combo string. */
const MODIFIERS = new Set(['Ctrl', 'Alt', 'Shift', 'Meta']);

/**
 * Bring a stored binding into the format `eventToCombo` produces. Older
 * captures recorded the raw `event.key`, so a letter could be stored
 * lowercase and would then never match.
 */
export function normalizePttKey(key: string): string {
  const parts = key.split('+');
  const last = parts[parts.length - 1];
  if (last === ' ') parts[parts.length - 1] = 'Space';
  else if (last.length === 1) parts[parts.length - 1] = last.toUpperCase();
  return parts.join('+');
}

/** The non-modifier key of a combo, e.g. "K" for "Ctrl+Shift+K". */
function mainKey(combo: string): string {
  return combo.split('+').pop() ?? '';
}

/** `event.key` normalised the way combo strings spell it. */
function eventKeyName(event: KeyboardEvent): string {
  const key = event.key;
  if (key === ' ') return 'Space';
  if (key === 'Control') return 'Ctrl';
  if (key.length === 1) return key.toUpperCase();
  return key;
}

export class PushToTalkManager {
  private currentKey: string = 'Space';
  private isPressed = false;
  private listeners: Array<(isPressed: boolean) => void> = [];
  private keydownHandler: ((e: KeyboardEvent) => void) | null = null;
  private keyupHandler: ((e: KeyboardEvent) => void) | null = null;
  private releaseHandler: (() => void) | null = null;
  private globalEnabled = false;

  constructor(initialKey: string = 'Space') {
    this.currentKey = normalizePttKey(initialKey);
    this.setupEventListeners();
  }

  /**
   * Set the key that activates push-to-talk
   */
  setKey(key: string) {
    this.currentKey = normalizePttKey(key);
    // Never leave the gate held open by a binding that no longer exists.
    this.setPressed(false);
    this.syncGlobalShortcut();
  }

  /**
   * Get the current PTT key
   */
  getKey(): string {
    return this.currentKey;
  }

  /**
   * Check if PTT key is currently pressed
   */
  getIsPressed(): boolean {
    return this.isPressed;
  }

  /**
   * Subscribe to PTT state changes
   */
  subscribe(callback: (isPressed: boolean) => void) {
    this.listeners.push(callback);
    return () => {
      this.listeners = this.listeners.filter(cb => cb !== callback);
    };
  }

  /**
   * Register (or drop) the OS-level shortcut. Called when joining and leaving
   * a voice channel: outside a channel push-to-talk does nothing, and there
   * is no reason to hold a system-wide key grab for it.
   */
  setGlobalEnabled(enabled: boolean) {
    this.globalEnabled = enabled;
    // Leaving the channel (or switching mode) while the key is held would
    // otherwise strand `isPressed` at true, and the next press — being a
    // no-op — would never reopen the gate.
    if (!enabled) this.setPressed(false);
    this.syncGlobalShortcut();
  }

  /**
   * Whether the current binding can be grabbed system-wide. A bare letter or
   * Space would be swallowed in every other application, so those stay
   * in-app only and the settings UI says so.
   */
  static isGlobalCapable(key: string): boolean {
    return firesWhileTyping(normalizePttKey(key));
  }

  private syncGlobalShortcut() {
    if (this.globalEnabled && PushToTalkManager.isGlobalCapable(this.currentKey)) {
      setGlobalPushToTalk({
        combo: this.currentKey,
        press: () => this.setPressed(true),
        release: () => this.setPressed(false)
      });
    } else {
      setGlobalPushToTalk(null);
    }
  }

  private setPressed(pressed: boolean) {
    if (this.isPressed === pressed) return;
    this.isPressed = pressed;
    this.notifyListeners(pressed);
  }

  private setupEventListeners() {
    this.keydownHandler = (e: KeyboardEvent) => {
      // Ignore if user is typing in an input field
      if (this.isInputFocused()) return;

      if (this.matchesKey(e, this.currentKey) && !this.isPressed) {
        e.preventDefault();
        e.stopPropagation();
        this.setPressed(true);
      }
    };

    this.keyupHandler = (e: KeyboardEvent) => {
      if (!this.isPressed) return;
      // Release on the main key *or* on any modifier of the combo. Letting go
      // of Ctrl before K in "Ctrl+K" produces a keyup whose modifier state no
      // longer matches the combo, and demanding an exact match there left the
      // microphone open until the key was pressed and released again.
      const released = eventKeyName(e);
      const parts = this.currentKey.split('+');
      if (released === mainKey(this.currentKey) || (MODIFIERS.has(released) && parts.includes(released))) {
        e.preventDefault();
        e.stopPropagation();
        this.setPressed(false);
      }
    };

    // Losing focus swallows the keyup entirely: alt-tabbing out of the window
    // while holding the key used to leave the microphone open indefinitely.
    this.releaseHandler = () => this.setPressed(false);

    // Listen on document to catch key events anywhere in the window.
    document.addEventListener('keydown', this.keydownHandler, true);
    document.addEventListener('keyup', this.keyupHandler, true);
    window.addEventListener('blur', this.releaseHandler);
    document.addEventListener('visibilitychange', this.releaseHandler);
  }

  private isInputFocused(): boolean {
    const activeElement = document.activeElement;
    if (!activeElement) return false;

    const tagName = activeElement.tagName.toLowerCase();
    return (
      tagName === 'input' ||
      tagName === 'textarea' ||
      tagName === 'select' ||
      activeElement.hasAttribute('contenteditable')
    );
  }

  private matchesKey(event: KeyboardEvent, keyName: string): boolean {
    return eventToCombo(event) === keyName;
  }

  private notifyListeners(isPressed: boolean) {
    for (const callback of this.listeners) {
      callback(isPressed);
    }
  }

  /**
   * Clean up event listeners
   */
  destroy() {
    if (this.keydownHandler) {
      document.removeEventListener('keydown', this.keydownHandler, true);
    }
    if (this.keyupHandler) {
      document.removeEventListener('keyup', this.keyupHandler, true);
    }
    if (this.releaseHandler) {
      window.removeEventListener('blur', this.releaseHandler);
      document.removeEventListener('visibilitychange', this.releaseHandler);
    }
    this.setGlobalEnabled(false);
    this.listeners = [];
  }

  /**
   * Get a human-readable key name for display
   */
  static getKeyDisplayName(key: string): string {
    const displayNames: Record<string, string> = {
      'Enter': 'Enter',
      'Escape': 'Esc',
      'Tab': 'Tab',
      'Backspace': 'Backspace',
      'Delete': 'Del',
      'ArrowUp': '↑',
      'ArrowDown': '↓',
      'ArrowLeft': '←',
      'ArrowRight': '→'
    };

    return normalizePttKey(key)
      .split('+')
      .map((part) => displayNames[part] ?? part)
      .join(' + ');
  }

  /**
   * Capture the next key press for key binding.
   *
   * Static because it needs no instance: constructing a throwaway manager
   * just to capture a key would attach a second set of document listeners
   * and, on destroy, release the running session's global key grab.
   */
  static captureKey(): Promise<string> {
    return new Promise((resolve) => {
      const captureHandler = (e: KeyboardEvent) => {
        // Modifiers alone are not a binding — keep listening until a real key
        // arrives, otherwise pressing Ctrl of "Ctrl+K" ends the capture.
        const combo = eventToCombo(e);
        e.preventDefault();
        e.stopPropagation();
        if (!combo) return;

        document.removeEventListener('keydown', captureHandler, true);
        resolve(combo);
      };

      document.addEventListener('keydown', captureHandler, true);
    });
  }
}
