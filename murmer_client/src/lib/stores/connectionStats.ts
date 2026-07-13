import { writable, derived, get } from 'svelte/store';
import { chat } from './chat';
import { connection } from './connection';
import { session } from './session';
import { roles } from './roles';
import { ping } from './ping';
import { voiceStats } from './voice';
import type { Message, UserConnectionStats } from '../types';

/** Roles the server answers `get-connection-stats` requests for. */
const CONNECTION_STATS_ROLES = ['owner', 'admin'];

/** How often the client reports its own stats while connected. */
const REPORT_INTERVAL_MS = 10_000;

/**
 * Privacy note: this is the complete payload that ever leaves the machine —
 * four quality numbers, nothing else (no IPs, device names or peer details).
 * The server keeps only the latest report in memory and drops it on
 * disconnect. Server admins/owners can view these numbers.
 */
function buildReport() {
  const stats = Object.values(get(voiceStats));
  const worst = (values: number[]): number | null =>
    values.length > 0 ? Math.max(...values) : null;
  const pingMs = get(ping);
  return {
    type: 'connection-stats',
    ping: pingMs > 0 ? Math.round(pingMs) : null,
    voiceRtt: worst(stats.map((s) => Math.round(s.rtt))),
    voiceJitter: worst(stats.map((s) => Math.round(s.jitter * 10) / 10)),
    voiceLoss: worst(stats.map((s) => Math.round(s.packetLoss * 10) / 10))
  };
}

/**
 * Periodically reports this client's connection stats to the server while
 * connected. Starts as soon as the module is imported (the chat header pulls
 * it in), keyed off the shared connection state.
 */
function startReporting() {
  let interval: number | null = null;

  connection.subscribe((state) => {
    if (state === 'connected') {
      if (interval !== null) return;
      interval = window.setInterval(() => {
        if (get(session).user) {
          chat.sendRaw(buildReport());
        }
      }, REPORT_INTERVAL_MS);
    } else if (interval !== null) {
      clearInterval(interval);
      interval = null;
    }
  });
}

startReporting();

/**
 * Whether the current user's role allows viewing everyone's stats. The server
 * enforces the actual check; this only controls what the UI offers.
 */
export const canViewAllConnectionStats = derived([session, roles], ([$session, $roles]) => {
  const user = $session.user;
  if (!user) return false;
  const role = $roles[user]?.role?.toLowerCase();
  return !!role && CONNECTION_STATS_ROLES.includes(role);
});

function createAllConnectionStatsStore() {
  const { subscribe, set } = writable<Record<string, UserConnectionStats>>({});

  function sanitizeNumber(raw: unknown): number | null {
    return typeof raw === 'number' && Number.isFinite(raw) && raw >= 0 ? raw : null;
  }

  chat.on('connection-stats-list', (msg: Message) => {
    const raw = (msg as any).stats;
    if (!raw || typeof raw !== 'object') return;
    const parsed: Record<string, UserConnectionStats> = {};
    for (const [user, entry] of Object.entries(raw as Record<string, any>)) {
      if (!entry || typeof entry !== 'object') continue;
      parsed[user] = {
        ping: sanitizeNumber(entry.ping),
        voiceRtt: sanitizeNumber(entry.voiceRtt),
        voiceJitter: sanitizeNumber(entry.voiceJitter),
        voiceLoss: sanitizeNumber(entry.voiceLoss),
        ageSeconds: sanitizeNumber(entry.ageSeconds) ?? 0
      };
    }
    set(parsed);
  });

  connection.subscribe((state) => {
    if (state !== 'connected') set({});
  });

  /** Ask the server for everyone's stats (answered only for Owner/Admin). */
  function request() {
    chat.sendRaw({ type: 'get-connection-stats' });
  }

  return { subscribe, request };
}

/** Everyone's self-reported stats, populated on request for Owner/Admin. */
export const allConnectionStats = createAllConnectionStatsStore();
