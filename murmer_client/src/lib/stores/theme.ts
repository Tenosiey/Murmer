import { writable, get } from 'svelte/store';
import { assets } from '$app/paths';

export type Theme = 'dark' | 'light';

const STORAGE_KEY = 'murmer-theme';
const ACCENT_KEY = 'murmer-accent';

const isTauri = typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

/**
 * Points the favicon at the matching logo variant. The in-app logo
 * (`MurmerLogo.svelte`) needs no help here — it reads the `--color-brand-*`
 * tokens, which the `data-theme` attribute below already switches. Only the
 * favicon is outside the document's styling reach.
 */
function applyFavicon(value: Theme) {
  if (typeof document === 'undefined') return;
  const link = document.querySelector<HTMLLinkElement>('link[rel="icon"]');
  // `assets` mirrors the `%sveltekit.assets%` prefix app.html uses for the
  // initial href, so both agree if a base path is ever configured.
  if (link) link.href = `${assets}/logo/murmer-${value}.svg`;
}

/**
 * Swaps the system tray icon to match. No-op outside the Tauri shell, and
 * failures are non-fatal: a stale tray icon must never break theming.
 */
function applyTrayIcon(value: Theme) {
  if (!isTauri) return;
  import('@tauri-apps/api/core')
    .then(({ invoke }) => invoke('set_tray_theme', { theme: value }))
    .catch((e) => console.error('Failed to update tray icon', e));
}

function applyTheme(value: Theme) {
  if (typeof document !== 'undefined') {
    document.documentElement.dataset.theme = value;
  }
  applyFavicon(value);
  applyTrayIcon(value);
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

/** Wheel position matching the built-in lime accent from the logo (#b0e52a).
    The built-in palette in +layout.svelte is this position rendered through
    the shape tables below, so picking it changes nothing — keep them in sync. */
export const DEFAULT_ACCENT: Accent = { hue: 77, saturation: 78 };

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

/* The wheel only carries hue and saturation, so an accent is rendered as a
   hex code at a fixed 50% lightness — the same level the preset swatches
   use. A hex typed by the user is read back the same way: its hue and
   saturation are kept, its lightness is dropped, because the palette above
   supplies the lightness for every token. */
const HEX_LIGHTNESS = 50;

/** Renders a wheel position as `#rrggbb`. */
export function accentToHex({ hue, saturation }: Accent): string {
  const s = saturation / 100;
  const l = HEX_LIGHTNESS / 100;
  const chroma = (1 - Math.abs(2 * l - 1)) * s;
  const channel = (n: number) => {
    const k = (n + hue / 30) % 12;
    const value = l - (chroma / 2) * Math.max(-1, Math.min(k - 3, 9 - k, 1));
    return Math.round(value * 255)
      .toString(16)
      .padStart(2, '0');
  };
  return `#${channel(0)}${channel(8)}${channel(4)}`;
}

/**
 * Parses `#rgb`/`#rrggbb` (with or without the hash) into a wheel position.
 * Returns null for anything else so callers can reject invalid input
 * instead of applying a garbage palette.
 */
export function hexToAccent(input: string): Accent | null {
  const raw = input.trim().replace(/^#/, '');
  const expanded =
    raw.length === 3
      ? raw
          .split('')
          .map((c) => c + c)
          .join('')
      : raw;
  if (!/^[0-9a-fA-F]{6}$/.test(expanded)) return null;
  const r = parseInt(expanded.slice(0, 2), 16) / 255;
  const g = parseInt(expanded.slice(2, 4), 16) / 255;
  const b = parseInt(expanded.slice(4, 6), 16) / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const delta = max - min;
  const l = (max + min) / 2;
  if (delta === 0) return { hue: 0, saturation: 0 };
  const saturation = delta / (1 - Math.abs(2 * l - 1));
  let hue: number;
  if (max === r) hue = ((g - b) / delta) % 6;
  else if (max === g) hue = (b - r) / delta + 2;
  else hue = (r - g) / delta + 4;
  hue = (hue * 60 + 360) % 360;
  return { hue: Math.round(hue), saturation: Math.round(Math.min(1, saturation) * 100) };
}

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
