import type { Message, VoiceChannelInfo } from '../types';
import { extractLinks } from '../link-preview';
import { VOICE_QUALITY_PRESETS, DEFAULT_VOICE_PRESET } from './constants';

export type MessageBlock =
  | { kind: 'separator'; label: string; key: string }
  | { kind: 'message'; message: Message; key: string; links: string[] };

export function pingToStrength(ms: number): number {
  return ms === 0 ? 5 : ms < 50 ? 5 : ms < 100 ? 4 : ms < 200 ? 3 : ms < 400 ? 2 : 1;
}

export function parseTimestampValue(timestamp: string | undefined): Date | null {
  if (!timestamp) return null;
  const parsed = Date.parse(timestamp);
  if (Number.isNaN(parsed)) return null;
  return new Date(parsed);
}

function dateKey(date: Date): string {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
}

function formatDayHeading(date: Date): string {
  const today = new Date();
  const key = dateKey(date);
  const todayKey = dateKey(today);
  if (key === todayKey) return 'Today';
  const yesterday = new Date(today);
  yesterday.setDate(today.getDate() - 1);
  if (key === dateKey(yesterday)) return 'Yesterday';
  return date.toLocaleDateString(undefined, { year: 'numeric', month: 'long', day: 'numeric' });
}

export function buildMessageBlocks(messages: Message[]): MessageBlock[] {
  const blocks: MessageBlock[] = [];
  let lastDateKey: string | null = null;

  for (let index = 0; index < messages.length; index += 1) {
    const message = messages[index];
    const timestamp = parseTimestampValue(message.timestamp);
    if (timestamp) {
      const currentKey = dateKey(timestamp);
      if (lastDateKey !== currentKey) {
        blocks.push({
          kind: 'separator',
          label: formatDayHeading(timestamp),
          key: `separator-${currentKey}-${message.id ?? index}`
        });
        lastDateKey = currentKey;
      }
    }

    const links = extractLinks(message.text);
    blocks.push({
      kind: 'message',
      message,
      key: `message-${message.id ?? `${index}-${message.time ?? ''}`}`,
      links
    });
  }

  return blocks;
}

export function describeDuration(seconds: number): string {
  if (seconds < 60) {
    return `${seconds} ${seconds === 1 ? 'second' : 'seconds'}`;
  }
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  if (minutes < 60) {
    if (remainingSeconds === 0) {
      return `${minutes} ${minutes === 1 ? 'minute' : 'minutes'}`;
    }
    return `${minutes} ${minutes === 1 ? 'minute' : 'minutes'} ${remainingSeconds} ${remainingSeconds === 1 ? 'second' : 'seconds'}`;
  }
  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;
  if (remainingMinutes === 0) {
    return `${hours} ${hours === 1 ? 'hour' : 'hours'}`;
  }
  return `${hours} ${hours === 1 ? 'hour' : 'hours'} ${remainingMinutes} ${remainingMinutes === 1 ? 'minute' : 'minutes'}`;
}

export function formatExpiry(expiresAt: string | undefined, now: number): string | null {
  if (!expiresAt) return null;
  const parsed = Date.parse(expiresAt);
  if (Number.isNaN(parsed)) return null;
  const diff = parsed - now;
  if (diff <= 0) return 'Expired';
  const totalSeconds = Math.round(diff / 1000);
  if (totalSeconds < 60) return `Expires in ${totalSeconds}s`;
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  if (minutes < 60) {
    return seconds === 0 ? `Expires in ${minutes}m` : `Expires in ${minutes}m ${seconds}s`;
  }
  const hours = Math.floor(minutes / 60);
  const remainingMinutes = minutes % 60;
  if (hours < 24) {
    return remainingMinutes === 0
      ? `Expires in ${hours}h`
      : `Expires in ${hours}h ${remainingMinutes}m`;
  }
  const days = Math.floor(hours / 24);
  const remainingHours = hours % 24;
  return remainingHours === 0
    ? `Expires in ${days}d`
    : `Expires in ${days}d ${remainingHours}h`;
}

export function formatExpiryAbsolute(expiresAt: string | undefined): string | null {
  if (!expiresAt) return null;
  const parsed = parseTimestampValue(expiresAt);
  return parsed ? parsed.toLocaleString() : null;
}

export function ephemeralInfo(
  message: Message,
  now: number
): { label: string; absolute?: string } | null {
  const expiresAt = typeof message.expiresAt === 'string' ? message.expiresAt : undefined;
  const label = formatExpiry(expiresAt, now);
  if (!label) return null;
  const absolute = formatExpiryAbsolute(expiresAt) ?? undefined;
  return { label, absolute };
}

export function searchResultPreview(message: Message): string {
  if (typeof message.text === 'string' && message.text.trim().length > 0) {
    const normalized = message.text.trim().replace(/\s+/g, ' ');
    return normalized.length > 120 ? `${normalized.slice(0, 117)}…` : normalized;
  }
  if (typeof message.image === 'string' && message.image.trim().length > 0) {
    return '[Image]';
  }
  return 'Message';
}

export function formatSearchTimestamp(message: Message): string {
  const timestamp = typeof message.timestamp === 'string' ? message.timestamp : undefined;
  const parsed = parseTimestampValue(timestamp);
  if (parsed) return parsed.toLocaleString();
  if (typeof message.time === 'string') return message.time;
  return '';
}

export function formatVoiceQuality(info: VoiceChannelInfo): string {
  const preset = VOICE_QUALITY_PRESETS.find((p) => p.quality === info.quality);
  const bitrate = info.bitrate ?? preset?.bitrate ?? null;
  const label = preset ? preset.label : info.quality;
  return bitrate && bitrate > 0 ? `${label} (${Math.round(bitrate / 1000)} kbps)` : label;
}

export function promptVoicePreset(): { quality: string; bitrate: number | null } {
  const input = prompt(
    'Voice quality (low, standard, high, ultra, lossless)',
    DEFAULT_VOICE_PRESET.quality
  );
  if (!input) return { quality: DEFAULT_VOICE_PRESET.quality, bitrate: DEFAULT_VOICE_PRESET.bitrate };
  const normalized = input.trim().toLowerCase();
  const preset = VOICE_QUALITY_PRESETS.find((p) => p.quality === normalized) ?? DEFAULT_VOICE_PRESET;
  return { quality: preset.quality, bitrate: preset.bitrate };
}

export function reactionEntries(
  msg: Message | undefined
): Array<{ emoji: string; users: string[] }> {
  if (!msg) return [];
  return Object.entries(msg.reactions ?? {})
    .map(([emoji, users]) => ({ emoji, users }))
    .filter((entry) => entry.users.length > 0);
}

export function notificationButtonIcon(value: string): string {
  switch (value) {
    case 'mentions':
      return '@';
    case 'mute':
      return '🔕';
    default:
      return '🔔';
  }
}
