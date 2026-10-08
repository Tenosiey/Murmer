import type { ChannelNotificationPreference } from '../stores/channelNotifications';
import type { UserStatus } from '../types';
import { t } from '../i18n';

/* Custom server emoji naming rules; must match the server's validation. */
export const EMOJI_NAME_RE = /^[a-z0-9_]{2,32}$/;
export const EMOJI_SHORTCODE_RE = /^:([a-z0-9_]{2,32}):$/;
export const MAX_EMOJI_FILE_BYTES = 512 * 1024;

/* Soundboard limits; must match the server's validation. The server re-checks
   all of it (size, extension, magic bytes) — this copy only rejects a file
   before uploading it and drives the file picker's `accept`. */
export const MIN_SOUND_NAME_LEN = 2;
export const MAX_SOUND_NAME_LEN = 32;
export const MAX_SOUND_FILE_BYTES = 1024 * 1024;
export const MAX_SOUNDBOARD_SOUNDS = 100;
export const SOUND_EXTENSIONS = ['mp3', 'wav', 'ogg', 'm4a', 'opus'];
export const SOUND_ACCEPT = SOUND_EXTENSIONS.map((ext) => `.${ext}`).join(',');
/** Playback cooldown per user, mirrored from the server for the UI hint. */
export const SOUNDBOARD_COOLDOWN_MS = 3_000;

/* Server identity limits; must match the server's validation. */
export const MAX_SERVER_NAME_LENGTH = 64;
export const MAX_SERVER_DESCRIPTION_LENGTH = 300;
export const MAX_WELCOME_MESSAGE_LENGTH = 500;
export const MAX_SERVER_ICON_BYTES = 1024 * 1024;

/* User avatar size limit; must match the server's validation. */
export const MAX_AVATAR_BYTES = 1024 * 1024;

/* Role icon size limit; must match the server's validation. */
export const MAX_ROLE_ICON_BYTES = 512 * 1024;

/* User profile limits; must match the server's validation. */
export const MAX_DISPLAY_NAME_LENGTH = 32;
export const MAX_NICKNAME_LENGTH = 32;
export const MAX_ABOUT_LENGTH = 300;
export const MAX_STATUS_TEXT_LENGTH = 80;

/* Upload policy mirror of `murmer_server/src/upload.rs` — the server enforces
   all of this on `/upload`; the client copy only exists to reject a file
   before uploading it and to describe the rules in the UI. Keep both in sync. */
export const MIN_UPLOAD_MAX_BYTES = 64 * 1024;
export const MAX_UPLOAD_MAX_BYTES = 100 * 1024 * 1024;
export const DEFAULT_UPLOAD_MAX_BYTES = 10 * 1024 * 1024;

export const UPLOAD_CATEGORIES: Array<{
  id: string;
  label: string;
  extensions: string[];
}> = [
  { id: 'images', label: t('uploadCategory.images'), extensions: ['jpg', 'jpeg', 'png', 'gif', 'webp'] },
  {
    id: 'documents',
    label: t('uploadCategory.documents'),
    extensions: [
      'pdf', 'txt', 'md', 'log', 'csv', 'json', 'toml', 'yaml', 'yml', 'rtf', 'doc', 'docx',
      'xls', 'xlsx', 'ppt', 'pptx', 'odt', 'ods', 'odp'
    ]
  },
  { id: 'archives', label: t('uploadCategory.archives'), extensions: ['zip', 'gz', 'tar', 'bz2', 'xz', '7z', 'rar'] },
  { id: 'audio', label: t('uploadCategory.audio'), extensions: ['mp3', 'wav', 'ogg', 'flac', 'm4a', 'opus'] },
  { id: 'video', label: t('uploadCategory.video'), extensions: ['mp4', 'webm', 'mkv', 'mov', 'avi'] }
];

/* The channel every server is seeded with. The server places new connections
   into it and refuses to delete it, so the client can rely on it existing. */
export const DEFAULT_CHANNEL_NAME = 'general';

/* Chat policy mirror of `murmer_server/src/db/chat_settings.rs`. The server
   enforces slow mode, the length cap and the profanity filter itself; these
   only bound what the dashboard offers and what the composer accepts. A
   server may configure a *lower* message cap, never a higher one. */
export const MAX_MESSAGE_LENGTH = 4000;
export const MIN_CONFIGURABLE_MESSAGE_LENGTH = 10;
export const MAX_SLOW_MODE_SECONDS = 6 * 60 * 60;
export const MAX_PROFANITY_WORDS = 200;
export const MAX_PROFANITY_WORD_LEN = 32;

/* Timed mute bounds, mirrored from `murmer_server/src/db/moderation.rs`. The
   server clamps every mute it issues, by hand or by rule; this bounds the
   duration input in the auto-moderation editor. */
export const MIN_MUTE_SECONDS = 10;
export const MAX_MUTE_SECONDS = 30 * 24 * 60 * 60;

/* Auto-moderation mirror of `murmer_server/src/automod.rs`. The server
   validates every rule it is sent, compiles the patterns itself and enforces
   the actions; these only bound what the dashboard editor offers. Each `id`
   is the wire name — the server rejects anything it does not know rather than
   falling back to a default, so these must stay exactly as spelled in Rust. */
export type AutomodKind = 'word' | 'substring' | 'regex';
export type AutomodAction = 'warn' | 'delete' | 'mute';

