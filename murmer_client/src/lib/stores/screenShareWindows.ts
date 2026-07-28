import { browser } from '$app/environment';
import { get, writable } from 'svelte/store';

/**
 * Geometry of the floating screen-share windows.
 *
 * Watching a share never takes the app over: every share is its own window
 * floating above the chat, freely movable and resizable, so a second share can
 * be opened next to the first one and the app stays usable underneath.
 *
 * Three modes, all reversible:
 *  - `normal`     – wherever the user put it
 *  - `pip`        – shrunk into a corner to keep half an eye on it
 *  - `maximized`  – filling the viewport (still just a window, not a modal)
 *
 * `pip`/`maximized` remember the geometry they came from in `restore`, so
 * leaving them puts the window back exactly where it was.
 *
 * The store only ever deals in numbers and is handed the viewport by its
 * caller — it never touches `window`, which keeps every clamping rule testable
 * and lets the layer decide what "the viewport" means.
 */

export type ScreenShareWindowMode = 'normal' | 'pip' | 'maximized';

export interface Rect {
  x: number;
  y: number;
  width: number;
  height: number;
}

export interface Viewport {
  width: number;
  height: number;
}

export interface ScreenShareWindowState extends Rect {
  mode: ScreenShareWindowMode;
  /** Geometry to return to when leaving `pip`/`maximized`. */
  restore: Rect | null;
  /** Stacking order; the highest window is the one in front. */
  z: number;
}

/** Height of the window header, so "fit to the stream" can size the video. */
export const WINDOW_HEADER_HEIGHT = 38;

/** Below this a share is unreadable and the window unusable. */
const MIN_WIDTH = 280;
const MIN_HEIGHT = 160;
/** Gap kept to the viewport edges so a window never sits flush in a corner. */
const MARGIN = 12;
/** Width of a picture-in-picture window: small, but still legible. */
const PIP_WIDTH = 320;
/** Offset between windows opened after one another. */
const CASCADE_STEP = 28;
const DEFAULT_ASPECT = 16 / 9;

const STORAGE_KEY = 'murmer_screenshare_windows';

type WindowMap = Record<string, ScreenShareWindowState>;

/** What survives a restart: everything but the stacking order. */
type PersistedWindow = Rect & {
  mode: ScreenShareWindowMode;
  restore: Rect | null;
};

/**
 * serverUrl -> windowKey -> geometry. The keys carry account names, which are
 * only unique per server, so the persisted state has to be namespaced or two
 * servers' Alices share one window.
 */
type PersistedState = Record<string, Record<string, PersistedWindow>>;

function isFiniteNumber(value: unknown): value is number {
  return typeof value === 'number' && Number.isFinite(value);
}

function parseRect(value: unknown): Rect | null {
  if (!value || typeof value !== 'object') return null;
  const raw = value as Record<string, unknown>;
  if (!isFiniteNumber(raw.x) || !isFiniteNumber(raw.y)) return null;
  if (!isFiniteNumber(raw.width) || !isFiniteNumber(raw.height)) return null;
  return { x: raw.x, y: raw.y, width: raw.width, height: raw.height };
}

function parseMode(value: unknown): ScreenShareWindowMode {
  return value === 'pip' || value === 'maximized' ? value : 'normal';
}

function parseWindow(value: unknown): PersistedWindow | null {
  const rect = parseRect(value);
  if (!rect) return null;
  const raw = value as Record<string, unknown>;
  return { ...rect, mode: parseMode(raw.mode), restore: parseRect(raw.restore) };
}

function loadPersisted(): PersistedState {
  if (!browser) return {};
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return {};
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    if (!parsed || typeof parsed !== 'object') return {};
    const result: PersistedState = {};
    for (const [server, windows] of Object.entries(parsed)) {
      if (!windows || typeof windows !== 'object') continue;
      const slice: Record<string, PersistedWindow> = {};
      for (const [key, entry] of Object.entries(windows as Record<string, unknown>)) {
        const parsedWindow = parseWindow(entry);
        if (parsedWindow) slice[key] = parsedWindow;
      }
      result[server] = slice;
    }
    return result;
  } catch (error) {
    console.error('Failed to parse screen share window layout', error);
    return {};
  }
}

