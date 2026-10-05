/**
 * The failure this guards against is silent: a mention that renders fine but
 * notifies nobody. So every insertion is checked with the real
 * `containsMention`, the function that decides whether the target hears
 * about it.
 */
import { describe, expect, it } from 'vitest';
import { containsMention } from '../message-utils';
import type { RoleDef } from '../types';
import {
  groupCandidates,
  groupMentionsIn,
  insertMention,
  mentionCandidates,
  mentionQuery,
  parseGroupMentions,
  pingsMe
} from './mentions';

const NAMES: Record<string, string> = {
  anna_s: 'Anna Smith',
  bob: 'Captain',
  'mary jane': 'mary jane',
  jo: 'Jo',
  me: 'Me'
};
const shown = (user: string) => NAMES[user] ?? user;

describe('mentionQuery', () => {
  it('finds the @ the caret is typing after', () => {
    expect(mentionQuery('hi @Cap', 7)).toEqual({ start: 3, query: 'Cap' });
    expect(mentionQuery('@', 1)).toEqual({ start: 0, query: '' });
    expect(mentionQuery('(@an', 4)).toEqual({ start: 1, query: 'an' });
  });

  it('keeps spaces, since names have them', () => {
    expect(mentionQuery('@mary j', 7)).toEqual({ start: 0, query: 'mary j' });
  });

  it('ignores an @ inside a word, an address, or a doubled @', () => {
    expect(mentionQuery('mail bob@ex', 11)).toBeNull();
    expect(mentionQuery('@@bob', 5)).toBeNull();
    expect(mentionQuery('@ bob', 5)).toBeNull();
  });

  it('does not reach across a line break or past the longest name', () => {
    expect(mentionQuery('@bob\nhi', 7)).toBeNull();
    expect(mentionQuery(`@${'x'.repeat(40)}`, 41)).toBeNull();
  });
});

describe('mentionCandidates', () => {
  const pick = (query: string, online: string[], offline: string[] = []) =>
    mentionCandidates(query, online, offline, shown, 'me').map((c) => c.insert);

  it('finds members by the name the UI shows, not only the account name', () => {
    expect(pick('cap', ['bob', 'anna_s'])).toEqual(['bob']);
    expect(pick('smi', ['bob', 'anna_s'])).toEqual(['anna_s']);
    expect(pick('bo', ['bob'])).toEqual(['bob']);
  });

  it('ranks a shown-name prefix first, then online before offline', () => {
    // "Jo" starts with the query; "mary jane" only has a word that does, so
    // Jo wins even though they are offline.
    expect(pick('j', ['mary jane'], ['jo'])).toEqual(['jo', 'mary jane']);
    expect(pick('', ['bob'], ['anna_s'])).toEqual(['bob', 'anna_s']);
  });

  it('never offers the current user', () => {
    expect(pick('m', ['me', 'mary jane'])).toEqual(['mary jane']);
  });

  it('stops matching once the user has typed past the name', () => {
    expect(pick('captain how are you', ['bob'])).toEqual([]);
  });
});

describe('insertMention', () => {
  it('inserts the account name, which is what actually notifies', () => {
    // The bug: "@Captain" reads right and notifies nobody.
    const text = 'hey @Cap';
    const result = insertMention(text, mentionQuery(text, 8)!, 8, 'bob');
    expect(result).toEqual({ text: 'hey @bob ', caret: 9 });
    expect(containsMention(result.text, 'bob')).toBe(true);
    expect(containsMention('hey @Captain', 'bob')).toBe(false);
  });

  it('keeps account names with spaces and underscores mentionable', () => {
    for (const user of ['mary jane', 'anna_s', 'x-ray']) {
      const result = insertMention('@m', { start: 0, query: 'm' }, 2, user);
      expect(containsMention(`${result.text}thanks`, user)).toBe(true);
    }
  });

  it('completes mid-sentence without doubling the following space', () => {
    const text = 'ask @an about it';
    const result = insertMention(text, mentionQuery(text, 7)!, 7, 'anna_s');
    expect(result.text).toBe('ask @anna_s about it');
    expect(result.caret).toBe('ask @anna_s '.length);
  });
});

function role(id: number, name: string, isDefault = false): RoleDef {
  return { id, name, permissions: 0, position: id, isDefault, isOwner: false };
}

const ROLES = [role(1, '@everyone', true), role(2, 'Mod'), role(3, 'Event Team')];

describe('groupCandidates', () => {
  const names = (query: string) => groupCandidates(query, ROLES).map((c) => c.label);

  it('offers @here and roles by name, never @everyone', () => {
    expect(names('')).toEqual(['here', 'Event Team', 'Mod']);
    expect(names('h')).toEqual(['here']);
    expect(names('te')).toEqual(['Event Team']);
    expect(names('every')).toEqual([]);
  });
});

describe('group mentions', () => {
  it('derives the field from the text the way a member mention is matched', () => {
    expect(groupMentionsIn('@here and @mod', ROLES)).toEqual({ here: true, roles: [2] });
    expect(groupMentionsIn('ping @Event Team now', ROLES)).toEqual({ here: false, roles: [3] });
    expect(groupMentionsIn('mail me@here.example', ROLES)).toBeNull();
    expect(groupMentionsIn('@heresy', ROLES)).toBeNull();
    // @everyone is not a group anyone can ping.
    expect(groupMentionsIn('@@everyone', ROLES)).toBeNull();
  });

  it('an inserted group is one the sender\'s field picks up', () => {
    const result = insertMention('@eve', { start: 0, query: 'eve' }, 4, 'Event Team');
    expect(groupMentionsIn(`${result.text}meet at 5`, ROLES)).toEqual({ here: false, roles: [3] });
  });

  it('reads only the shape the server builds off the wire', () => {
    expect(parseGroupMentions({ here: true, roles: [2] })).toEqual({ here: true, roles: [2] });
    expect(parseGroupMentions({ here: 'yes', roles: ['2'] })).toBeNull();
    expect(parseGroupMentions(['here'])).toBeNull();
    expect(parseGroupMentions(null)).toBeNull();
  });

  it('pings everyone for @here and role holders for a role', () => {
    expect(pingsMe({ here: true, roles: [] }, [])).toBe(true);
    expect(pingsMe({ here: false, roles: [2] }, [2, 3])).toBe(true);
    expect(pingsMe({ here: false, roles: [2] }, [3])).toBe(false);
    expect(pingsMe(null, [2])).toBe(false);
  });
});
