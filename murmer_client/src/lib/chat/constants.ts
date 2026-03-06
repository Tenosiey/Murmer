import type { ChannelNotificationPreference } from '../stores/channelNotifications';

export const MODERATOR_ROLES = ['Admin', 'Mod', 'Owner'] as const;

export const MESSAGE_INPUT_MAX_HEIGHT = 360;
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
    description: 'Update the current channel topic or clear it when run without text.'
  },
  {
    usage: '/status <online|away|busy|offline>',
    description: 'Change your presence indicator across all connected clients.'
  },
  { usage: '/focus', description: 'Toggle focus mode for a distraction-free chat view.' },
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

