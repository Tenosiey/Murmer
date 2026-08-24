import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';

/**
 * Stand-in for the chat store, as in the other store tests: records outgoing
 * frames and lets a test push incoming ones into the handlers the store
 * registers at import time.
 */
const bus = vi.hoisted(() => {
  const sent: Record<string, any>[] = [];
  const handlers = new Map<string, ((msg: any) => void)[]>();
  return {
    sent,
    handlers,
    reset() {
      sent.length = 0;
      handlers.clear();
    },
    emit(type: string, payload: Record<string, unknown>) {
      for (const handler of handlers.get(type) ?? []) handler({ type, ...payload });
    }
  };
});

vi.mock('./chat', () => ({
  chat: {
    sendRaw(data: Record<string, any>) {
      bus.sent.push(data);
    },
    on(type: string, callback: (msg: any) => void) {
      const list = bus.handlers.get(type) ?? [];
      list.push(callback);
      bus.handlers.set(type, list);
    },
    off() {}
  }
}));

/** The store wires up its handler on import, so each test gets a fresh one. */
async function load() {
  bus.reset();
  vi.resetModules();
  const [metrics, { connection }] = await Promise.all([
    import('./serverMetrics'),
    import('./connection')
  ]);
  connection.set('connected');
  return { ...metrics, connection };
}

/** A full frame, so a test only has to name the fields it cares about. */
function frame(overrides: Record<string, unknown> = {}) {
  return {
    uptimeSeconds: 100,
    connections: 3,
    peakConnections: 9,
    frames: 1000,
    dbCalls: 500,
    dbTotalMs: 250,
    dbMaxMs: 42,
    rejectedMessages: 0,
    rejectedAuth: 0,
    rejectedUploads: 0,
    rejectedReplays: 0,
    ...overrides
  };
}

beforeEach(() => {
  bus.reset();
  vi.useFakeTimers();
  vi.setSystemTime(0);
});

afterEach(() => {
  vi.useRealTimers();
});

describe('serverMetrics — turning cumulative counters into rates', () => {
  it('reports averages since startup until it has a second sample', async () => {
    const { serverMetrics } = await load();

    expect(get(serverMetrics)).toBeNull();
    serverMetrics.refresh();
    expect(bus.sent).toEqual([{ type: 'get-server-metrics' }]);

    bus.emit('server-metrics', frame());

    const view = get(serverMetrics)!;
    // 1000 frames over 100 seconds of uptime, 250 ms across 500 calls.
    expect(view.framesPerSecond).toBeCloseTo(10);
    expect(view.dbAverageMs).toBeCloseTo(0.5);
  });

  it('measures the rate across the interval it actually watched', async () => {
    const { serverMetrics } = await load();

    bus.emit('server-metrics', frame());
    vi.setSystemTime(4_000);
    bus.emit(
      'server-metrics',
      frame({ uptimeSeconds: 104, frames: 1120, dbCalls: 510, dbTotalMs: 270 })
    );

    const view = get(serverMetrics)!;
    // 120 frames in the four seconds between the two samples, not 1120 over
    // the server's whole life; 20 ms across the 10 calls made in between.
    expect(view.framesPerSecond).toBeCloseTo(30);
    expect(view.dbAverageMs).toBeCloseTo(2);
  });

  it('reports no latency at all rather than zero when nothing queried', async () => {
    const { serverMetrics } = await load();

    bus.emit('server-metrics', frame());
    vi.setSystemTime(4_000);
    bus.emit('server-metrics', frame({ uptimeSeconds: 104, frames: 1120 }));

    // No calls in the interval. A mean of zero would read as an infinitely
    // fast database; what actually happened is that nothing asked it.
    expect(get(serverMetrics)!.dbAverageMs).toBeNull();
  });

  it('does not report a negative rate when the server restarted in between', async () => {
    const { serverMetrics } = await load();

    bus.emit('server-metrics', frame());
    vi.setSystemTime(4_000);
    // Counters back near zero and a lower uptime: this is a new process, so
    // differencing against the old sample would report a huge negative rate.
    bus.emit('server-metrics', frame({ uptimeSeconds: 2, frames: 40, dbCalls: 8, dbTotalMs: 4 }));

    const view = get(serverMetrics)!;
    expect(view.framesPerSecond).toBeCloseTo(20);
    expect(view.dbAverageMs).toBeCloseTo(0.5);
  });

  it('drops garbage fields rather than rendering them', async () => {
    const { serverMetrics } = await load();

    bus.emit('server-metrics', frame({ connections: 'lots', dbMaxMs: -1, frames: null }));

    const view = get(serverMetrics)!;
    expect(view.connections).toBe(0);
    expect(view.dbMaxMs).toBe(0);
    expect(view.frames).toBe(0);
  });

  it('forgets the counters and the baseline when the connection drops', async () => {
    const { serverMetrics, connection } = await load();

    bus.emit('server-metrics', frame());
    connection.set('idle');
    expect(get(serverMetrics)).toBeNull();

    // The next server's counters are its own: the first sample after
    // reconnecting must be an average since *its* startup, not a difference
    // against numbers from a different process.
    connection.set('connected');
    vi.setSystemTime(9_000);
    bus.emit('server-metrics', frame({ uptimeSeconds: 20, frames: 200 }));
    expect(get(serverMetrics)!.framesPerSecond).toBeCloseTo(10);
  });
});

describe('serverMetrics — the units an operator reads', () => {
  it('renders uptime as the coarsest two units that fit', async () => {
    const { formatUptime } = await load();

    expect(formatUptime(45)).toBe('45s');
    expect(formatUptime(90)).toBe('1m');
    expect(formatUptime(3 * 3600 + 25 * 60)).toBe('3h 25m');
    expect(formatUptime(50 * 3600)).toBe('2d 2h');
  });

  it('keeps sub-millisecond latency visible instead of rounding it to zero', async () => {
    const { formatMs } = await load();

    expect(formatMs(0.42)).toBe('0.42 ms');
    expect(formatMs(3.14)).toBe('3.1 ms');
    expect(formatMs(142.6)).toBe('143 ms');
  });
});
