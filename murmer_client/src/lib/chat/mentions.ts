/**
 * Mention completion for the message fields.
 *
 * A mention only notifies anyone when it names the **account**:
 * `containsMention` (`message-utils.ts`) matches `@accountname` and nothing
 * else, because display names and nicknames are decoration and deliberately
 * not unique. The UI, though, shows people by those decorations — so a user
 * typing the name they can see, `@Nick`, used to notify nobody. Completion
 * closes that gap: members are found by what the UI calls them, and what
 * lands in the message is the account name.
 */

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
  /** Account name — what gets inserted and what `containsMention` matches. */
  user: string;
  /** What the UI shows for them: nickname, display name or account name. */
  label: string;
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
  const ranked: Array<MentionCandidate & { rank: number; away: number }> = [];
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
  return ranked.slice(0, limit).map(({ user, label }) => ({ user, label }));
}

/**
 * Replace the `@query` that ends at `caret` with `@<account> `, returning the
 * new text and where the caret goes. The trailing space ends the mention for
 * `containsMention`; it is skipped when the text already continues with one.
 */
export function insertMention(
  text: string,
  mention: MentionQuery,
  caret: number,
  user: string
): { text: string; caret: number } {
  const after = text.slice(caret);
  const inserted = `@${user}${after.startsWith(' ') ? '' : ' '}`;
  return {
    text: text.slice(0, mention.start) + inserted + after,
    caret: mention.start + inserted.length + (after.startsWith(' ') ? 1 : 0)
  };
}
