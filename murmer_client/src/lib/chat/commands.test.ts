import { describe, expect, it } from 'vitest';
import { parseSlashCommand } from './commands';
import { MAX_EPHEMERAL_SECONDS, MAX_TOPIC_LENGTH, MIN_EPHEMERAL_SECONDS } from './constants';

/**
 * Every branch here ends in either a frame to the server or a line of
 * feedback under the composer. Before this module the only way to reach the
 * usage errors and clamps was typing them into the running app.
 */
const now = new Date('2026-09-23T12:00:00Z');
const parse = (line: string) => parseSlashCommand(line, now);

describe('parseSlashCommand', () => {
  it('ignores a bare slash and is case-insensitive about the name', () => {
    expect(parse('/')).toEqual({ kind: 'none' });
    expect(parse('/HELP')).toEqual({ kind: 'help' });
  });

  it('names an unknown command back to the user', () => {
    expect(parse('/frobnicate now')).toEqual({ kind: 'error', message: 'Unknown command: /frobnicate' });
  });

  it('turns /me and /shrug into plain messages', () => {
    expect(parse('/me waves')).toEqual({ kind: 'send', text: '_waves_' });
    expect(parse('/me')).toMatchObject({ kind: 'error' });
    expect(parse('/shrug ok')).toEqual({ kind: 'send', text: 'ok ¯\\\\\\_(ツ)\\_/¯' });
  });

  it('allows clearing the topic but not an oversized one', () => {
    expect(parse('/topic')).toEqual({ kind: 'topic', topic: '' });
    expect(parse(`/topic ${'x'.repeat(MAX_TOPIC_LENGTH + 1)}`)).toMatchObject({ kind: 'error' });
  });

  it('accepts only the known statuses', () => {
    expect(parse('/status Away')).toEqual({ kind: 'status', status: 'away' });
    expect(parse('/status asleep')).toMatchObject({ kind: 'error' });
  });

  describe('/ephemeral', () => {
    it('needs a duration and a message', () => {
      expect(parse('/ephemeral')).toMatchObject({ kind: 'error' });
      expect(parse('/ephemeral 30')).toMatchObject({ kind: 'error' });
      expect(parse('/temp soon hello')).toMatchObject({ kind: 'error' });
      expect(parse('/temp 0 hello')).toMatchObject({ kind: 'error' });
    });

    it('clamps into the allowed window and says so', () => {
      expect(parse('/temp 30 hello there')).toEqual({
        kind: 'ephemeral',
        text: 'hello there',
        seconds: 30,
        clampNote: null
      });
      expect(parse('/temp 1 hi')).toMatchObject({ seconds: MIN_EPHEMERAL_SECONDS, clampNote: expect.stringContaining('Minimum') });
      expect(parse(`/temp ${MAX_EPHEMERAL_SECONDS + 1} hi`)).toMatchObject({
        seconds: MAX_EPHEMERAL_SECONDS,
        clampNote: expect.stringContaining('Maximum')
      });
    });
  });

  describe('/remind and /schedule', () => {
    it('resolve the time against now', () => {
      expect(parse('/remind 15m stretch')).toEqual({
        kind: 'remind',
        text: 'stretch',
        at: new Date('2026-09-23T12:15:00Z')
      });
      expect(parse('/schedule 2h notes are up')).toEqual({
        kind: 'schedule',
        text: 'notes are up',
        at: new Date('2026-09-23T14:00:00Z')
      });
    });

    it('refuse a missing part or an unreadable time', () => {
      expect(parse('/remind 15m')).toMatchObject({ kind: 'error' });
      expect(parse('/schedule soonish hi')).toMatchObject({ kind: 'error', message: expect.stringContaining('soonish') });
    });

    it('refuse a time outside the server window', () => {
      expect(parse('/remind 1s too soon')).toMatchObject({ kind: 'error' });
    });
  });
});
