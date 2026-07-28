import { describe, expect, it } from 'vitest';
import type { RoleDef } from '../types';
import {
  ALL_PERMISSIONS,
  CHANNEL_OVERRIDABLE,
  PERMISSIONS,
  PERMISSION_GROUPS,
  applyOverrideState,
  computeMask,
  computeTopPosition,
  hasPermission,
  overrideState,
  type OverrideState
} from './permissions';

function role(partial: Partial<RoleDef> & { id: number }): RoleDef {
  return {
    name: `role-${partial.id}`,
    permissions: 0,
    position: 0,
    isDefault: false,
    isOwner: false,
    ...partial
  };
}

const everyone = role({
  id: 1,
  name: '@everyone',
  isDefault: true,
  permissions: PERMISSIONS.VIEW_CHANNELS | PERMISSIONS.SEND_MESSAGES | PERMISSIONS.USE_SOUNDBOARD,
  position: 0
});
const moderator = role({
  id: 2,
  permissions: PERMISSIONS.KICK_MEMBERS | PERMISSIONS.MANAGE_MESSAGES,
  position: 5
});
const emojiEditor = role({ id: 3, permissions: PERMISSIONS.MANAGE_EMOJIS, position: 2 });
const owner = role({
  id: 4,
  permissions: PERMISSIONS.ADMINISTRATOR,
  position: 10,
  isOwner: true
});

describe('hasPermission', () => {
  it('checks the flag against the mask', () => {
    const mask = PERMISSIONS.VIEW_CHANNELS | PERMISSIONS.KICK_MEMBERS;
    expect(hasPermission(mask, PERMISSIONS.VIEW_CHANNELS)).toBe(true);
    expect(hasPermission(mask, PERMISSIONS.KICK_MEMBERS)).toBe(true);
    expect(hasPermission(mask, PERMISSIONS.BAN_MEMBERS)).toBe(false);
    expect(hasPermission(0, PERMISSIONS.VIEW_CHANNELS)).toBe(false);
  });

  it('requires every bit of a multi-flag check', () => {
    expect(hasPermission(PERMISSIONS.VIEW_CHANNELS, CHANNEL_OVERRIDABLE)).toBe(false);
    expect(
      hasPermission(PERMISSIONS.VIEW_CHANNELS | PERMISSIONS.SEND_MESSAGES, CHANNEL_OVERRIDABLE)
    ).toBe(true);
  });

  it('lets Administrator satisfy anything', () => {
    for (const flag of Object.values(PERMISSIONS)) {
      expect(hasPermission(PERMISSIONS.ADMINISTRATOR, flag)).toBe(true);
    }
    expect(hasPermission(PERMISSIONS.ADMINISTRATOR, ALL_PERMISSIONS)).toBe(true);
  });
});

describe('computeMask', () => {
  const defs = [everyone, moderator, emojiEditor, owner];

  it('starts from the @everyone baseline', () => {
    expect(computeMask(defs, [])).toBe(everyone.permissions);
  });

  it('unions every assigned role onto the baseline', () => {
    expect(computeMask(defs, [moderator.id])).toBe(everyone.permissions | moderator.permissions);
    expect(computeMask(defs, [moderator.id, emojiEditor.id])).toBe(
      everyone.permissions | moderator.permissions | emojiEditor.permissions
    );
  });

  it('ignores role ids that no longer exist', () => {
    expect(computeMask(defs, [999])).toBe(everyone.permissions);
    expect(computeMask(defs, [moderator.id, 999])).toBe(
      everyone.permissions | moderator.permissions
    );
  });

  it('expands Administrator to the full mask', () => {
    expect(computeMask(defs, [owner.id])).toBe(ALL_PERMISSIONS);
    // Even when it arrives alongside a narrow role.
    expect(computeMask(defs, [emojiEditor.id, owner.id])).toBe(ALL_PERMISSIONS);
  });

  it('expands Administrator granted through @everyone too', () => {
    const adminBaseline = [role({ ...everyone, permissions: PERMISSIONS.ADMINISTRATOR })];
    expect(computeMask(adminBaseline, [])).toBe(ALL_PERMISSIONS);
  });

  it('grants nothing when there is no default role', () => {
    expect(computeMask([moderator], [])).toBe(0);
    expect(computeMask([], [1, 2])).toBe(0);
  });

  it('is not confused by a duplicated assignment', () => {
    expect(computeMask(defs, [moderator.id, moderator.id])).toBe(
      everyone.permissions | moderator.permissions
    );
  });
});

