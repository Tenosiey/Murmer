/**
 * The lookup is what every screen renders through, and its failures are
 * quiet: a placeholder that is not filled, a plural form that is never
 * picked, or a key built from server input that resolves to an inherited
 * property instead of a message.
 */
import { describe, expect, it } from 'vitest';
import { en } from './en';
import { hasMessage, t } from './index';

describe('t', () => {
  it('returns the English message', () => {
    expect(t('login.title')).toBe('Sign in to Murmer');
  });

  it('fills every placeholder and leaves unknown ones visible', () => {
    expect(t('servers.remove', { name: 'Lobby' })).toBe('Remove Lobby');
    expect(t('servers.remove')).toBe('Remove {name}');
  });

  it('picks a plural form by count', () => {
    expect(t('servers.savedCount', { count: 1 })).toBe('1 saved');
    expect(t('servers.savedCount', { count: 3 })).toBe('3 saved');
  });
});

describe('hasMessage', () => {
  it('rejects inherited properties', () => {
    expect(hasMessage('error.banned')).toBe(true);
    expect(hasMessage('toString')).toBe(false);
    expect(hasMessage('constructor')).toBe(false);
  });
});

describe('catalog', () => {
  it('uses only placeholders t can fill', () => {
    // A `${name}` left over from a template literal would render literally.
    const messages = Object.values(en).flatMap((m) => (typeof m === 'string' ? [m] : Object.values(m)));
    expect(messages.filter((m) => m.includes('${'))).toEqual([]);
  });
});
