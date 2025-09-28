import { get, writable } from 'svelte/store';
import { chat } from './chat';
import { session } from './session';
import type { Message, UserStatus } from '../types';

export const USER_STATUS_VALUES = ['online', 'away', 'busy', 'offline'] as const;

const USER_STATUS_SET = new Set(USER_STATUS_VALUES);

export const STATUS_LABELS: Record<UserStatus, string> = {
  online: 'Online',
  away: 'Away',
  busy: 'Busy',
  offline: 'Offline'
};

export const STATUS_EMOJIS: Record<UserStatus, string> = {
  online: '🟢',
  away: '🌙',
  busy: '⛔',
  offline: '⚫'
};

function normalizeStatus(value: unknown): UserStatus | null {
  if (typeof value !== 'string') return null;
  const lowered = value.toLowerCase() as UserStatus;
  return USER_STATUS_SET.has(lowered) ? lowered : null;
}

function createStatusStore() {
  const { subscribe, set, update } = writable<Record<string, UserStatus>>({});

  chat.on('status-snapshot', (msg: Message) => {
    const raw = (msg as any).statuses;
    if (!raw || typeof raw !== 'object') return;
    const entries = raw as Record<string, unknown>;
    const normalized: Record<string, UserStatus> = {};
    for (const [user, value] of Object.entries(entries)) {
      if (typeof user !== 'string') continue;
      const status = normalizeStatus(value);
      if (status) {
        normalized[user] = status;
      }
    }
    set(normalized);
  });

  chat.on('status-update', (msg: Message) => {
    const user = typeof msg.user === 'string' ? msg.user : null;
    if (!user) return;
    const status = normalizeStatus((msg as any).status);
    if (!status) return;
    update((map) => ({
      ...map,
      [user]: status
    }));
  });

  return {
    subscribe,
    setSelf(status: UserStatus) {
      if (!USER_STATUS_SET.has(status)) return;
      const user = get(session).user;
      if (!user) return;
      chat.sendRaw({ type: 'status-update', status });
      update((map) => ({
        ...map,
        [user]: status
      }));
    }
  };
}

export const statuses = createStatusStore();