describe('computeTopPosition', () => {
  const defs = [everyone, moderator, emojiEditor, owner];

  it('floors at the default role position', () => {
    const defaultAtThree = [role({ ...everyone, position: 3 })];
    expect(computeTopPosition(defaultAtThree, [])).toBe(3);
    expect(computeTopPosition(defs, [])).toBe(0);
  });

  it('takes the highest position across the assigned roles', () => {
    expect(computeTopPosition(defs, [emojiEditor.id])).toBe(2);
    expect(computeTopPosition(defs, [emojiEditor.id, moderator.id])).toBe(5);
    expect(computeTopPosition(defs, [moderator.id, emojiEditor.id])).toBe(5);
  });

  it('places Administrator above everyone', () => {
    // Hierarchy checks compare positions, so the owner has to outrank a role
    // that sits numerically higher than theirs.
    const aboveOwner = role({ id: 5, position: 99 });
    expect(computeTopPosition([...defs, aboveOwner], [owner.id])).toBe(Number.POSITIVE_INFINITY);
    expect(computeTopPosition([...defs, aboveOwner], [aboveOwner.id])).toBe(99);
    expect(computeTopPosition([...defs, aboveOwner], [owner.id, aboveOwner.id])).toBe(
      Number.POSITIVE_INFINITY
    );
  });

  it('places Administrator granted through @everyone above everyone', () => {
    const adminBaseline = [role({ ...everyone, permissions: PERMISSIONS.ADMINISTRATOR })];
    expect(computeTopPosition(adminBaseline, [])).toBe(Number.POSITIVE_INFINITY);
  });

  it('ignores unknown role ids and a missing default role', () => {
    expect(computeTopPosition(defs, [999])).toBe(0);
    expect(computeTopPosition([moderator], [])).toBe(0);
  });
});

describe('channel overrides', () => {
  const states: OverrideState[] = ['inherit', 'allow', 'deny'];

  it('reads the tri-state out of an allow/deny pair', () => {
    expect(overrideState(0, 0, PERMISSIONS.SEND_MESSAGES)).toBe('inherit');
    expect(overrideState(PERMISSIONS.SEND_MESSAGES, 0, PERMISSIONS.SEND_MESSAGES)).toBe('allow');
    expect(overrideState(0, PERMISSIONS.SEND_MESSAGES, PERMISSIONS.SEND_MESSAGES)).toBe('deny');
  });

  it('round-trips every state through apply', () => {
    for (const state of states) {
      const { allow, deny } = applyOverrideState(0, 0, PERMISSIONS.VIEW_CHANNELS, state);
      expect(overrideState(allow, deny, PERMISSIONS.VIEW_CHANNELS)).toBe(state);
    }
  });

  it('never leaves a flag both allowed and denied', () => {
    let masks = { allow: 0, deny: 0 };
    for (const state of [...states, ...states.slice().reverse()]) {
      masks = applyOverrideState(masks.allow, masks.deny, PERMISSIONS.SEND_MESSAGES, state);
      expect(masks.allow & masks.deny).toBe(0);
    }
  });

  it('leaves the other flags alone', () => {
    const start = applyOverrideState(0, 0, PERMISSIONS.VIEW_CHANNELS, 'allow');
    const next = applyOverrideState(
      start.allow,
      start.deny,
      PERMISSIONS.SEND_MESSAGES,
      'deny'
    );
    expect(overrideState(next.allow, next.deny, PERMISSIONS.VIEW_CHANNELS)).toBe('allow');
    expect(overrideState(next.allow, next.deny, PERMISSIONS.SEND_MESSAGES)).toBe('deny');
  });

  it('covers exactly "see" and "write/talk"', () => {
    // Widening this would let a channel override grant e.g. BAN_MEMBERS, which
    // the server's CHANNEL_OVERRIDABLE does not accept.
    expect(CHANNEL_OVERRIDABLE).toBe(PERMISSIONS.VIEW_CHANNELS | PERMISSIONS.SEND_MESSAGES);
  });
});

describe('the flag table itself', () => {
  it('assigns every flag a distinct single bit', () => {
    const flags = Object.values(PERMISSIONS);
    expect(new Set(flags).size).toBe(flags.length);
    for (const flag of flags) {
      expect(flag & (flag - 1)).toBe(0);
      expect(flag).toBeGreaterThan(0);
    }
  });

  it('unions into ALL_PERMISSIONS', () => {
    expect(ALL_PERMISSIONS).toBe(Object.values(PERMISSIONS).reduce((acc, bit) => acc | bit, 0));
    for (const flag of Object.values(PERMISSIONS)) {
      expect(ALL_PERMISSIONS & flag).toBe(flag);
    }
  });

  it('shows every flag exactly once in the dashboard groups', () => {
    // A flag missing from the groups is a permission no owner can ever toggle.
    const grouped = PERMISSION_GROUPS.flatMap((group) => group.permissions);
    expect(grouped.map((p) => p.key).sort()).toEqual(Object.keys(PERMISSIONS).sort());
    for (const entry of grouped) {
      expect(entry.flag).toBe(PERMISSIONS[entry.key]);
      expect(entry.label.length).toBeGreaterThan(0);
      expect(entry.description.length).toBeGreaterThan(0);
    }
  });
});
