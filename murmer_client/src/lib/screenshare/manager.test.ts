import { beforeEach, describe, expect, it, vi } from 'vitest';

/**
 * The viewer identity the manager signs its offers with is invisible in the
 * UI: when it goes stale, the offer simply names the wrong channel, the
 * sharer drops it as not meant for them, and the window says "Connecting…"
 * forever. Nothing on screen points at the cause, so it is pinned here.
 */

const mocks = vi.hoisted(() => ({
  handlers: new Map<string, (msg: unknown) => void>(),
  sent: [] as Array<Record<string, unknown>>
}));

vi.mock('../stores/chat', () => ({
  chat: {
    on: (type: string, cb: (msg: unknown) => void) => mocks.handlers.set(type, cb),
    off: () => {},
    sendRaw: (frame: Record<string, unknown>) => mocks.sent.push(frame)
  }
}));

// Just enough of a peer connection for the viewer's offer path; Node has none.
class FakePeerConnection {
  connectionState = 'new';
  signalingState = 'stable';
  addTransceiver() {}
  getTransceivers() {
    return [];
  }
  async createOffer() {
    return { type: 'offer', sdp: '' };
  }
  async setLocalDescription() {}
  close() {}
}

beforeEach(() => {
  vi.stubGlobal('RTCPeerConnection', FakePeerConnection);
  mocks.handlers.clear();
  mocks.sent.length = 0;
});

async function freshManager() {
  vi.resetModules();
  const { ScreenShareManager } = await import('./manager');
  return new ScreenShareManager();
}

function offersTo(target: string) {
  return mocks.sent.filter((f) => f.type === 'screenshare-offer' && f.target === target);
}

describe('ScreenShareManager — viewer identity', () => {
  it('signs offers for the channel it is in now, after an earlier share ended', async () => {
    const manager = await freshManager();

    await manager.viewScreenShare('alice', 'me', 1);
    mocks.handlers.get('screenshare-stop')?.({ type: 'screenshare-stop', user: 'alice', channelId: 1 });

    // Moved to another voice channel and opened a share there.
    await manager.viewScreenShare('bob', 'me', 2);

    expect(offersTo('bob')).toHaveLength(1);
    expect(offersTo('bob')[0].channelId).toBe(2);
  });

  it('keeps the identity while another share is still being watched', async () => {
    const manager = await freshManager();

    await manager.viewScreenShare('alice', 'me', 1);
    await manager.viewScreenShare('bob', 'me', 1);
    manager.stopViewing('alice');

    // Bob's share is still running in channel 1; a stray channel from the
    // caller must not re-address it.
    await manager.viewScreenShare('carol', 'me', 2);
    expect(offersTo('carol')[0].channelId).toBe(1);
  });
});
