/**
 * Translation of server error codes (the `message` field of
 * `{"type":"error"}` frames) into user-facing text. The text itself lives in
 * the i18n catalog under `error.<code>`.
 */
import { MAX_POLL_OPTION_LENGTH, MAX_POLL_OPTIONS } from './chat/constants';
import { hasMessage, t } from './i18n';

/**
 * Error codes after which the server closes the connection; the client
 * should return to the server list instead of showing a reconnect prompt.
 */
const FATAL_CONNECTION_ERRORS = new Set([
  'unauthenticated',
  'invalid-password',
  'invalid-invite',
  'auth-rate-limit',
  'invalid-signature',
  'invalid-signature-format',
  'invalid-public-key',
  'invalid-key-length',
  'invalid-encoding',
  'invalid-username',
  'username-taken',
  'banned',
  'login-failed'
]);

/** Convert a server error code into a message suitable for display. */
export function describeServerError(code: string): string {
  // The code arrives off the wire, so it may name an inherited property
  // (`toString`, `constructor`, ...). `hasMessage` is an own-property lookup,
  // which keeps those on the fallback path instead of handing the UI a
  // function to render.
  const key = `error.${code}`;
  return hasMessage(key)
    ? t(key, { maxPollOptions: MAX_POLL_OPTIONS, maxPollOptionLength: MAX_POLL_OPTION_LENGTH })
    : t('error.unknown', { code });
}

/** Whether the error ends the connection (auth rejection, ban, ...). */
export function isFatalConnectionError(code: string): boolean {
  return FATAL_CONNECTION_ERRORS.has(code);
}