export const MAX_AUTOMOD_RULES = 50;
export const MAX_AUTOMOD_PATTERN_LEN = 200;
export const MAX_AUTOMOD_NAME_LEN = 48;
export const DEFAULT_AUTOMOD_MUTE_SECONDS = 300;

export const AUTOMOD_KINDS: Array<{ id: AutomodKind; label: string; description: string }> = [
  {
    id: 'word',
    label: t('automodKind.word'),
    description: t('automodKind.word.description')
  },
  { id: 'substring', label: t('automodKind.substring'), description: t('automodKind.substring.description') },
  {
    id: 'regex',
    label: t('automodKind.regex'),
    description: t('automodKind.regex.description')
  }
];

/* Ordered from least to most severe, the same order the server resolves two
   matching rules by. */
export const AUTOMOD_ACTIONS: Array<{ id: AutomodAction; label: string; description: string }> = [
  { id: 'warn', label: t('automodAction.warn'), description: t('automodAction.warn.description') },
  { id: 'delete', label: t('automodAction.delete'), description: t('automodAction.delete.description') },
  { id: 'mute', label: t('automodAction.mute'), description: t('automodAction.mute.description') }
];

export const USER_STATUS_VALUES = ['online', 'away', 'busy', 'offline'] as const;

export const STATUS_LABELS: Record<UserStatus, string> = {
  online: t('status.online'),
  away: t('status.away'),
  busy: t('status.busy'),
  offline: t('status.offline')
};

export const MESSAGE_INPUT_MAX_HEIGHT = 360;
export const MAX_TOPIC_LENGTH = 256;
export const PIN_PREVIEW_LIMIT = 120;
/** Poll bounds; mirror `MAX_POLL_OPTIONS` / `MAX_POLL_OPTION_LENGTH` in ws/constants.rs. */
export const MAX_POLL_OPTIONS = 10;
export const MAX_POLL_OPTION_LENGTH = 55;
export const MIN_EPHEMERAL_SECONDS = 5;
export const MAX_EPHEMERAL_SECONDS = 86_400;

/* Reminder and scheduling bounds, mirrored from
   `murmer_server/src/ws/constants.rs`. The server refuses a time outside them
   outright rather than clamping — "when" is the whole request — so these exist
   to keep the picker from offering a time that would only bounce back. */
export const MIN_SCHEDULE_LEAD_SECONDS = 30;
export const MAX_SCHEDULE_AHEAD_SECONDS = 365 * 24 * 60 * 60;
export const MAX_SCHEDULED_MESSAGES_PER_USER = 25;
export const MAX_REMINDERS_PER_USER = 50;
export const MAX_REMINDER_TEXT_LENGTH = 500;

export const VOICE_QUALITY_PRESETS: Array<{
  quality: string;
  bitrate: number | null;
  label: string;
}> = [
  { quality: 'low', bitrate: 32_000, label: t('voiceQuality.low') },
  { quality: 'standard', bitrate: 64_000, label: t('voiceQuality.standard') },
  { quality: 'high', bitrate: 96_000, label: t('voiceQuality.high') },
  { quality: 'ultra', bitrate: 128_000, label: t('voiceQuality.ultra') },
  { quality: 'lossless', bitrate: null, label: t('voiceQuality.lossless') }
];

export const DEFAULT_VOICE_PRESET = VOICE_QUALITY_PRESETS[1];

/** Upper bound the server accepts for a voice bitrate (`MAX_ALLOWED_VOICE_BITRATE`). */
export const MAX_VOICE_BITRATE = 320_000;

/** Largest per-channel user limit the server accepts (`MAX_VOICE_USER_LIMIT`). */
export const MAX_VOICE_USER_LIMIT = 99;

export const NOTIFICATION_OPTIONS: Array<{
  value: ChannelNotificationPreference;
  label: string;
  description: string;
  icon: string;
}> = [
  {
    value: 'all',
    label: t('notificationPreference.all'),
    description: t('notificationPreference.all.description'),
    icon: '🔔'
  },
  {
    value: 'mentions',
    label: t('notificationPreference.mentions'),
    description: t('notificationPreference.mentions.description'),
    icon: '@'
  },
  {
    value: 'mute',
    label: t('notificationPreference.mute'),
    description: t('notificationPreference.mute.description'),
    icon: '🔕'
  }
];

export const HELP_COMMANDS: Array<{
  usage: string;
  description: string;
  aliases?: string[];
}> = [
  { usage: '/help', description: t('slashCommand.help') },
  { usage: '/me <action>', description: t('slashCommand.me') },
  {
    usage: '/tts <message>',
    description: t('slashCommand.tts')
  },
  {
    usage: '/shrug [message]',
    description: t('slashCommand.shrug')
  },
  {
    usage: '/topic <text>',
    description: t('slashCommand.topic')
  },
  {
    usage: '/status <online|away|busy|offline>',
    description: t('slashCommand.status')
  },
  {
    usage: '/ephemeral <seconds> <message>',
    description: t('slashCommand.ephemeral'),
    aliases: ['/temp <seconds> <message>']
  },
  {
    usage: '/poll <question> | <option> | <option> …',
    description: t('slashCommand.poll', { max: MAX_POLL_OPTIONS })
  },
  {
    usage: '/search [query]',
    description: t('slashCommand.search')
  },
  {
    usage: '/remind <when> <note>',
    description: t('slashCommand.remind'),
    aliases: ['/remindme <when> <note>']
  },
  {
    usage: '/schedule <when> <message>',
    description: t('slashCommand.schedule')
  },
  {
    usage: '/reminders',
    description: t('slashCommand.reminders')
  }
];