/** Keep a window inside the viewport, at a size that can still be read. */
export function clampRect(rect: Rect, viewport: Viewport): Rect {
  const maxWidth = Math.max(MIN_WIDTH, viewport.width - MARGIN * 2);
  const maxHeight = Math.max(MIN_HEIGHT, viewport.height - MARGIN * 2);
  const width = Math.round(Math.min(Math.max(rect.width, MIN_WIDTH), maxWidth));
  const height = Math.round(Math.min(Math.max(rect.height, MIN_HEIGHT), maxHeight));
  // A window is never allowed off screen: a header dragged past the edge can
  // never be grabbed again, and the whole point is that it stays reachable.
  const maxX = Math.max(MARGIN, viewport.width - width - MARGIN);
  const maxY = Math.max(MARGIN, viewport.height - height - MARGIN);
  return {
    width,
    height,
    x: Math.round(Math.min(Math.max(rect.x, MARGIN), maxX)),
    y: Math.round(Math.min(Math.max(rect.y, MARGIN), maxY))
  };
}

function maximizedRect(viewport: Viewport): Rect {
  return clampRect(
    {
      x: MARGIN,
      y: MARGIN,
      width: viewport.width - MARGIN * 2,
      height: viewport.height - MARGIN * 2
    },
    viewport
  );
}

/** A small window parked in the bottom-right corner. */
function pipRect(viewport: Viewport, aspect: number): Rect {
  const width = Math.min(PIP_WIDTH, Math.max(MIN_WIDTH, viewport.width - MARGIN * 2));
  const height = Math.round(width / aspect) + WINDOW_HEADER_HEIGHT;
  return clampRect(
    { x: viewport.width - width - MARGIN, y: viewport.height - height - MARGIN, width, height },
    viewport
  );
}

/** Where the `index`-th window of a session opens: centred, then cascaded. */
function defaultRect(viewport: Viewport, index: number, aspect: number): Rect {
  const width = Math.round(Math.min(Math.max(viewport.width * 0.42, 480), 960));
  const height = Math.round(width / aspect) + WINDOW_HEADER_HEIGHT;
  const step = CASCADE_STEP * (index % 6);
  return clampRect(
    {
      x: Math.round((viewport.width - width) / 2) + step,
      y: Math.round((viewport.height - height) / 2) + step,
      width,
      height
    },
    viewport
  );
}

/** The geometry a mode implies, given where the window currently sits. */
function rectForMode(
  mode: ScreenShareWindowMode,
  fallback: Rect,
  viewport: Viewport,
  aspect: number
): Rect {
  if (mode === 'maximized') return maximizedRect(viewport);
  if (mode === 'pip') return pipRect(viewport, aspect);
  return clampRect(fallback, viewport);
}

