import { beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';

/**
 * The camera announcements are the only thing that says whose tile to show:
 * the video transceiver delivers a track whether or not a camera is on, so a
 * frame parsed wrongly here shows a live tile for a camera that was switched
 * off — or, on the reset path, a camera badge on a member of a *different*
 * server, since channel ids are only unique per server. None of that is
 * visible until somebody switches servers with a call still up.
 */
const mocks = vi.hoisted(() => ({
  /** Frame handlers the store registered with `chat.on`. */
  chatHandlers: new Map<string, (msg: unknown) => void>(),
  /** Frames the store sent. */
  sent: [] as Array<Record<string, unknown>>,
  /** Camera tracks handed to the voice manager, `null` for a detach. */
  cameraTracks: [] as Array<MediaStreamTrack | null>,
  /** Tracks whose `stop()` the store called. */
  stopped: [] as string[],
  /** Set to make `openCamera` reject. */
  openFails: false
}));

vi.mock('./chat', () => ({
  chat: {
    on: (type: string, cb: (msg: unknown) => void) => mocks.chatHandlers.set(type, cb),
    off: () => {},
    sendRaw: (msg: Record<string, unknown>) => mocks.sent.push(msg)
  }
}));

// The real store builds a `VoiceManager`, and with it `RTCPeerConnection`s a
// plain Node run has no implementation for.
vi.mock('./voice', () => ({
  voice: {
    setCameraTrack: (track: MediaStreamTrack | null) => mocks.cameraTracks.push(track)
  }
}));

vi.mock('./videoDevices', () => ({
  refreshVideoDevices: () => Promise.resolve()
}));

/** A capture with one video track, standing in for `getUserMedia`. */
function fakeCapture(id: string): MediaStream {
  const track = {
    id,
    kind: 'video',
    contentHint: '',
    stop: () => mocks.stopped.push(id),
    addEventListener: () => {}
  } as unknown as MediaStreamTrack;
  return {
    getTracks: () => [track],
    getVideoTracks: () => [track]
  } as unknown as MediaStream;
}

let captureCount = 0;

vi.mock('../voice/camera', async (importOriginal) => {
  const actual = await importOriginal<typeof import('../voice/camera')>();
  return {
    ...actual,
    openCamera: async () => {
      if (mocks.openFails) throw new Error('permission denied');
      return fakeCapture(`capture-${++captureCount}`);
    }
  };
});

/** Fresh module instance: the store wires itself up at import time. */
async function loadWebcam() {
  vi.resetModules();
  mocks.chatHandlers.clear();
  mocks.sent.length = 0;
  mocks.cameraTracks.length = 0;
  mocks.stopped.length = 0;
  mocks.openFails = false;
  captureCount = 0;
  const connection = await import('./connection');
  connection.connection.set('connected');
  const webcam = await import('./webcam');
  return { webcam, connection };
}

/** Deliver a server frame the way the chat store would. */
function frame(type: string, msg: Record<string, unknown>) {
  mocks.chatHandlers.get(type)?.(msg);
}

beforeEach(() => {
  localStorage.clear();
});

describe('camera announcements', () => {
  it('tracks who has a camera on, per channel', async () => {
    const { webcam } = await loadWebcam();
    frame('webcam-start', { user: 'ada', channelId: 1 });
    frame('webcam-start', { user: 'bob', channelId: 2 });
    expect(get(webcam.activeWebcams)[1]).toEqual(['ada']);
    expect(get(webcam.activeWebcams)[2]).toEqual(['bob']);

    frame('webcam-stop', { user: 'ada', channelId: 1 });
    expect(get(webcam.activeWebcams)[1]).toEqual([]);
    // Stopping in one channel must not touch the other.
    expect(get(webcam.activeWebcams)[2]).toEqual(['bob']);
  });

  it('never lists the same camera twice', async () => {
    const { webcam } = await loadWebcam();
    // The announcement and the `webcam-active` catch-up both name a camera
    // that was already on when we joined.
    frame('webcam-start', { user: 'ada', channelId: 1 });
    frame('webcam-active', { channelId: 1, users: ['ada', 'bob'] });
    expect(get(webcam.activeWebcams)[1]).toEqual(['ada', 'bob']);
  });

  it('ignores frames whose fields are the wrong shape', async () => {
    const { webcam } = await loadWebcam();
    frame('webcam-start', { user: 'ada' });
    frame('webcam-start', { user: 42, channelId: 1 });
    frame('webcam-start', { user: 'ada', channelId: '1' });
    frame('webcam-active', { channelId: 1, users: 'ada' });
    expect(get(webcam.activeWebcams)).toEqual({});

    // A list with a bogus entry keeps the good ones and drops the rest.
    frame('webcam-active', { channelId: 1, users: ['ada', 7, null] });
    expect(get(webcam.activeWebcams)[1]).toEqual(['ada']);
  });

  it('forgets every camera when the connection goes', async () => {
    const { webcam, connection } = await loadWebcam();
    frame('webcam-start', { user: 'ada', channelId: 1 });
    connection.connection.set('disconnected');
    // Channel ids are only unique per server, so a retained list would put a
    // camera badge on whoever happens to sit in channel 1 of the next one.
    expect(get(webcam.activeWebcams)).toEqual({});
  });
});

describe('the local camera', () => {
  it('announces itself, hands the track to the voice mesh, and takes both back', async () => {
    const { webcam } = await loadWebcam();
    await webcam.startCamera('ada', 3);

    expect(get(webcam.cameraOn)).toBe(true);
    expect(get(webcam.activeWebcams)[3]).toEqual(['ada']);
    expect(mocks.sent).toEqual([{ type: 'webcam-start', user: 'ada', channelId: 3 }]);
    expect(mocks.cameraTracks.at(-1)).not.toBeNull();

    await webcam.stopCamera();

    expect(get(webcam.cameraOn)).toBe(false);
    expect(get(webcam.activeWebcams)[3]).toEqual([]);
    expect(mocks.sent.at(-1)).toEqual({ type: 'webcam-stop', user: 'ada', channelId: 3 });
    // Detached from the peers *and* released, or the camera light stays on.
    expect(mocks.cameraTracks.at(-1)).toBeNull();
    expect(mocks.stopped).toEqual(['capture-1']);
  });

  it('leaves the camera off when it cannot be opened', async () => {
    const { webcam } = await loadWebcam();
    mocks.openFails = true;
    await expect(webcam.startCamera('ada', 3)).rejects.toThrow();
    // A denied permission prompt must not leave the button reading "on".
    expect(get(webcam.cameraOn)).toBe(false);
    expect(get(webcam.localCameraStream)).toBeNull();
    expect(mocks.sent).toEqual([]);
    expect(get(webcam.activeWebcams)[3]).toBeUndefined();
  });

  it('stops the capture when the server says our camera ended', async () => {
    const { webcam } = await loadWebcam();
    await webcam.startCamera('ada', 3);
    // The server ends cameras on voice-leave and on disconnect; a client that
    // ignored its own stop frame would keep encoding to nobody.
    frame('webcam-stop', { user: 'ada', channelId: 3 });
    await webcam.stopCamera();
    expect(get(webcam.cameraOn)).toBe(false);
    expect(mocks.stopped).toEqual(['capture-1']);
  });

  it('swaps the track without re-announcing when the quality changes', async () => {
    const { webcam } = await loadWebcam();
    await webcam.startCamera('ada', 3);
    const announcements = mocks.sent.length;

    webcam.cameraQuality.set('720p');
    await webcam.restartCamera();

    // A new capture replaced the old one on the same peers: `replaceTrack`,
    // no renegotiation, and nothing that says the camera went off and on.
    expect(mocks.stopped).toEqual(['capture-1']);
    expect(mocks.cameraTracks.at(-1)).not.toBeNull();
    expect(mocks.sent.length).toBe(announcements);
    expect(get(webcam.activeWebcams)[3]).toEqual(['ada']);
  });

  it('rejects a hand-edited quality in storage', async () => {
    localStorage.setItem('murmer_camera_quality', '4k');
    const { webcam } = await loadWebcam();
    expect(get(webcam.cameraQuality)).toBe('480p');
  });
});
