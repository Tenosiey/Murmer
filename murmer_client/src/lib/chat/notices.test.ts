/**
 * These texts are the only trace a moderation action leaves on the screen of
 * the member it hit, and they are built from untrusted frames: a malformed
 * one must say nothing rather than "undefined has been muted".
 */
import { describe, expect, it } from 'vitest';
import type { Message } from '../types';
import { NOTICES, describeForceDisconnect } from './notices';

function frame(type: string, fields: Record<string, unknown> = {}): Message {
  return { type, ...fields };
}

describe('NOTICES', () => {
  it('tells the muted member, as an error, and everyone else plainly', () => {
    const mute = frame('user-muted', { user: 'bob', until: '2026-10-05T12:00:00Z' });
    expect(NOTICES['user-muted'](mute, 'bob')).toEqual({
      text: expect.stringMatching(/^You have been muted until .+\.$/),
      type: 'error'
    });
    expect(NOTICES['user-muted'](mute, 'alice')).toEqual({
      text: 'bob has been muted.',
      type: 'info'
    });
  });

  it('says nothing for a frame without an account name', () => {
    for (const type of ['user-muted', 'user-unmuted', 'user-unbanned']) {
      expect(NOTICES[type](frame(type, { user: 42 }), 'alice')).toBeNull();
    }
  });

  it('names the automod rule but falls back when the frame has none', () => {
    expect(NOTICES['automod-warning'](frame('automod-warning', { rule: ' links ' }), 'a')?.text).toBe(
      'Your message was flagged by the “links” rule.'
    );
    expect(NOTICES['automod-warning'](frame('automod-warning', { rule: '' }), 'a')?.text).toBe(
      'Your message was flagged by an auto-moderation rule.'
    );
  });

  it('credits the moderator behind a Danger Zone action only when named', () => {
    expect(NOTICES['server-reset'](frame('server-reset', { by: 'root' }), 'a')?.text).toBe(
      'This server was reset by root.'
    );
    expect(NOTICES['messages-purged'](frame('messages-purged', { by: 7 }), 'a')?.text).toBe(
      'Every message on this server was deleted.'
    );
  });
});

describe('describeForceDisconnect', () => {
  it('only answers the frame aimed at this client', () => {
    const ban = frame('force-disconnect', { user: 'bob', action: 'banned', by: 'mod' });
    expect(describeForceDisconnect(ban, 'bob')).toBe('You were banned from this server by mod.');
    expect(describeForceDisconnect(ban, 'alice')).toBeNull();
    expect(describeForceDisconnect(frame('force-disconnect'), null)).toBeNull();
  });

  it('treats anything but a ban as a kick', () => {
    const kick = frame('force-disconnect', { user: 'bob', action: 'kicked' });
    expect(describeForceDisconnect(kick, 'bob')).toBe('You were kicked from this server.');
  });
});
