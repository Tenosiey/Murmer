import type { Message, UserStatus, VoiceChannelInfo } from '../types';
import { extractLinks } from '../link-preview';
import { VOICE_QUALITY_PRESETS } from './constants';

export type MessageBlock =
  | { kind: 'separator'; label: string; key: string }
  | { kind: 'unread'; key: string }
  | {
      kind: 'message';
      message: Message;
      key: string;
      links: string[];
      /** True when this message continues the previous author's group and
       *  renders without its own avatar/username header. */
      continuation: boolean;
    };

/** Messages from the same author within this window collapse into one group. */
const GROUP_WINDOW_MS = 5 * 60 * 1000;

export interface MessageBlockOptions {
  /** Insert a "new messages" marker before the first foreign message with an id above this. */
  unreadAfterId?: number;
  /** The viewer's username; their own messages never count as unread. */
  currentUser?: string | null;
}

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

/* buildMessageBlocks runs on every chat-store update, so link extraction is
   cached per message object to avoid re-running the URL regex over the whole
   history each time. */
const linkCache = new WeakMap<Message, string[]>();

function linksFor(message: Message): string[] {
  let links = linkCache.get(message);
  if (!links) {
    links = extractLinks(message.text);
    linkCache.set(message, links);
  }
  return links;
}

export function buildMessageBlocks(
  messages: Message[],
  options: MessageBlockOptions = {}
): MessageBlock[] {
  const blocks: MessageBlock[] = [];
  let lastDateKey: string | null = null;
  let groupUser: string | null = null;
  let groupTime: number | null = null;

  const { unreadAfterId, currentUser } = options;
  let unreadMarkerPlaced = unreadAfterId === undefined || unreadAfterId <= 0;

  for (let index = 0; index < messages.length; index += 1) {
    const message = messages[index];

    if (
      !unreadMarkerPlaced &&
      typeof message.id === 'number' &&
      message.id > unreadAfterId! &&
      message.user !== currentUser
    ) {
      blocks.push({ kind: 'unread', key: 'unread-marker' });
      unreadMarkerPlaced = true;
      groupUser = null;
    }
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
        groupUser = null;
      }
    }

    /* A message continues the current group when it has the same author,
       arrives within the grouping window and is not a reply (replies show
       their quote and deserve a fresh header). */
    const time = timestamp?.getTime() ?? null;
    const continuation =
      groupUser !== null &&
      message.user === groupUser &&
      !message.replyTo &&
      groupTime !== null &&
      time !== null &&
      time - groupTime <= GROUP_WINDOW_MS;

    if (!continuation) {
      groupUser = message.user ?? null;
      groupTime = time;
    }

    const links = linksFor(message);
    blocks.push({
      kind: 'message',
      message,
      key: `message-${message.id ?? `${index}-${message.time ?? ''}`}`,
      links,
      continuation
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
  if (message.attachment) {
    return `[File] ${message.attachment.name}`;
  }
  return 'Message';
}

export function formatFileSize(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes <= 0) return '';
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb.toFixed(kb < 10 ? 1 : 0)} KB`;
  const mb = kb / 1024;
  return `${mb.toFixed(mb < 10 ? 1 : 0)} MB`;
}

/** Short "HH:MM" label shown next to messages (no seconds). */
export function formatShortTime(message: Message): string {
  const parsed = parseTimestampValue(message.timestamp);
  if (parsed) {
    return parsed.toLocaleTimeString(undefined, { hour: '2-digit', minute: '2-digit' });
  }
  // Fall back to trimming the seconds off the pre-formatted time string.
  if (typeof message.time === 'string') {
    return message.time.replace(/(\d{1,2}:\d{2}):\d{2}/, '$1');
  }
  return '';
}

/** Full date + time (with seconds) revealed on hover via the title tooltip. */
export function formatFullTimestamp(message: Message): string {
  const parsed = parseTimestampValue(message.timestamp);
  if (parsed) return parsed.toLocaleString();
  return typeof message.time === 'string' ? message.time : '';
}

export function formatSearchTimestamp(message: Message): string {
  const timestamp = typeof message.timestamp === 'string' ? message.timestamp : undefined;
  const parsed = parseTimestampValue(timestamp);
  if (parsed) return parsed.toLocaleString();
  if (typeof message.time === 'string') return message.time;
  return '';
}

/** A voice channel in sidebar order, with its breakout rooms folded in. */
export interface VoiceChannelRow {
  channel: VoiceChannelInfo;
  /** True for a breakout room, which renders indented under its parent. */
  room: boolean;
}

/** Custom sort order: position first, name and id as tie-breakers. */
function byPosition(a: VoiceChannelInfo, b: VoiceChannelInfo): number {
  return a.position - b.position || a.name.localeCompare(b.name) || a.id - b.id;
}

/**
 * Order voice channels for the sidebar, placing each breakout room directly
 * under the channel it was split off from.
 *
 * Rooms are created after their parent and so sort to the *end* of the
 * category on position alone, which is exactly where they must not be: a room
 * only makes sense next to the call it came from. A room whose parent is not
 * in the list — the parent is in another category, or the viewer cannot see
 * it — is kept at the end rather than dropped, so a channel somebody is in
 * can never vanish from their sidebar.
 */
export function orderVoiceChannels(channels: VoiceChannelInfo[]): VoiceChannelRow[] {
  const sorted = [...channels].sort(byPosition);
  const ids = new Set(sorted.map((c) => c.id));
  const roomsByParent = new Map<number, VoiceChannelInfo[]>();
  for (const channel of sorted) {
    const parent = channel.breakoutParent;
    if (parent == null || !ids.has(parent)) continue;
    const list = roomsByParent.get(parent);
    if (list) list.push(channel);
    else roomsByParent.set(parent, [channel]);
  }

  const rows: VoiceChannelRow[] = [];
  for (const channel of sorted) {
    const parent = channel.breakoutParent;
    if (parent != null && ids.has(parent)) continue;
    rows.push({ channel, room: parent != null });
    for (const room of roomsByParent.get(channel.id) ?? []) {
      rows.push({ channel: room, room: true });
    }
  }
  return rows;
}

export function formatVoiceQuality(info: VoiceChannelInfo): string {
  const preset = VOICE_QUALITY_PRESETS.find((p) => p.quality === info.quality);
  const bitrate = info.bitrate ?? preset?.bitrate ?? null;
  const label = preset ? preset.label : info.quality;
  return bitrate && bitrate > 0 ? `${label} (${Math.round(bitrate / 1000)} kbps)` : label;
}

export function reactionEntries(
  msg: Message | undefined
): Array<{ emoji: string; users: string[] }> {
  if (!msg) return [];
  return Object.entries(msg.reactions ?? {})
    .map(([emoji, users]) => ({ emoji, users }))
    .filter((entry) => entry.users.length > 0);
}

export function ensureStatus(
  map: Record<string, UserStatus>,
  user: string,
  fallback: UserStatus = 'offline'
): UserStatus {
  return (map[user] ?? fallback) as UserStatus;
}

