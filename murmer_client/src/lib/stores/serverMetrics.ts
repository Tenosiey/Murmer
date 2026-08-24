/**
 * The server's live operator counters, shown in the Server Dashboard's Health
 * tab: connections, frames taken in, database latency and how many requests
 * the rate limits turned away.
 *
 * Like the ban list and the storage report this is a cached *answer*, not a
 * broadcast — only a viewer holding `MANAGE_SERVER` gets one, so `null` means
 * "not answered yet", never "zero".
 *
 * The server sends cumulative counters plus its uptime, because a rate needs
 * a window and it has no business keeping one per viewer. The rates below are
 * therefore derived here, from the difference between the last two samples,
 * which makes them the rate over exactly the interval this dashboard was
 * watching. The interval is measured on this clock rather than from the
 * server's uptime: uptime arrives in whole seconds, and quantising a three
 * second poll to 2, 3 or 4 would make an idle server's frame rate jump by a
 * third between refreshes.
 */
import { writable } from 'svelte/store';
import { chat } from './chat';
import { connection } from './connection';
import type { Message } from '../types';

/** How often the Health tab re-asks while it is open. */
export const METRICS_POLL_INTERVAL_MS = 3000;

/** The counters exactly as the server reports them: cumulative since start. */
export interface ServerMetrics {
  uptimeSeconds: number;
  /** Connections open right now. */
  connections: number;
  /** The most that were open at once since the server started. */
  peakConnections: number;
  /** Frames accepted from clients, valid JSON or not. */
  frames: number;
  dbCalls: number;
  /** Time spent in database calls, the wait for the connection thread included. */
  dbTotalMs: number;
  /** The slowest single call since the server started. */
  dbMaxMs: number;
  rejectedMessages: number;
  rejectedAuth: number;
  rejectedUploads: number;
  rejectedReplays: number;
}

/** The counters plus the rates derived from the previous sample. */
export interface ServerMetricsView extends ServerMetrics {
  /** Frames per second over the last polling interval. */
  framesPerSecond: number;
  /**
   * Mean database call latency over that same interval, in milliseconds, or
   * `null` when no call was made in it. An idle server must not be rendered
   * as an infinitely fast one.
   */
  dbAverageMs: number | null;
}

function count(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? Math.round(value) : 0;
}

function millis(value: unknown): number {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : 0;
}

function parse(payload: Record<string, unknown>): ServerMetrics {
  return {
    uptimeSeconds: count(payload.uptimeSeconds),
    connections: count(payload.connections),
    peakConnections: count(payload.peakConnections),
    frames: count(payload.frames),
    dbCalls: count(payload.dbCalls),
    dbTotalMs: millis(payload.dbTotalMs),
    dbMaxMs: millis(payload.dbMaxMs),
    rejectedMessages: count(payload.rejectedMessages),
    rejectedAuth: count(payload.rejectedAuth),
    rejectedUploads: count(payload.rejectedUploads),
    rejectedReplays: count(payload.rejectedReplays)
  };
}

/** A previous sample and when this client received it. */
interface Sample {
  metrics: ServerMetrics;
  at: number;
}

function derive(now: ServerMetrics, at: number, previous: Sample | null): ServerMetricsView {
  // A server that restarted between two polls reports a lower uptime and
  // counters that started again from zero. Differencing across that would
  // report a wildly negative rate, so the older sample is discarded and this
  // one is treated as the first.
  const usable = previous && now.uptimeSeconds >= previous.metrics.uptimeSeconds ? previous : null;
  const seconds = usable ? (at - usable.at) / 1000 : now.uptimeSeconds;
  const frames = usable ? now.frames - usable.metrics.frames : now.frames;
  const calls = usable ? now.dbCalls - usable.metrics.dbCalls : now.dbCalls;
  const dbMs = usable ? now.dbTotalMs - usable.metrics.dbTotalMs : now.dbTotalMs;
  return {
    ...now,
    framesPerSecond: seconds > 0 ? frames / seconds : 0,
    dbAverageMs: calls > 0 ? dbMs / calls : null
  };
}

function createServerMetricsStore() {
  /** null until the server has answered a request on this connection. */
  const { subscribe, set } = writable<ServerMetricsView | null>(null);
  let previous: Sample | null = null;

  chat.on('server-metrics', (msg: Message) => {
    const metrics = parse(msg as unknown as Record<string, unknown>);
    const at = Date.now();
    set(derive(metrics, at, previous));
    previous = { metrics, at };
  });

  connection.subscribe((state) => {
    if (state !== 'connected') {
      // The next server's counters have nothing to do with this one's, so the
      // baseline goes with the connection that produced it.
      previous = null;
      set(null);
    }
  });

  /** Ask the server for the current counters (`MANAGE_SERVER` only). */
  function refresh(): void {
    chat.sendRaw({ type: 'get-server-metrics' });
  }

  return { subscribe, refresh };
}

export const serverMetrics = createServerMetricsStore();

/** Render a duration in seconds as the coarsest two units that fit. */
export function formatUptime(seconds: number): string {
  if (seconds < 60) return `${Math.floor(seconds)}s`;
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}m`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ${minutes % 60}m`;
  return `${Math.floor(hours / 24)}d ${hours % 24}h`;
}

/**
 * Render a latency in milliseconds. Sub-millisecond calls are the normal case
 * for a local SQLite file, so they keep two decimals rather than rounding to
 * a flat "0 ms" that hides the moment they stop being sub-millisecond.
 */
export function formatMs(ms: number): string {
  if (ms >= 100) return `${Math.round(ms)} ms`;
  if (ms >= 1) return `${ms.toFixed(1)} ms`;
  return `${ms.toFixed(2)} ms`;
}
