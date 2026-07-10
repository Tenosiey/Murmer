import { writable, get } from 'svelte/store';

export type Theme = 'dark' | 'light';

const STORAGE_KEY = 'murmer-theme';
const ACCENT_KEY = 'murmer-accent';

function applyTheme(value: Theme) {
  if (typeof document !== 'undefined') {
    document.documentElement.dataset.theme = value;
  }
}

function createThemeStore() {
  const store = writable<Theme>('dark');

  return {
    subscribe: store.subscribe,
    set: (value: Theme) => {
      store.set(value);
      if (typeof localStorage !== 'undefined') {
        localStorage.setItem(STORAGE_KEY, value);
      }
      applyTheme(value);
    },
    toggle: () => {
      const nextValue: Theme = get(store) === 'dark' ? 'light' : 'dark';
      if (typeof localStorage !== 'undefined') {
        localStorage.setItem(STORAGE_KEY, nextValue);
      }
      store.set(nextValue);
      applyTheme(nextValue);
    },
    init: () => {
      if (typeof window === 'undefined') return;
      const stored = localStorage.getItem(STORAGE_KEY) as Theme | null;
      const initial: Theme = stored ?? (window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light');
      store.set(initial);
      applyTheme(initial);
    }
  };
}

export const theme = createThemeStore();

/* ---- Accent color (theme color wheel) ----------------------------------
   A point on the hue/saturation wheel. When set, the whole palette is
   re-derived from it: the accent tokens take the picked hue directly and
   the surface/text tokens keep their hand-tuned lightness but shift to the
   picked hue, so the entire UI is tinted (Zen-Browser-style theming).
   `null` means the built-in palette from +layout.svelte is used untouched. */

export interface Accent {
  /** Hue in degrees, 0–360, measured clockwise from the top of the wheel. */
  hue: number;
  /** Saturation 0–100 (distance from the wheel's center). */
  saturation: number;
}

/** Wheel position matching the built-in cyan accent (#27c0e8). */
export const DEFAULT_ACCENT: Accent = { hue: 193, saturation: 78 };

/* Saturation/lightness pairs mirroring the handcrafted palettes in
   +layout.svelte. The saturation column is the value at the default accent
   strength; it scales with the picked saturation so a muted accent yields
   equally muted surfaces. */
type PaletteShape = Record<string, [saturation: number, lightness: number]>;

const DARK_SHAPE: PaletteShape = {
  '--color-bg': [29, 6],
  '--color-surface': [26, 9],
  '--color-surface-elevated': [25, 12],
  '--color-surface-raised': [22, 16],
  '--color-surface-outline': [22, 19],
  '--color-outline-strong': [20, 27],
  '--color-on-surface': [35, 93],
  '--color-on-surface-variant': [19, 75],
  '--color-muted': [15, 60],
  '--color-primary': [78, 53],
  '--color-on-primary': [76, 10],
  '--color-primary-container': [71, 18]
};

const LIGHT_SHAPE: PaletteShape = {
  '--color-bg': [31, 95],
  '--color-surface': [45, 98],
  '--color-surface-elevated': [0, 100],
  '--color-surface-raised': [35, 94],
  '--color-surface-outline': [27, 88],
  '--color-outline-strong': [22, 77],
  '--color-on-surface': [25, 12],
  '--color-on-surface-variant': [18, 29],
  '--color-muted': [13, 42],
  '--color-primary': [87, 34],
  '--color-on-primary': [0, 100],
  '--color-primary-container': [74, 89]
};

const THEMED_TOKENS = Object.keys(DARK_SHAPE);

function applyAccent(mode: Theme, value: Accent | null) {
  if (typeof document === 'undefined') return;
  const style = document.documentElement.style;
  if (!value) {
    for (const token of THEMED_TOKENS) style.removeProperty(token);
    return;
  }
  const shape = mode === 'dark' ? DARK_SHAPE : LIGHT_SHAPE;
  const factor = value.saturation / DEFAULT_ACCENT.saturation;
  for (const [token, [s, l]] of Object.entries(shape)) {
    style.setProperty(token, `hsl(${Math.round(value.hue)} ${Math.round(Math.min(100, s * factor))}% ${l}%)`);
  }
}

function loadAccent(): Accent | null {
  if (typeof localStorage === 'undefined') return null;
  try {
    const raw = localStorage.getItem(ACCENT_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    if (typeof parsed?.hue === 'number' && typeof parsed?.saturation === 'number') {
      return {
        hue: ((parsed.hue % 360) + 360) % 360,
        saturation: Math.max(0, Math.min(100, parsed.saturation))
      };
    }
  } catch (e) {
    console.error('Failed to parse stored accent color', e);
  }
  return null;
}

function createAccentStore() {
  const store = writable<Accent | null>(loadAccent());

  return {
    subscribe: store.subscribe,
    set: (value: Accent) => {
      store.set(value);
      if (typeof localStorage !== 'undefined') {
        localStorage.setItem(ACCENT_KEY, JSON.stringify(value));
      }
    },
    reset: () => {
      store.set(null);
      if (typeof localStorage !== 'undefined') {
        localStorage.removeItem(ACCENT_KEY);
      }
    }
  };
}

export const accent = createAccentStore();

// Re-derive the palette whenever the mode or the accent changes.
theme.subscribe((mode) => applyAccent(mode, get(accent)));
accent.subscribe((value) => applyAccent(get(theme), value));
