import { beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';
import type { ScreenSharePeer } from '../types';

/**
 * Handles on the mocked collaborators. `vi.mock` factories are hoisted above
 * the imports, so the shared state they capture has to be hoisted with them.
 */
const mocks = vi.hoisted(() => ({
  /** Frame handlers the store registered with `chat.on`. */
  chatHandlers: new Map<string, (msg: unknown) => void>(),
  /** Peer-list subscribers the store registered with the manager. */
  peerListeners: [] as Array<(peers: ScreenSharePeer[]) => void>,
  viewed: [] as string[],
  stoppedViewing: [] as string[]
}));

vi.mock('./chat', () => ({
  chat: {
    on: (type: string, cb: (msg: unknown) => void) => mocks.chatHandlers.set(type, cb),
    off: () => {},
    sendRaw: () => {}
  }
}));

// The real manager builds `RTCPeerConnection`s, which a plain Node run has no
// implementation for; the store only ever talks to it through this surface.
vi.mock('../screenshare/manager', () => ({
  ScreenShareManager: class {
    subscribe(cb: (peers: ScreenSharePeer[]) => void) {
      mocks.peerListeners.push(cb);
      return () => {};
    }
    subscribeLocalStream(cb: (stream: MediaStream | null) => void) {
      cb(null);
      return () => {};
    }
    async viewScreenShare(userId: string) {
      mocks.viewed.push(userId);
    }
    stopViewing(userId: string) {
      mocks.stoppedViewing.push(userId);
    }
    leaveAsViewer() {}
    stopSharing() {}
    setServerMaxBitrate() {}
    async startSharing() {}
    updateSettings() {}
    destroy() {}
  },
  QUALITY_PRESETS: {}
}));

/** Fresh module instance: the store wires itself up at import time. */
async function loadScreenShare() {
  vi.resetModules();
  mocks.chatHandlers.clear();
  mocks.peerListeners.length = 0;
  mocks.viewed.length = 0;
  mocks.stoppedViewing.length = 0;
  return import('./screenShare');
}

/** Publish a peer list the way the manager does after a track arrives. */
function emitPeers(users: string[], reconnecting: string[] = []) {
  const peers = users.map((userId) => ({
    userId,
    stream: {} as MediaStream,
    hasAudio: false,
    reconnecting: reconnecting.includes(userId)
  }));
  for (const listener of mocks.peerListeners) listener(peers);
}

/** Deliver a server frame to the handler the store registered for it. */
function deliver(type: string, msg: Record<string, unknown>) {
  mocks.chatHandlers.get(type)?.(msg);
}

beforeEach(() => {
  localStorage.clear();
});

describe('screenShare — watching several shares at once', () => {
  it('keeps one entry per share, in the order they were opened', async () => {
    const { watchedScreenShares, openScreenShare } = await loadScreenShare();

    await openScreenShare('alice', 'me', 1);
    await openScreenShare('bob', 'me', 1);

    expect(get(watchedScreenShares)).toEqual(['alice', 'bob']);
    // Each share is its own peer connection; neither replaces the other.
    expect(mocks.viewed).toEqual(['alice', 'bob']);
  });

  it('ignores a repeated open instead of listing the share twice', async () => {
    const { watchedScreenShares, openScreenShare } = await loadScreenShare();

    await openScreenShare('alice', 'me', 1);
    await openScreenShare('alice', 'me', 1);

    expect(get(watchedScreenShares)).toEqual(['alice']);
  });

  it('closes one share without touching the others', async () => {
    const { watchedScreenShares, openScreenShare, closeScreenShare } = await loadScreenShare();

    await openScreenShare('alice', 'me', 1);
    await openScreenShare('bob', 'me', 1);
    emitPeers(['alice', 'bob']);

    closeScreenShare('alice');

    expect(get(watchedScreenShares)).toEqual(['bob']);
    // Only alice's connection is torn down — bob keeps streaming.
    expect(mocks.stoppedViewing).toEqual(['alice']);
  });

  it('closes every share on demand', async () => {
    const { watchedScreenShares, openScreenShare, closeAllScreenShares } = await loadScreenShare();

    await openScreenShare('alice', 'me', 1);
    await openScreenShare('bob', 'me', 1);
    closeAllScreenShares();

    expect(get(watchedScreenShares)).toEqual([]);
    expect(mocks.stoppedViewing).toEqual(['alice', 'bob']);
  });
});

describe('screenShare — shares that end', () => {
  it('drops only the share whose stream disappeared', async () => {
    const { watchedScreenShares, openScreenShare } = await loadScreenShare();

    await openScreenShare('alice', 'me', 1);
    await openScreenShare('bob', 'me', 1);
    emitPeers(['alice', 'bob']);
    emitPeers(['bob']);

    expect(get(watchedScreenShares)).toEqual(['bob']);
    expect(mocks.stoppedViewing).toEqual(['alice']);
  });

  it('leaves a share that has not connected yet on the stage', async () => {
    const { watchedScreenShares, openScreenShare } = await loadScreenShare();

    await openScreenShare('alice', 'me', 1);
    // Negotiation is still running: no peer yet, which must not be read as
    // "the share ended".
    emitPeers([]);

    expect(get(watchedScreenShares)).toEqual(['alice']);
    expect(mocks.stoppedViewing).toEqual([]);
  });

  it('keeps a share whose connection is being repaired on the stage', async () => {
    const { watchedScreenShares, screenSharePeers, openScreenShare } = await loadScreenShare();

    await openScreenShare('alice', 'me', 1);
    await openScreenShare('bob', 'me', 1);
    emitPeers(['alice', 'bob']);
    // Alice's connection broke. The manager keeps reporting her (see
    // `ScreenShareManager.getPeersList`) precisely so this reconciliation does
    // not read a repair as a share that ended and close her window — the
    // coupling is easy to break from either side.
    emitPeers(['alice', 'bob'], ['alice']);

    expect(get(watchedScreenShares)).toEqual(['alice', 'bob']);
    expect(mocks.stoppedViewing).toEqual([]);
    expect(get(screenSharePeers).find((p) => p.userId === 'alice')?.reconnecting).toBe(true);
  });

  it('closes a share that a repair gave up on', async () => {
    const { watchedScreenShares, openScreenShare } = await loadScreenShare();

    await openScreenShare('alice', 'me', 1);
    emitPeers(['alice']);
    emitPeers(['alice'], ['alice']);
    // Out of rebuilds: the manager drops it from the list for good.
    emitPeers([]);

    expect(get(watchedScreenShares)).toEqual([]);
    expect(mocks.stoppedViewing).toEqual(['alice']);
  });

  it('takes down a share that stops while it is still connecting', async () => {
    const { watchedScreenShares, openScreenShare } = await loadScreenShare();

    await openScreenShare('alice', 'me', 1);
    deliver('screenshare-stop', { user: 'alice', channelId: 1 });

    expect(get(watchedScreenShares)).toEqual([]);
    expect(mocks.stoppedViewing).toEqual(['alice']);
  });

  it('forgets everything when the connection drops', async () => {
    const { watchedScreenShares, openScreenShare } = await loadScreenShare();
    const { connection } = await import('./connection');

    await openScreenShare('alice', 'me', 1);
    emitPeers(['alice']);
    connection.set('disconnected');

    expect(get(watchedScreenShares)).toEqual([]);
  });
});

describe('screenShare — per-share audio', () => {
  it('falls back to the app-wide volume until a share is adjusted', async () => {
    const { shareAudio, setShareAudio } = await loadScreenShare();
    const { screenShareVolume } = await import('./settings');

    screenShareVolume.set(0.5);
    expect(get(shareAudio)('alice')).toEqual({ volume: 0.5, muted: false });

    // Muting one share must not silence the other one on screen.
    setShareAudio('alice', { volume: 0.5, muted: true });
    expect(get(shareAudio)('alice')).toEqual({ volume: 0.5, muted: true });
    expect(get(shareAudio)('bob')).toEqual({ volume: 0.5, muted: false });
  });

  it('starts from the app-wide setting again after a share is closed', async () => {
    const { openScreenShare, closeScreenShare, shareAudio, setShareAudio } =
      await loadScreenShare();

    await openScreenShare('alice', 'me', 1);
    setShareAudio('alice', { volume: 0.2, muted: true });
    closeScreenShare('alice');

    expect(get(shareAudio)('alice')).toEqual({ volume: 1, muted: false });
  });
});
