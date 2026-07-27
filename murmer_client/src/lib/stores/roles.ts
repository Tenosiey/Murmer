import { writable, derived } from 'svelte/store';
import { chat } from './chat';
import { roleDefinitions } from './roleDefinitions';
import type { Message, RoleDef, RoleInfo } from '../types';

/**
 * Role ids assigned to each user, keyed by username. Populated from the
 * `user-roles` frame the server sends per user on connect and on any change.
 * The default `@everyone` role is implicit and never appears here.
 */
function createUserRoleIdsStore() {
  const { subscribe, update, set } = writable<Record<string, number[]>>({});

  chat.on('user-roles', (msg: Message) => {
    const user = msg.user;
    const raw = (msg as any).roleIds;
    if (typeof user !== 'string' || !Array.isArray(raw)) return;
    const ids = raw.filter((id): id is number => typeof id === 'number');
    update((r) => ({ ...r, [user]: ids }));
  });

  return { subscribe, reset: () => set({}) };
}

export const userRoleIds = createUserRoleIdsStore();

/**
 * The role a user is displayed as: their highest-position assigned role.
 *
 * The icon is picked separately, from the highest role that actually carries
 * one — a member whose top role has no icon still shows the badge of a lower
 * role that does, instead of silently losing it.
 */
function displayRole(defs: RoleDef[], ids: number[]): RoleInfo | undefined {
  const byId = new Map(defs.map((d) => [d.id, d]));
  let best: RoleDef | undefined;
  let bestIcon: RoleDef | undefined;
  for (const id of ids) {
    const def = byId.get(id);
    if (!def || def.isDefault) continue;
    if (!best || def.position > best.position) best = def;
    if (def.icon && (!bestIcon || def.position > bestIcon.position)) bestIcon = def;
  }
  if (!best) return undefined;
  return {
    role: best.name,
    color: best.color,
    icon: bestIcon?.icon,
    iconRole: bestIcon?.name
  };
}

/**
 * Per-user display role (username → highest-position role name/color), derived
 * from the role definitions and each user's assignments. Kept in the same shape
 * the badge/label components consumed before custom roles existed.
 */
export const roles = derived(
  [roleDefinitions, userRoleIds],
  ([defs, assignments]) => {
    const map: Record<string, RoleInfo> = {};
    for (const [user, ids] of Object.entries(assignments)) {
      const info = displayRole(defs, ids);
      if (info) map[user] = info;
    }
    return map;
  }
);
