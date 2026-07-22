import { derived } from 'svelte/store';
import { roleDefinitions } from './roleDefinitions';
import { userRoleIds } from './roles';
import { session } from './session';
import { computeMask, computeTopPosition, hasPermission } from '../chat/permissions';

/** The current user's effective permission bitmask. */
export const myPermissions = derived(
  [roleDefinitions, userRoleIds, session],
  ([defs, assignments, sess]) => {
    const ids = sess.user ? (assignments[sess.user] ?? []) : [];
    return computeMask(defs, ids);
  }
);

/** The current user's hierarchy position (Infinity for administrators). */
export const myTopPosition = derived(
  [roleDefinitions, userRoleIds, session],
  ([defs, assignments, sess]) => {
    const ids = sess.user ? (assignments[sess.user] ?? []) : [];
    return computeTopPosition(defs, ids);
  }
);

/**
 * Reactive permission predicate for the current user: `$can(flag)` is true when
 * the user's roles grant `flag`. Purely cosmetic — the server re-checks.
 */
export const can = derived(myPermissions, (mask) => (flag: number) => hasPermission(mask, flag));
