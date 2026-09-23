import { afterEach, describe, expect, it, vi } from 'vitest';
import { WebSocketManager } from './websocket-manager';

/**
 * Several stores listen to the same frame types, and so do the call
 * managers. Both failure modes here are silent: a handler that throws, or a
 * teardown that removes too much, just leaves some other part of the UI
 * never hearing about a frame again.
 */
class FakeSocket {
  static OPEN = 1;
  static last: FakeSocket;
  readyState = FakeSocket.OPEN;
  private listeners = new Map<string, ((ev: unknown) => void)[]>();
  constructor() {
    FakeSocket.last = this;
  }
  addEventListener(type: string, cb: (ev: unknown) => void) {
    this.listeners.set(type, [...(this.listeners.get(type) ?? []), cb]);
  }
  receive(frame: object) {
    for (const cb of this.listeners.get('message') ?? []) cb({ data: JSON.stringify(frame) });
  }
  close() {}
  send() {}
}

afterEach(() => vi.unstubAllGlobals());

function connected() {
  vi.stubGlobal('WebSocket', FakeSocket);
  const manager = new WebSocketManager();
  manager.connect('ws://test/ws', () => {});
  return manager;
}

describe('WebSocketManager handlers', () => {
  it('still runs later handlers when an earlier one throws', () => {
    const manager = connected();
    const seen: string[] = [];
    vi.spyOn(console, 'error').mockImplementation(() => {});
    manager.on('ping', () => {
      throw new Error('broken store');
    });
    manager.on('ping', (msg) => seen.push(msg.type));

    FakeSocket.last.receive({ type: 'ping' });

    expect(seen).toEqual(['ping']);
  });

  it('removes only the handler it is given', () => {
    const manager = connected();
    const seen: string[] = [];
    const callManager = () => seen.push('call');
    manager.on('screenshare-stop', () => seen.push('store'));
    manager.on('screenshare-stop', callManager);

    manager.off('screenshare-stop', callManager);
    FakeSocket.last.receive({ type: 'screenshare-stop' });

    expect(seen).toEqual(['store']);
  });
});
