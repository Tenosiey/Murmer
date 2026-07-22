/**
 * Client mirror of the server permission bitmask
 * (`murmer_server/src/permissions.rs`). Keep the flag values in sync.
 *
 * Every gate here is cosmetic: it only decides whether to show or enable UI.
 * The server re-checks every action, so a tampered client gains nothing.
 */
import type { RoleDef } from '../types';

export const PERMISSIONS = {
  VIEW_CHANNELS: 1 << 0,
  SEND_MESSAGES: 1 << 1,
  MANAGE_MESSAGES: 1 << 2,
  MANAGE_CHANNELS: 1 << 3,
  MANAGE_WIKI: 1 << 4,
  MANAGE_ROLES: 1 << 5,
  MANAGE_EMOJIS: 1 << 6,
  MANAGE_SERVER: 1 << 7,
  VIEW_SERVER_INFO: 1 << 8,
  VIEW_CONNECTION_STATS: 1 << 9,
  KICK_MEMBERS: 1 << 10,
  BAN_MEMBERS: 1 << 11,
  MUTE_MEMBERS: 1 << 12,
  ADMINISTRATOR: 1 << 13
} as const;

export type PermissionKey = keyof typeof PERMISSIONS;

/** Union of every defined permission flag. */
export const ALL_PERMISSIONS = Object.values(PERMISSIONS).reduce((acc, bit) => acc | bit, 0);

/** Whether a permission mask satisfies `flag`. Administrator grants everything. */
export function hasPermission(mask: number, flag: number): boolean {
  return (mask & PERMISSIONS.ADMINISTRATOR) !== 0 || (mask & flag) === flag;
}

/**
 * The effective permission mask for a set of assigned role ids: the union of
 * the default `@everyone` role and every assigned role. Administrator expands
 * to the full mask.
 */
export function computeMask(defs: RoleDef[], assignedIds: number[]): number {
  let mask = defs.find((d) => d.isDefault)?.permissions ?? 0;
  const byId = new Map(defs.map((d) => [d.id, d]));
  for (const id of assignedIds) {
    const def = byId.get(id);
    if (def) mask |= def.permissions;
  }
  return (mask & PERMISSIONS.ADMINISTRATOR) !== 0 ? ALL_PERMISSIONS : mask;
}

/**
 * The highest hierarchy position across a user's roles (the default role's
 * position is the floor). Administrators sit above everyone
 * (`Number.POSITIVE_INFINITY`).
 */
export function computeTopPosition(defs: RoleDef[], assignedIds: number[]): number {
  const def = defs.find((d) => d.isDefault);
  let position = def?.position ?? 0;
  let admin = def ? (def.permissions & PERMISSIONS.ADMINISTRATOR) !== 0 : false;
  const byId = new Map(defs.map((d) => [d.id, d]));
  for (const id of assignedIds) {
    const role = byId.get(id);
    if (!role) continue;
    position = Math.max(position, role.position);
    if ((role.permissions & PERMISSIONS.ADMINISTRATOR) !== 0) admin = true;
  }
  return admin ? Number.POSITIVE_INFINITY : position;
}

export interface PermissionMeta {
  key: PermissionKey;
  flag: number;
  label: string;
  description: string;
}

export interface PermissionGroup {
  title: string;
  permissions: PermissionMeta[];
}

/** Grouping used by the dashboard's role editor checkboxes. */
export const PERMISSION_GROUPS: PermissionGroup[] = [
  {
    title: 'General',
    permissions: [
      {
        key: 'VIEW_CHANNELS',
        flag: PERMISSIONS.VIEW_CHANNELS,
        label: 'View channels',
        description: 'See channels and read message history.'
      },
      {
        key: 'SEND_MESSAGES',
        flag: PERMISSIONS.SEND_MESSAGES,
        label: 'Send messages',
        description: 'Post messages and add reactions.'
      }
    ]
  },
  {
    title: 'Messages',
    permissions: [
      {
        key: 'MANAGE_MESSAGES',
        flag: PERMISSIONS.MANAGE_MESSAGES,
        label: 'Manage messages',
        description: "Delete and pin other members' messages."
      }
    ]
  },
  {
    title: 'Members',
    permissions: [
      {
        key: 'KICK_MEMBERS',
        flag: PERMISSIONS.KICK_MEMBERS,
        label: 'Kick members',
        description: 'Disconnect members from the server.'
      },
      {
        key: 'BAN_MEMBERS',
        flag: PERMISSIONS.BAN_MEMBERS,
        label: 'Ban members',
        description: 'Ban and unban members.'
      },
      {
        key: 'MUTE_MEMBERS',
        flag: PERMISSIONS.MUTE_MEMBERS,
        label: 'Mute members',
        description: 'Mute and unmute members.'
      }
    ]
  },
  {
    title: 'Management',
    permissions: [
      {
        key: 'MANAGE_CHANNELS',
        flag: PERMISSIONS.MANAGE_CHANNELS,
        label: 'Manage channels',
        description: 'Create, edit, reorder and delete channels and categories.'
      },
      {
        key: 'MANAGE_WIKI',
        flag: PERMISSIONS.MANAGE_WIKI,
        label: 'Manage wiki',
        description: 'Create, edit and delete wiki pages.'
      },
      {
        key: 'MANAGE_EMOJIS',
        flag: PERMISSIONS.MANAGE_EMOJIS,
        label: 'Manage emojis',
        description: 'Add and remove custom server emojis.'
      },
      {
        key: 'MANAGE_ROLES',
        flag: PERMISSIONS.MANAGE_ROLES,
        label: 'Manage roles',
        description: 'Create, edit, delete and assign roles below your own.'
      },
      {
        key: 'MANAGE_SERVER',
        flag: PERMISSIONS.MANAGE_SERVER,
        label: 'Manage server',
        description: 'Edit server identity, stats, screen-share and other settings.'
      },
      {
        key: 'VIEW_SERVER_INFO',
        flag: PERMISSIONS.VIEW_SERVER_INFO,
        label: 'View server info',
        description: 'See server details such as the running version.'
      },
      {
        key: 'VIEW_CONNECTION_STATS',
        flag: PERMISSIONS.VIEW_CONNECTION_STATS,
        label: 'View connection stats',
        description: "View other members' connection quality."
      },
      {
        key: 'ADMINISTRATOR',
        flag: PERMISSIONS.ADMINISTRATOR,
        label: 'Administrator',
        description: 'Grants every permission and bypasses the hierarchy. Use sparingly.'
      }
    ]
  }
];
