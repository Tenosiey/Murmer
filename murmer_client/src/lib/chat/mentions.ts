/**
 * Mentions: completing them in the message fields, and the group pings.
 *
 * A mention only notifies anyone when it names the **account**:
 * `containsMention` (`message-utils.ts`) matches `@accountname` and nothing
 * else, because display names and nicknames are decoration and deliberately
 * not unique. The UI, though, shows people by those decorations — so a user
 * typing the name they can see, `@Nick`, used to notify nobody. Completion
 * closes that gap: members are found by what the UI calls them, and what
 * lands in the message is the account name.
 *
 * **Group mentions** — `@here` and `@<role>` — work the other way round. The
 * text alone pings nobody, because anyone may type it; a message pings a
 * group only through its `mentions` field, which the server authorizes
 * against `MENTION_GROUPS` and rebuilds. That field is also the one thing
 * that still works in an encrypted channel, where the server reads no text.
 * So the sender's client derives the field from the text
 * (`groupMentionsIn`), and a recipient pings on the field alone
 * (`parseGroupMentions`, `pingsMe`).
 */
import { containsMention } from '../message-utils';
import type { RoleDef } from '../types';

/** Longest account or display name the server accepts. */
const MAX_NAME_LENGTH = 32;

/** How many members the suggestion list offers at once. */
export const MAX_MENTION_SUGGESTIONS = 8;

/** The `@…` being typed: where its `@` sits and what follows it so far. */
export interface MentionQuery {
  start: number;
  query: string;
}

/**
 * The mention being typed at `caret`, or null when the caret is not inside
 * one. The `@` has to start the text or follow something other than a word
 * character or another `@` — the same boundary `containsMention` uses, so an
 * e-mail address never opens the list. The query may contain spaces, since
 * names do; it just stops matching anybody once the user has typed past the
 * name, and the list closes by itself.
 */
export function mentionQuery(text: string, caret: number): MentionQuery | null {
  const floor = Math.max(0, caret - MAX_NAME_LENGTH - 1);
  for (let at = caret - 1; at >= floor; at -= 1) {
    const char = text[at];
    if (char === '\n') return null;
    if (char !== '@') continue;
    const before = at > 0 ? text[at - 1] : '';
    if (before && (/\w/.test(before) || before === '@')) return null;
    const query = text.slice(at + 1, caret);
    if (query.startsWith(' ')) return null;
    return { start: at, query };
  }
  return null;
}

export interface MentionCandidate {
  kind: 'member' | 'here' | 'role';
  /**
   * What goes after the `@`: a member's account name — the only thing
   * `containsMention` matches — or `here`, or a role's name.
   */
  insert: string;
  /** What the list shows: a member's shown name, `here`, or the role name. */
  label: string;
  /** A role's colour, if it has one. */
  color?: string;
}

/**
 * Members matching `query`, best first. A member matches when the name the
 * UI shows starts with the query, when their account name does, or when any
 * word of the shown name does ("@smi" finds "Anna Smith"). Online members
 * come before offline ones within each rank, then alphabetical by the shown
 * name. The current user is left out: mentioning yourself notifies nobody.
 */
export function mentionCandidates(
  query: string,
  online: string[],
  offline: string[],
  displayName: (user: string) => string,
  currentUser: string | null,
  limit = MAX_MENTION_SUGGESTIONS
): MentionCandidate[] {
  const needle = query.toLowerCase();
  const onlineSet = new Set(online);
  const ranked: Array<{ user: string; label: string; rank: number; away: number }> = [];
  for (const user of new Set([...online, ...offline])) {
    if (user === currentUser) continue;
    const label = displayName(user);
    const shown = label.toLowerCase();
    let rank: number;
    if (shown.startsWith(needle)) rank = 0;
    else if (user.toLowerCase().startsWith(needle)) rank = 1;
    else if (shown.split(/\s+/).some((word) => word.startsWith(needle))) rank = 2;
    else continue;
    ranked.push({ user, label, rank, away: onlineSet.has(user) ? 0 : 1 });
  }
  ranked.sort((a, b) => a.rank - b.rank || a.away - b.away || a.label.localeCompare(b.label));
  return ranked
    .slice(0, limit)
    .map(({ user, label }) => ({ kind: 'member' as const, insert: user, label }));
}

/**
 * The groups matching `query`: `@here`, then roles by name. `@everyone` is
 * left out — it is every member, offline ones included, and is not a role
 * anyone can ping. Only offered to members who may ping groups at all; the
 * server refuses the message otherwise.
 */
export function groupCandidates(query: string, roles: RoleDef[]): MentionCandidate[] {
  const needle = query.toLowerCase();
  const groups: MentionCandidate[] = [];
  if ('here'.startsWith(needle)) groups.push({ kind: 'here', insert: 'here', label: 'here' });
  const matching = roles
    .filter((role) => !role.isDefault)
    .filter((role) => {
      const name = role.name.toLowerCase();
      return name.startsWith(needle) || name.split(/\s+/).some((word) => word.startsWith(needle));
    })
    .sort((a, b) => a.name.localeCompare(b.name));
  for (const role of matching) {
    groups.push({ kind: 'role', insert: role.name, label: role.name, color: role.color });
  }
  return groups;
}

/** The groups a message pings: `@here`, and role ids. */
export interface GroupMentions {
  here: boolean;
  roles: number[];
}

/**
 * The groups `text` mentions, for the sender to put on the frame — or null
 * when it mentions none. Matched with `containsMention`, the same rule a
 * member mention follows, so `@here`, `@Mods` and `@event team` count and
 * `mail@here.example` does not.
 */
export function groupMentionsIn(text: string, roles: RoleDef[]): GroupMentions | null {
  const here = containsMention(text, 'here');
  const ids = roles
    .filter((role) => !role.isDefault && containsMention(text, role.name))
    .map((role) => role.id);
  return here || ids.length > 0 ? { here, roles: ids } : null;
}

/**
 * Read the `mentions` field of an incoming frame. It comes off the wire, so
 * anything but the shape the server builds reads as "no group pinged".
 */
export function parseGroupMentions(raw: unknown): GroupMentions | null {
  if (!raw || typeof raw !== 'object' || Array.isArray(raw)) return null;
  const { here, roles } = raw as { here?: unknown; roles?: unknown };
  const ids = Array.isArray(roles)
    ? roles.filter((id): id is number => typeof id === 'number')
    : [];
  return here === true || ids.length > 0 ? { here: here === true, roles: ids } : null;
}

/**
 * Whether a group ping reaches the signed-in user. `@here` reaches everyone
 * who receives the message at all: the server only sends it to members who
 * can see the channel, and receiving it live is what being "here" means.
 */
export function pingsMe(mentions: GroupMentions | null, ownRoleIds: number[]): boolean {
  if (!mentions) return false;
  return mentions.here || mentions.roles.some((id) => ownRoleIds.includes(id));
}

/**
 * Replace the `@query` that ends at `caret` with `@<name> `, returning the
 * new text and where the caret goes. The trailing space ends the mention for
 * `containsMention`; it is skipped when the text already continues with one.
 */
export function insertMention(
  text: string,
  mention: MentionQuery,
  caret: number,
  name: string
): { text: string; caret: number } {
  const after = text.slice(caret);
  const inserted = `@${name}${after.startsWith(' ') ? '' : ' '}`;
  return {
    text: text.slice(0, mention.start) + inserted + after,
    caret: mention.start + inserted.length + (after.startsWith(' ') ? 1 : 0)
  };
}