function createScreenShareWindows() {
  const store = writable<WindowMap>({});
  const persisted = loadPersisted();
  let activeServer: string | null = null;

  function save() {
    if (!browser || !activeServer) return;
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(persisted));
    } catch (error) {
      console.error('Failed to persist screen share window layout', error);
    }
  }

  // Windows are remembered per key even after they are closed, so reopening
  // someone's share puts it back where the user last had it. Closed entries
  // are therefore never removed from the persisted slice.
  store.subscribe((map) => {
    if (!activeServer) return;
    const slice = persisted[activeServer] ?? {};
    for (const [key, state] of Object.entries(map)) {
      slice[key] = {
        x: state.x,
        y: state.y,
        width: state.width,
        height: state.height,
        mode: state.mode,
        restore: state.restore
      };
    }
    persisted[activeServer] = slice;
    save();
  });

  /** Compact the stacking order and put `key` in front. */
  function raiseIn(map: WindowMap, key: string): WindowMap {
    const order = Object.keys(map)
      .filter((other) => other !== key)
      .sort((a, b) => map[a].z - map[b].z);
    const next: WindowMap = {};
    order.forEach((other, index) => {
      next[other] = { ...map[other], z: index + 1 };
    });
    if (map[key]) next[key] = { ...map[key], z: order.length + 1 };
    return next;
  }

  function update(key: string, viewport: Viewport, change: (state: ScreenShareWindowState) => Rect) {
    store.update((map) => {
      const state = map[key];
      if (!state) return map;
      const rect = clampRect(change(state), viewport);
      return { ...map, [key]: { ...state, ...rect } };
    });
  }

  return {
    subscribe: store.subscribe,

    /** Switch to the given server's remembered layout. Called on connect. */
    setServer(url: string) {
      activeServer = url;
      store.set({});
    },

    /**
     * Bring the set of open windows in line with the shares being watched:
     * new keys get a window (restored from the remembered layout when there is
     * one), keys that are gone lose theirs.
     */
    sync(
      entries: Array<{ key: string; preferredMode?: ScreenShareWindowMode; aspect?: number }>,
      viewport: Viewport
    ) {
      store.update((map) => {
        const wanted = new Set(entries.map((entry) => entry.key));
        let next: WindowMap = {};
        for (const [key, state] of Object.entries(map)) {
          if (wanted.has(key)) next[key] = state;
        }
        let opened = Object.keys(next).length;
        for (const entry of entries) {
          if (next[entry.key]) continue;
          const aspect = entry.aspect ?? DEFAULT_ASPECT;
          const remembered = activeServer ? persisted[activeServer]?.[entry.key] : undefined;
          const mode = remembered?.mode ?? entry.preferredMode ?? 'normal';
          const fallback = remembered ?? defaultRect(viewport, opened, aspect);
          next[entry.key] = {
            ...rectForMode(mode, fallback, viewport, aspect),
            mode,
            restore: remembered?.restore ?? null,
            z: opened + 1
          };
          // A share that just opened is the one the user wants to look at.
          next = raiseIn(next, entry.key);
          opened += 1;
        }
        return next;
      });
    },

    /** Put a window in front of the others. */
    raise(key: string) {
      store.update((map) => (map[key] ? raiseIn(map, key) : map));
    },

    move(key: string, x: number, y: number, viewport: Viewport) {
      update(key, viewport, (state) => ({ ...state, x, y }));
    },

    /** Resize (and, for the top/left handles, reposition) a window. */
    resize(key: string, rect: Rect, viewport: Viewport) {
      update(key, viewport, () => rect);
    },

    /** Size the window so the video fills it at the stream's aspect ratio. */
    fitAspect(key: string, aspect: number, viewport: Viewport) {
      update(key, viewport, (state) => ({
        ...state,
        height: Math.round(state.width / aspect) + WINDOW_HEADER_HEIGHT
      }));
    },

    /**
     * Switch a window between `normal` and `pip`/`maximized`; asking for the
     * mode it is already in returns it to where it came from, which is what
     * makes every one of these buttons a toggle.
     */
    toggleMode(
      key: string,
      mode: ScreenShareWindowMode,
      viewport: Viewport,
      aspect = DEFAULT_ASPECT
    ) {
      store.update((map) => {
        const state = map[key];
        if (!state) return map;
        const target = state.mode === mode ? 'normal' : mode;
        if (target === 'normal') {
          const restore = state.restore ?? { x: state.x, y: state.y, width: state.width, height: state.height };
          return { ...map, [key]: { ...state, ...clampRect(restore, viewport), mode: 'normal', restore: null } };
        }
        // Coming from another special mode, the geometry to return to is still
        // the one from before that mode — not the pip/maximized box.
        const restore =
          state.restore ?? { x: state.x, y: state.y, width: state.width, height: state.height };
        return {
          ...map,
          [key]: { ...state, ...rectForMode(target, restore, viewport, aspect), mode: target, restore }
        };
      });
    },

    /** Arrange every open window in a grid so they can be watched at once. */
    tileAll(viewport: Viewport) {
      store.update((map) => {
        const keys = Object.keys(map).sort((a, b) => map[a].z - map[b].z);
        if (keys.length === 0) return map;
        const columns = Math.ceil(Math.sqrt(keys.length));
        const rows = Math.ceil(keys.length / columns);
        const cellWidth = (viewport.width - MARGIN * (columns + 1)) / columns;
        const cellHeight = (viewport.height - MARGIN * (rows + 1)) / rows;
        const next: WindowMap = {};
        keys.forEach((key, index) => {
          const column = index % columns;
          const row = Math.floor(index / columns);
          const rect = clampRect(
            {
              x: MARGIN + column * (cellWidth + MARGIN),
              y: MARGIN + row * (cellHeight + MARGIN),
              width: cellWidth,
              height: cellHeight
            },
            viewport
          );
          next[key] = { ...map[key], ...rect, mode: 'normal', restore: null };
        });
        return next;
      });
    },

    /** Pull every window back inside the viewport after the app was resized. */
    clampAll(viewport: Viewport) {
      store.update((map) => {
        const next: WindowMap = {};
        for (const [key, state] of Object.entries(map)) {
          const rect =
            state.mode === 'maximized'
              ? maximizedRect(viewport)
              : clampRect({ x: state.x, y: state.y, width: state.width, height: state.height }, viewport);
          next[key] = { ...state, ...rect };
        }
        return next;
      });
    },

    /** The current state of one window, for callers outside a subscription. */
    get(key: string): ScreenShareWindowState | undefined {
      return get(store)[key];
    }
  };
}

export const screenShareWindows = createScreenShareWindows();
