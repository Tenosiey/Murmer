import { writable } from 'svelte/store';
import { chat } from './chat';
import type { Message, RoleDef } from '../types';

const HEX_COLOR_RE = /^#[0-9a-fA-F]{3,8}$/;

function sanitizeColor(raw: unknown): string | undefined {
  if (typeof raw === 'string' && HEX_COLOR_RE.test(raw)) return raw;
  return undefined;
}

function toRoleDef(raw: unknown): RoleDef | null {
  if (!raw || typeof raw !== 'object') return null;
  const r = raw as Record<string, unknown>;
  if (typeof r.id !== 'number' || typeof r.name !== 'string') return null;
  return {
    id: r.id,
    name: r.name,
    color: sanitizeColor(r.color),
    permissions: typeof r.permissions === 'number' ? r.permissions : 0,
    position: typeof r.position === 'number' ? r.position : 0,
    isDefault: r.isDefault === true,
    isOwner: r.isOwner === true
  };
}

/**
 * All server role definitions, sorted by ascending position (lowest power
 * first). Populated from the `role-definitions` frame the server sends on
 * connect and whenever a role changes.
 */
function createRoleDefinitionsStore() {
  const { subscribe, set } = writable<RoleDef[]>([]);

  chat.on('role-definitions', (msg: Message) => {
    const list = (msg as any).roles;
    if (!Array.isArray(list)) return;
    const defs = list.map(toRoleDef).filter((d): d is RoleDef => d !== null);
    defs.sort((a, b) => a.position - b.position || a.id - b.id);
    set(defs);
  });

  return { subscribe, reset: () => set([]) };
}

export const roleDefinitions = createRoleDefinitionsStore();
