/**
 * Push-to-Talk (PTT) key binding system
 * Handles global key press detection for voice activation
 */

export class PushToTalkManager {
  private currentKey: string = 'Space';
  private isPressed = false;
  private listeners: Array<(isPressed: boolean) => void> = [];
  private keydownHandler: ((e: KeyboardEvent) => void) | null = null;
  private keyupHandler: ((e: KeyboardEvent) => void) | null = null;

  constructor(initialKey: string = 'Space') {
    this.currentKey = initialKey;
    this.setupEventListeners();
  }

  /**
   * Set the key that activates push-to-talk
   */
  setKey(key: string) {
    this.currentKey = key;
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

  private setupEventListeners() {
    this.keydownHandler = (e: KeyboardEvent) => {
      // Ignore if user is typing in an input field
      if (this.isInputFocused()) return;
      
      if (this.matchesKey(e, this.currentKey) && !this.isPressed) {
        e.preventDefault();
        e.stopPropagation();
        this.isPressed = true;
        this.notifyListeners(true);
      }
    };

    this.keyupHandler = (e: KeyboardEvent) => {
      if (this.matchesKey(e, this.currentKey) && this.isPressed) {
        e.preventDefault();
        e.stopPropagation();
        this.isPressed = false;
        this.notifyListeners(false);
      }
    };

    // Listen on document to catch global key events
    document.addEventListener('keydown', this.keydownHandler, true);
    document.addEventListener('keyup', this.keyupHandler, true);
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
    // Handle special key names
    const keyMap: Record<string, string> = {
      'Space': ' ',
      'Enter': 'Enter',
      'Escape': 'Escape',
      'Tab': 'Tab',
      'Backspace': 'Backspace',
      'Delete': 'Delete',
      'ArrowUp': 'ArrowUp',
      'ArrowDown': 'ArrowDown',
      'ArrowLeft': 'ArrowLeft',
      'ArrowRight': 'ArrowRight'
    };

    const expectedKey = keyMap[keyName] || keyName;
    
    // Check for modifier keys
    if (keyName.includes('+')) {
      const parts = keyName.split('+').map(p => p.trim());
      const modifiers = parts.slice(0, -1);
      const key = parts[parts.length - 1];
      
      for (const modifier of modifiers) {
        switch (modifier.toLowerCase()) {
          case 'ctrl':
            if (!event.ctrlKey) return false;
            break;
          case 'alt':
            if (!event.altKey) return false;
            break;
          case 'shift':
            if (!event.shiftKey) return false;
            break;
          case 'meta':
            if (!event.metaKey) return false;
            break;
        }
      }
      
      return event.key === (keyMap[key] || key);
    }

    return event.key === expectedKey;
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
    this.listeners = [];
  }

  /**
   * Get a human-readable key name for display
   */
  static getKeyDisplayName(key: string): string {
    const displayNames: Record<string, string> = {
      ' ': 'Space',
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

    return displayNames[key] || key.toUpperCase();
  }

  /**
   * Capture the next key press for key binding
   */
  captureKey(): Promise<string> {
    return new Promise((resolve) => {
      const captureHandler = (e: KeyboardEvent) => {
        e.preventDefault();
        e.stopPropagation();
        
        let keyName = e.key;
        
        // Build modifier string
        const modifiers: string[] = [];
        if (e.ctrlKey) modifiers.push('Ctrl');
        if (e.altKey) modifiers.push('Alt');
        if (e.shiftKey) modifiers.push('Shift');
        if (e.metaKey) modifiers.push('Meta');
        
        // Special key handling
        if (keyName === ' ') keyName = 'Space';
        
        const fullKey = modifiers.length > 0 
          ? `${modifiers.join('+')}+${keyName}`
          : keyName;
        
        document.removeEventListener('keydown', captureHandler, true);
        resolve(fullKey);
      };

      document.addEventListener('keydown', captureHandler, true);
    });
  }
}