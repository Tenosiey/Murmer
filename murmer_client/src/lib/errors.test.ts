/**
 * The error table is a lookup, so what can break is not the lookup but the
 * table: a code the server sends and the client has no entry for degrades to
 * the raw identifier, and a fatal code missing from the fatal set leaves the
 * user staring at a reconnect prompt for a connection the server already
 * closed (or, worse, reconnecting into a ban in a loop).
 */
import { describe, expect, it } from 'vitest';
import { describeServerError, isFatalConnectionError } from './errors';

/** Codes the server sends immediately before closing the socket. */
const FATAL_CODES = [
  'unauthenticated',
  'invalid-password',
  'auth-rate-limit',
  'invalid-timestamp',
  'replay-attack',
  'invalid-signature',
  'invalid-signature-format',
  'invalid-public-key',
  'invalid-key-length',
  'invalid-encoding',
  'invalid-username',
  'username-taken',
  'banned'
];

describe('describeServerError', () => {
  it('translates known codes into prose', () => {
    expect(describeServerError('invalid-password')).toBe('The server password is incorrect.');
    expect(describeServerError('banned')).toBe('You are banned from this server.');
    expect(describeServerError('slow-mode')).toBe(
      'Slow mode is on — wait a moment before sending another message.'
    );
  });

  it('falls back to the raw code so an unmapped error is still diagnosable', () => {
    expect(describeServerError('brand-new-code')).toBe(
      'The server reported an error: brand-new-code'
    );
    expect(describeServerError('')).toBe('The server reported an error: ');
  });

  it('never returns an empty or untranslated-looking message for a known code', () => {
    for (const code of FATAL_CODES) {
      const message = describeServerError(code);
      expect(message.trim()).not.toBe('');
      expect(message, code).not.toContain('The server reported an error:');
    }
  });

  it('does not inherit prototype properties as error messages', () => {
    // The code comes straight off the wire and indexes a plain object literal,
    // so `toString` and friends must miss rather than return a function.
    for (const code of ['toString', 'constructor', '__proto__', 'hasOwnProperty']) {
      expect(describeServerError(code)).toBe(`The server reported an error: ${code}`);
    }
  });
});

describe('isFatalConnectionError', () => {
  it('flags every code the server closes the connection after', () => {
    for (const code of FATAL_CODES) {
      expect(isFatalConnectionError(code), code).toBe(true);
    }
  });

  it('leaves recoverable errors alone', () => {
    // These arrive on a live connection — treating one as fatal would kick the
    // user back to the server list mid-conversation.
    const recoverable = [
      'message-rate-limit',
      'message-too-long',
      'slow-mode',
      'muted',
      'unknown-channel',
      'send-permission-denied',
      'wiki-save-failed',
      'soundboard-cooldown',
      'not-authenticated',
      'unknown-code'
    ];
    for (const code of recoverable) {
      expect(isFatalConnectionError(code), code).toBe(false);
    }
  });
});
