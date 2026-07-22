/**
 * Translation of server error codes (the `message` field of
 * `{"type":"error"}` frames) into user-facing text.
 */

const SERVER_ERROR_MESSAGES: Record<string, string> = {
  unauthenticated: 'You are not authenticated with this server.',
  'invalid-password': 'The server password is incorrect.',
  'auth-rate-limit': 'Too many connection attempts. Please wait a moment and try again.',
  'invalid-timestamp': 'Authentication failed: your system clock appears to be wrong.',
  'replay-attack': 'Authentication failed. Please try connecting again.',
  'invalid-signature': 'Authentication failed: invalid signature.',
  'invalid-signature-format': 'Authentication failed: invalid signature.',
  'invalid-public-key': 'Authentication failed: invalid key.',
  'invalid-key-length': 'Authentication failed: invalid key.',
  'invalid-encoding': 'Authentication failed: invalid key encoding.',
  'invalid-username': 'That username is not allowed on this server.',
  'username-taken': 'That username is already in use by someone else on this server.',
  banned: 'You are banned from this server.',
  'invalid-channel-name': 'That channel name is not allowed.',
  'channel-permission-denied': 'You do not have permission to manage channels on this server.',
  'channel-creation-failed': 'The server could not create the channel. Please try again.',
  'channel-deletion-failed': 'The server could not delete the channel. Please try again.',
  'cannot-delete-general': 'The general channel cannot be deleted.',
  'unknown-channel': 'That channel no longer exists.',
  'message-rate-limit': 'You are sending messages too quickly. Please slow down.',
  'message-too-long': 'That message is too long to send.',
  'invalid-voice-quality': 'Invalid voice quality setting.',
  'invalid-voice-bitrate': 'Invalid voice bitrate setting.',
  'unknown-voice-channel': 'That voice channel no longer exists.',
  'voice-channel-update-failed': 'The server could not update the voice channel.',
  'role-permission-denied': 'You do not have permission to manage roles on this server.',
  'role-target-not-found': 'That user is not connected to the server.',
  'role-update-failed': 'The server could not update the role. Please try again.',
  'role-not-found': 'That role no longer exists.',
  'role-name-taken': 'A role with that name already exists.',
  'role-protected': 'That role is protected and cannot be changed.',
  'role-limit-reached': 'This server has reached its role limit.',
  'invalid-role-name': 'That role name is not allowed.',
  'invalid-role-color': 'That role color is not a valid hex color.',
  'invalid-role-permissions': 'Those permissions are not valid.',
  'send-permission-denied': 'You do not have permission to send messages in this channel.',
  'channel-override-permission-denied':
    'You do not have permission to edit this channel’s permissions.',
  'invalid-channel-override': 'That channel permission change was invalid.',
  'override-target-not-found': 'That role or user could not be found.',
  'channel-override-failed': 'The server could not update the channel permissions. Please try again.',
  'invalid-category-name': 'That category name is not allowed.',
  'category-creation-failed': 'The server could not create the category. Please try again.',
  'category-rename-failed': 'The server could not rename the category. Please try again.',
  'category-deletion-failed': 'The server could not delete the category. Please try again.',
  'unknown-category': 'That category no longer exists.',
  'channel-move-failed': 'The server could not move the channel. Please try again.',
  'invalid-channel-topic': 'That channel topic is not allowed.',
  'topic-update-failed': 'The server could not update the channel topic.',
  'moderation-permission-denied': 'You do not have permission for that moderation action.',
  'moderation-target-not-found': 'That user is not connected to the server.',
  'moderation-target-protected': 'That user cannot be moderated by you.',
  'cannot-moderate-self': 'You cannot moderate yourself.',
  'moderation-failed': 'The server could not complete the moderation action.',
  muted: 'You are muted and cannot send messages right now.',
  'not-authenticated': 'You are not authenticated with this server.',
  'invalid-status': 'That status is not recognised.',
  'invalid-message-id': 'That message could not be found.',
  'message-not-found': 'That message no longer exists.',
  'message-wrong-channel': 'That message belongs to a different channel.',
  'message-permission-denied': 'You do not have permission to modify that message.',
  'message-delete-failed': 'The server could not delete the message. Please try again.',
  'message-edit-failed': 'The server could not edit the message. Please try again.',
  'invalid-message-text': 'That message text is not allowed.',
  'invalid-reaction-action': 'That reaction could not be applied.',
  'invalid-emoji': 'That emoji is not allowed.',
  'reaction-failed': 'The server could not update the reaction. Please try again.',
  'dm-target-not-found': 'That user is not known on this server.',
  'invalid-dm-payload': 'That direct message could not be sent (malformed encrypted payload).',
  'cannot-dm-self': 'You cannot send a direct message to yourself.',
  'dm-send-failed': 'The server could not deliver your direct message.',
  'dm-history-failed': 'The server could not load that conversation.',
  'pin-target-not-found': 'The message you tried to pin no longer exists.',
  'pin-limit-reached': 'This channel already has the maximum number of pinned messages.',
  'pin-failed': 'The server could not update the pinned messages.',
  'reply-target-not-found': 'The message you are replying to no longer exists.',
  'thread-load-failed': 'The server could not load that thread. Please try again.',
  'emoji-permission-denied': 'You do not have permission to manage emojis on this server.',
  'invalid-emoji-name':
    'That emoji name is not allowed. Use 2-32 lowercase letters, digits or underscores.',
  'invalid-emoji-url': 'The uploaded emoji image could not be used. Please try again.',
  'emoji-name-taken': 'An emoji with that name already exists on this server.',
  'emoji-limit-reached': 'This server has reached its custom emoji limit.',
  'emoji-update-failed': 'The server could not update the emoji list. Please try again.',
  'emoji-not-found': 'That emoji no longer exists.',
  'identity-permission-denied': 'You do not have permission to edit the server identity.',
  'invalid-server-name': 'That server name is not allowed.',
  'invalid-server-description': 'That server description is not allowed.',
  'invalid-welcome-message': 'That welcome message is not allowed.',
  'invalid-server-icon': 'The uploaded server icon could not be used. Please try again.',
  'identity-update-failed': 'The server could not update the server identity. Please try again.',
  'screenshare-permission-denied':
    'You do not have permission to change screen share settings on this server.',
  'invalid-screenshare-bitrate': 'Invalid screen share bitrate setting.',
  'screenshare-update-failed':
    'The server could not update the screen share settings. Please try again.',
  'stats-permission-denied': 'You do not have permission to change stat tracking on this server.',
  'stats-not-available': "That user's stats are not available.",
  'stats-update-failed': 'The server could not update stat tracking. Please try again.',
  'invalid-wiki-slug':
    'That page name is not allowed. Use lowercase letters, digits and single dashes.',
  'invalid-wiki-title': 'That page title is not allowed.',
  'wiki-body-too-large': 'That wiki page is too large to save.',
  'wiki-slug-taken': 'A wiki page with that name already exists in this channel.',
  'wiki-page-not-found': 'That wiki page no longer exists.',
  'wiki-page-limit-reached': 'This channel has reached its wiki page limit.',
  'wiki-save-failed': 'The server could not update the wiki. Please try again.'
};

/**
 * Error codes after which the server closes the connection; the client
 * should return to the server list instead of showing a reconnect prompt.
 */
const FATAL_CONNECTION_ERRORS = new Set([
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
]);

/** Convert a server error code into a message suitable for display. */
export function describeServerError(code: string): string {
  return SERVER_ERROR_MESSAGES[code] ?? `The server reported an error: ${code}`;
}

/** Whether the error ends the connection (auth rejection, ban, ...). */
export function isFatalConnectionError(code: string): boolean {
  return FATAL_CONNECTION_ERRORS.has(code);
}
