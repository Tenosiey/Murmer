import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message, RoleInfo } from '../types';

const HEX_COLOR_RE = /^#[0-9a-fA-F]{3,8}$/;

function sanitizeColor(raw: unknown): string | undefined {
  if (typeof raw === 'string' && HEX_COLOR_RE.test(raw)) return raw;
  return undefined;
}

function createRoleStore() {
  const { subscribe, update, set } = writable<Record<string, RoleInfo>>({});
  chat.on('role-update', (msg: Message) => {
    const user = msg.user;
    const role = (msg as any).role as string | undefined;
    const color = sanitizeColor((msg as any).color);
    if (typeof user === 'string' && typeof role === 'string') {
      update(r => ({ ...r, [user]: { role, color } }));
    }
  });
  chat.on('role-remove', (msg: Message) => {
    const user = msg.user;
    if (typeof user === 'string') {
      update(r => {
        const copy = { ...r };
        delete copy[user];
        return copy;
      });
    }
  });
  return { subscribe, set };
}

export const roles = createRoleStore();
