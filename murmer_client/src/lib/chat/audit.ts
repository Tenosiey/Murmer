/**
 * Client mirror of the audit log's action names
 * (`murmer_server/src/db/audit.rs`), plus the label each one is shown under.
 *
 * The map is cosmetic: an action the client has no label for is rendered as
 * its own wire name rather than dropped, so an older client meeting a newer
 * server shows a plainer row instead of a shorter list. Losing a row would be
 * the worse failure — an audit log that quietly omits entries is worse than
 * no audit log at all.
 *
 * `murmer_client/test/server-mirror.test.ts` parses the Rust source and fails
 * when an action gains no label here.
 */

/** Wire names, in the order they are grouped in the dashboard's filter. */
export const AUDIT_ACTIONS = {
  kick: 'Kicked a member',
  ban: 'Banned a member',
  unban: 'Lifted a ban',
  mute: 'Muted a member',
  unmute: 'Lifted a mute',
  'role-create': 'Created a role',
  'role-update': 'Edited a role',
  'role-delete': 'Deleted a role',
  'role-reorder': 'Reordered the roles',
  'user-roles': "Changed a member's roles",
  'admin-role-grant': 'Granted a role via /role',
  'override-set': 'Set a channel permission',
  'override-remove': 'Removed a channel permission',
  'purge-messages': 'Purged all messages',
  'server-reset': 'Reset the server'
} as const;

export type AuditAction = keyof typeof AUDIT_ACTIONS;

/**
 * Actor recorded for a change made through the `/role` HTTP endpoint. It is
 * not an account name — the parentheses are what make it unforgeable — so the
 * dashboard must not run it through the nickname lookup.
 */
export const AUDIT_ACTOR_ADMIN_TOKEN = '(admin token)';

/** The label for an action, falling back to the raw wire name. */
export function auditActionLabel(action: string): string {
  return AUDIT_ACTIONS[action as AuditAction] ?? action;
}

/**
 * The actions whose `target` is an account name. Everything else targets a
 * role, a channel, or nothing at all.
 *
 * This exists because `$displayNames` is a *member* lookup: running a role
 * named "Helper" through it would show the nickname of a member who happens
 * to be called that. Names are deliberately not unique across those spaces,
 * so the action is the only thing that says which one a target belongs to.
 * An action this does not know is treated as not naming a member — showing
 * the stored string verbatim is always truthful.
 */
const MEMBER_TARGET_ACTIONS = new Set<string>([
  'kick',
  'ban',
  'unban',
  'mute',
  'unmute',
  'user-roles',
  'admin-role-grant'
]);

/** Whether this action's `target` should be read as an account name. */
export function auditTargetIsMember(action: string): boolean {
  return MEMBER_TARGET_ACTIONS.has(action);
}
