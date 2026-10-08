/**
 * Client mirror of the server permission bitmask
 * (`murmer_server/src/permissions.rs`). Keep the flag values in sync.
 *
 * Every gate here is cosmetic: it only decides whether to show or enable UI.
 * The server re-checks every action, so a tampered client gains nothing.
 */
import type { RoleDef } from '../types';
import { t } from '../i18n';

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
  ADMINISTRATOR: 1 << 13,
  USE_SOUNDBOARD: 1 << 14,
  MANAGE_SOUNDS: 1 << 15,
  MANAGE_NICKNAMES: 1 << 16,
  VIEW_AUDIT_LOG: 1 << 17,
  CREATE_INVITES: 1 << 18,
  MENTION_GROUPS: 1 << 19,
  MOVE_MEMBERS: 1 << 20
} as const;

export type PermissionKey = keyof typeof PERMISSIONS;

/** Union of every defined permission flag. */
export const ALL_PERMISSIONS = Object.values(PERMISSIONS).reduce((acc, bit) => acc | bit, 0);

/**
 * Permissions a per-channel override may grant or deny: "see" and "write/talk".
 * Mirrors the server's `CHANNEL_OVERRIDABLE`.
 */
export const CHANNEL_OVERRIDABLE = PERMISSIONS.VIEW_CHANNELS | PERMISSIONS.SEND_MESSAGES;

/** A per-channel override toggle state for one permission flag. */
export type OverrideState = 'inherit' | 'allow' | 'deny';

/** Resolve the tri-state of `flag` from an allow/deny mask pair. */
export function overrideState(allow: number, deny: number, flag: number): OverrideState {
  if ((allow & flag) === flag) return 'allow';
  if ((deny & flag) === flag) return 'deny';
  return 'inherit';
}

/** Apply a tri-state choice for `flag` onto an allow/deny mask pair. */
export function applyOverrideState(
  allow: number,
  deny: number,
  flag: number,
  state: OverrideState
): { allow: number; deny: number } {
  let nextAllow = allow & ~flag;
  let nextDeny = deny & ~flag;
  if (state === 'allow') nextAllow |= flag;
  else if (state === 'deny') nextDeny |= flag;
  return { allow: nextAllow, deny: nextDeny };
}

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
    title: t('permissionGroup.general'),
    permissions: [
      {
        key: 'VIEW_CHANNELS',
        flag: PERMISSIONS.VIEW_CHANNELS,
        label: t('permission.viewChannels'),
        description: t('permission.viewChannels.description')
      },
      {
        key: 'SEND_MESSAGES',
        flag: PERMISSIONS.SEND_MESSAGES,
        label: t('permission.sendMessages'),
        description: t('permission.sendMessages.description')
      },
      {
        key: 'USE_SOUNDBOARD',
        flag: PERMISSIONS.USE_SOUNDBOARD,
        label: t('permission.useSoundboard'),
        description: t('permission.useSoundboard.description')
      }
    ]
  },
  {
    title: t('permissionGroup.messages'),
    permissions: [
      {
        key: 'MANAGE_MESSAGES',
        flag: PERMISSIONS.MANAGE_MESSAGES,
        label: t('permission.manageMessages'),
        description: t('permission.manageMessages.description')
      },
      {
        key: 'MENTION_GROUPS',
        flag: PERMISSIONS.MENTION_GROUPS,
        label: t('permission.mentionGroups'),
        description: t('permission.mentionGroups.description')
      }
    ]
  },
  {
    title: t('permissionGroup.members'),
    permissions: [
      {
        key: 'KICK_MEMBERS',
        flag: PERMISSIONS.KICK_MEMBERS,
        label: t('permission.kickMembers'),
        description: t('permission.kickMembers.description')
      },
      {
        key: 'BAN_MEMBERS',
        flag: PERMISSIONS.BAN_MEMBERS,
        label: t('permission.banMembers'),
        description: t('permission.banMembers.description')
      },
      {
        key: 'MUTE_MEMBERS',
        flag: PERMISSIONS.MUTE_MEMBERS,
        label: t('permission.muteMembers'),
        description: t('permission.muteMembers.description')
      },
      {
        key: 'MANAGE_NICKNAMES',
        flag: PERMISSIONS.MANAGE_NICKNAMES,
        label: t('permission.manageNicknames'),
        description: t('permission.manageNicknames.description')
      },
      {
        key: 'MOVE_MEMBERS',
        flag: PERMISSIONS.MOVE_MEMBERS,
        label: t('permission.moveMembers'),
        description: t('permission.moveMembers.description')
      },
      {
        key: 'CREATE_INVITES',
        flag: PERMISSIONS.CREATE_INVITES,
        label: t('permission.createInvites'),
        description: t('permission.createInvites.description')
      }
    ]
  },
  {
    title: t('permissionGroup.management'),
    permissions: [
      {
        key: 'MANAGE_CHANNELS',
        flag: PERMISSIONS.MANAGE_CHANNELS,
        label: t('permission.manageChannels'),
        description: t('permission.manageChannels.description')
      },
      {
        key: 'MANAGE_WIKI',
        flag: PERMISSIONS.MANAGE_WIKI,
        label: t('permission.manageWiki'),
        description: t('permission.manageWiki.description')
      },
      {
        key: 'MANAGE_EMOJIS',
        flag: PERMISSIONS.MANAGE_EMOJIS,
        label: t('permission.manageEmojis'),
        description: t('permission.manageEmojis.description')
      },
      {
        key: 'MANAGE_SOUNDS',
        flag: PERMISSIONS.MANAGE_SOUNDS,
        label: t('permission.manageSounds'),
        description: t('permission.manageSounds.description')
      },
      {
        key: 'MANAGE_ROLES',
        flag: PERMISSIONS.MANAGE_ROLES,
        label: t('permission.manageRoles'),
        description: t('permission.manageRoles.description')
      },
      {
        key: 'MANAGE_SERVER',
        flag: PERMISSIONS.MANAGE_SERVER,
        label: t('permission.manageServer'),
        description: t('permission.manageServer.description')
      },
      {
        key: 'VIEW_SERVER_INFO',
        flag: PERMISSIONS.VIEW_SERVER_INFO,
        label: t('permission.viewServerInfo'),
        description: t('permission.viewServerInfo.description')
      },
      {
        key: 'VIEW_CONNECTION_STATS',
        flag: PERMISSIONS.VIEW_CONNECTION_STATS,
        label: t('permission.viewConnectionStats'),
        description: t('permission.viewConnectionStats.description')
      },
      {
        key: 'VIEW_AUDIT_LOG',
        flag: PERMISSIONS.VIEW_AUDIT_LOG,
        label: t('permission.viewAuditLog'),
        description: t('permission.viewAuditLog.description')
      },
      {
        key: 'ADMINISTRATOR',
        flag: PERMISSIONS.ADMINISTRATOR,
        label: t('permission.administrator'),
        description: t('permission.administrator.description')
      }
    ]
  }
];
