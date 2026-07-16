import type { ChannelNotificationPreference } from '../stores/channelNotifications';

export const MODERATOR_ROLES = ['Admin', 'Mod', 'Owner'] as const;

/** Mirror of the server's moderation ranking (Owner > Admin > Mod > none). */
export function roleRank(role: string | null | undefined): number {
  switch (role?.toLowerCase()) {
    case 'owner':
      return 3;
    case 'admin':
      return 2;
    case 'mod':
      return 1;
    default:
      return 0;
  }
}

/* Custom server emoji naming rules; must match the server's validation. */
export const EMOJI_NAME_RE = /^[a-z0-9_]{2,32}$/;
export const EMOJI_SHORTCODE_RE = /^:([a-z0-9_]{2,32}):$/;
export const MAX_EMOJI_FILE_BYTES = 512 * 1024;

/* Server identity limits; must match the server's validation. */
export const MAX_SERVER_NAME_LENGTH = 64;
export const MAX_SERVER_DESCRIPTION_LENGTH = 300;
export const MAX_WELCOME_MESSAGE_LENGTH = 500;
export const MAX_SERVER_ICON_BYTES = 1024 * 1024;

/* The channel every server is seeded with. The server places new connections
   into it and refuses to delete it, so the client can rely on it existing. */
export const DEFAULT_CHANNEL_NAME = 'general';

export const MESSAGE_INPUT_MAX_HEIGHT = 360;
export const MAX_TOPIC_LENGTH = 256;
export const PIN_PREVIEW_LIMIT = 120;
export const MIN_EPHEMERAL_SECONDS = 5;
export const MAX_EPHEMERAL_SECONDS = 86_400;

export const VOICE_QUALITY_PRESETS: Array<{
  quality: string;
  bitrate: number | null;
  label: string;
}> = [
  { quality: 'low', bitrate: 32_000, label: 'Low' },
  { quality: 'standard', bitrate: 64_000, label: 'Standard' },
  { quality: 'high', bitrate: 96_000, label: 'High' },
  { quality: 'ultra', bitrate: 128_000, label: 'Ultra' },
  { quality: 'lossless', bitrate: null, label: 'Lossless' }
];

export const DEFAULT_VOICE_PRESET = VOICE_QUALITY_PRESETS[1];

export const NOTIFICATION_OPTIONS: Array<{
  value: ChannelNotificationPreference;
  label: string;
  description: string;
  icon: string;
}> = [
  { value: 'all', label: 'All messages', description: 'Send alerts for every new message', icon: '🔔' },
  {
    value: 'mentions',
    label: 'Mentions only',
    description: 'Only alert when you are mentioned',
    icon: '@'
  },
  {
    value: 'mute',
    label: 'Muted',
    description: 'Do not show notifications for this channel',
    icon: '🔕'
  }
];

export const HELP_COMMANDS: Array<{
  usage: string;
  description: string;
  aliases?: string[];
}> = [
  { usage: '/help', description: 'Show this list of available slash commands.' },
  { usage: '/me <action>', description: 'Send an italicised third-person emote.' },
  {
    usage: '/shrug [message]',
    description: 'Append the classic shrug emoticon to your message.'
  },
  {
    usage: '/topic <text>',
    description:
      'Update the channel topic for everyone on the server or clear it when run without text.'
  },
  {
    usage: '/status <online|away|busy|offline>',
    description: 'Change your presence indicator across all connected clients.'
  },
  {
    usage: '/ephemeral <seconds> <message>',
    description:
      'Send a message that automatically deletes itself after the requested duration.',
    aliases: ['/temp <seconds> <message>']
  },
  {
    usage: '/search [query]',
    description: 'Open the search overlay and optionally pre-fill it with a query.'
  }
];

