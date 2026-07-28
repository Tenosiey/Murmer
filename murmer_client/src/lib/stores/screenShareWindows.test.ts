import { beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';

/** Fresh module instance: the layout is remembered inside the store. */
async function loadWindows() {
  vi.resetModules();
  return import('./screenShareWindows');
}

const VIEWPORT = { width: 1280, height: 800 };

beforeEach(() => {
  localStorage.clear();
});

describe('screenShareWindows — open windows follow the watched shares', () => {
  it('opens one window per share and drops the ones that ended', async () => {
    const { screenShareWindows } = await loadWindows();

    screenShareWindows.sync([{ key: 'peer:alice' }, { key: 'peer:bob' }], VIEWPORT);
    expect(Object.keys(get(screenShareWindows))).toEqual(['peer:alice', 'peer:bob']);

    screenShareWindows.sync([{ key: 'peer:bob' }], VIEWPORT);
    expect(Object.keys(get(screenShareWindows))).toEqual(['peer:bob']);
  });

  it('leaves an existing window untouched when another share opens', async () => {
    const { screenShareWindows } = await loadWindows();

    screenShareWindows.sync([{ key: 'peer:alice' }], VIEWPORT);
    screenShareWindows.move('peer:alice', 40, 60, VIEWPORT);
    screenShareWindows.sync([{ key: 'peer:alice' }, { key: 'peer:bob' }], VIEWPORT);

    const alice = get(screenShareWindows)['peer:alice'];
    expect([alice.x, alice.y]).toEqual([40, 60]);
    // The share that just opened is the one to look at, so it comes out on top.
    expect(get(screenShareWindows)['peer:bob'].z).toBeGreaterThan(alice.z);
  });

  it('puts a raised window in front without letting the stack run away', async () => {
    const { screenShareWindows } = await loadWindows();

    screenShareWindows.sync([{ key: 'peer:alice' }, { key: 'peer:bob' }], VIEWPORT);
    screenShareWindows.raise('peer:alice');

    const windows = get(screenShareWindows);
    expect(windows['peer:alice'].z).toBeGreaterThan(windows['peer:bob'].z);
    // The order is compacted on every raise: two windows are always 1 and 2,
    // however often they are clicked, so the values can never reach the app's
    // overlay levels.
    expect([windows['peer:alice'].z, windows['peer:bob'].z].sort()).toEqual([1, 2]);
  });
});

describe('screenShareWindows — staying on screen', () => {
  it('keeps a window dragged past the edge inside the viewport', async () => {
    const { screenShareWindows } = await loadWindows();

    screenShareWindows.sync([{ key: 'peer:alice' }], VIEWPORT);
    screenShareWindows.move('peer:alice', 5000, -500, VIEWPORT);

    const alice = get(screenShareWindows)['peer:alice'];
    expect(alice.x + alice.width).toBeLessThanOrEqual(VIEWPORT.width);
    expect(alice.y).toBeGreaterThanOrEqual(0);
  });

  it('pulls every window back in when the app window shrinks', async () => {
    const { screenShareWindows } = await loadWindows();

    screenShareWindows.sync([{ key: 'peer:alice' }], VIEWPORT);
    screenShareWindows.clampAll({ width: 640, height: 480 });

    const alice = get(screenShareWindows)['peer:alice'];
    expect(alice.x + alice.width).toBeLessThanOrEqual(640);
    expect(alice.y + alice.height).toBeLessThanOrEqual(480);
  });

  it('refuses to resize a window below the size it can still be used at', async () => {
    const { screenShareWindows } = await loadWindows();

    screenShareWindows.sync([{ key: 'peer:alice' }], VIEWPORT);
    screenShareWindows.resize('peer:alice', { x: 100, y: 100, width: 10, height: 10 }, VIEWPORT);

    const alice = get(screenShareWindows)['peer:alice'];
    expect(alice.width).toBeGreaterThanOrEqual(280);
    expect(alice.height).toBeGreaterThanOrEqual(160);
  });
});

describe('screenShareWindows — picture-in-picture and maximize', () => {
  it('returns a window to where it came from', async () => {
    const { screenShareWindows } = await loadWindows();

    screenShareWindows.sync([{ key: 'peer:alice' }], VIEWPORT);
    screenShareWindows.resize('peer:alice', { x: 200, y: 120, width: 700, height: 420 }, VIEWPORT);

    screenShareWindows.toggleMode('peer:alice', 'pip', VIEWPORT);
    expect(get(screenShareWindows)['peer:alice'].width).toBeLessThan(700);

    screenShareWindows.toggleMode('peer:alice', 'pip', VIEWPORT);
    const alice = get(screenShareWindows)['peer:alice'];
    expect([alice.x, alice.y, alice.width, alice.height]).toEqual([200, 120, 700, 420]);
    expect(alice.mode).toBe('normal');
  });

  it('remembers the original geometry across a second mode', async () => {
    const { screenShareWindows } = await loadWindows();

    screenShareWindows.sync([{ key: 'peer:alice' }], VIEWPORT);
    screenShareWindows.resize('peer:alice', { x: 200, y: 120, width: 700, height: 420 }, VIEWPORT);

    // pip -> maximized -> back: the geometry to restore is the one from before
    // the *first* mode, never the pip box that was on screen in between.
    screenShareWindows.toggleMode('peer:alice', 'pip', VIEWPORT);
    screenShareWindows.toggleMode('peer:alice', 'maximized', VIEWPORT);
    screenShareWindows.toggleMode('peer:alice', 'maximized', VIEWPORT);

    const alice = get(screenShareWindows)['peer:alice'];
    expect([alice.x, alice.y, alice.width, alice.height]).toEqual([200, 120, 700, 420]);
  });

  it('tiles every window into the viewport', async () => {
    const { screenShareWindows } = await loadWindows();

    screenShareWindows.sync([{ key: 'peer:alice' }, { key: 'peer:bob' }], VIEWPORT);
    screenShareWindows.toggleMode('peer:alice', 'pip', VIEWPORT);
    screenShareWindows.tileAll(VIEWPORT);

    for (const state of Object.values(get(screenShareWindows))) {
      // Tiling is what "watch both at once" means, so it also takes a window
      // out of pip — a tiled corner thumbnail would defeat the arrangement.
      expect(state.mode).toBe('normal');
      expect(state.x + state.width).toBeLessThanOrEqual(VIEWPORT.width);
      expect(state.y + state.height).toBeLessThanOrEqual(VIEWPORT.height);
    }
    const [alice, bob] = ['peer:alice', 'peer:bob'].map((key) => get(screenShareWindows)[key]);
    expect(alice.x + alice.width).toBeLessThanOrEqual(bob.x);
  });
});

describe('screenShareWindows — remembered layout', () => {
  it('restores a window where the user last had it', async () => {
    const { screenShareWindows } = await loadWindows();
    screenShareWindows.setServer('wss://one.example');

    screenShareWindows.sync([{ key: 'peer:alice' }], VIEWPORT);
    screenShareWindows.move('peer:alice', 64, 48, VIEWPORT);
    // Alice stops sharing…
    screenShareWindows.sync([], VIEWPORT);
    // …and starts again later.
    screenShareWindows.sync([{ key: 'peer:alice' }], VIEWPORT);

    const alice = get(screenShareWindows)['peer:alice'];
    expect([alice.x, alice.y]).toEqual([64, 48]);
  });

  it('survives a restart', async () => {
    const first = await loadWindows();
    first.screenShareWindows.setServer('wss://one.example');
    first.screenShareWindows.sync([{ key: 'peer:alice' }], VIEWPORT);
    first.screenShareWindows.toggleMode('peer:alice', 'pip', VIEWPORT);

    const second = await loadWindows();
    second.screenShareWindows.setServer('wss://one.example');
    second.screenShareWindows.sync([{ key: 'peer:alice' }], VIEWPORT);

    expect(get(second.screenShareWindows)['peer:alice'].mode).toBe('pip');
  });

  it('keeps the two servers’ layouts apart', async () => {
    const { screenShareWindows } = await loadWindows();

    // Account names are only unique per server: the same key on another server
    // is a different person and must not inherit their window.
    screenShareWindows.setServer('wss://one.example');
    screenShareWindows.sync([{ key: 'peer:alice' }], VIEWPORT);
    screenShareWindows.move('peer:alice', 64, 48, VIEWPORT);

    screenShareWindows.setServer('wss://two.example');
    screenShareWindows.sync([{ key: 'peer:alice' }], VIEWPORT);

    const alice = get(screenShareWindows)['peer:alice'];
    expect([alice.x, alice.y]).not.toEqual([64, 48]);
  });

  it('drops a malformed layout instead of opening windows off screen', async () => {
    localStorage.setItem(
      'murmer_screenshare_windows',
      JSON.stringify({ 'wss://one.example': { 'peer:alice': { x: 'left', y: null } } })
    );
    const { screenShareWindows } = await loadWindows();
    screenShareWindows.setServer('wss://one.example');
    screenShareWindows.sync([{ key: 'peer:alice' }], VIEWPORT);

    const alice = get(screenShareWindows)['peer:alice'];
    expect(Number.isFinite(alice.x)).toBe(true);
    expect(Number.isFinite(alice.width)).toBe(true);
  });
});
