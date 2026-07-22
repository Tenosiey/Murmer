export interface AttachmentInfo {
  url: string;
  name: string;
  size: number;
}

export interface ReplyInfo {
  id: number;
  user: string;
  text: string;
}

export interface Message {
  type: string;
  user?: string;
  text?: string;
  attachment?: AttachmentInfo;
  time?: string;
  timestamp?: string;
  channelId?: number;
  id?: number;
  messages?: Message[];
  reactions?: Record<string, string[]>;
  ephemeral?: boolean;
  expiresAt?: string;
  edited?: boolean;
  editedAt?: string;
  replyTo?: ReplyInfo;
  threadId?: number;
  /** Direct messages: sender/recipient names (metadata stays plaintext). */
  from?: string;
  to?: string;
  /** Direct messages: end-to-end encryption fields (base64). */
  nonce?: string;
  ciphertext?: string;
  /** Set when a DM ciphertext failed to decrypt; render a placeholder. */
  decryptFailed?: boolean;
  [key: string]: unknown;
}

export interface RemotePeer {
  id: string;
  stream: MediaStream;
  stats?: ConnectionStats;
}

export interface ConnectionStats {
  rtt: number;
  jitter: number;
  /** Packet loss over the last stats window, percent 0–100. */
  packetLoss: number;
  strength: number;
}

/** A user's self-reported connection stats as relayed by the server. */
export interface UserConnectionStats {
  ping: number | null;
  voiceRtt: number | null;
  voiceJitter: number | null;
  voiceLoss: number | null;
  /** Seconds since the user last reported. */
  ageSeconds: number;
}

/** A user's display role (highest-position assigned role) shown as a badge. */
export interface RoleInfo {
  role: string;
  color?: string;
}

/** A server role definition as broadcast in the `role-definitions` frame. */
export interface RoleDef {
  id: number;
  name: string;
  color?: string;
  /** Permission bitmask (see `src/lib/chat/permissions.ts`). */
  permissions: number;
  /** Hierarchy position; higher outranks lower. */
  position: number;
  /** The implicit `@everyone` baseline role (never assigned explicitly). */
  isDefault: boolean;
  /** The protected Owner role (locked to Administrator). */
  isOwner: boolean;
}

export type UserStatus = 'online' | 'away' | 'busy' | 'offline';

export interface VoiceChannelInfo {
  id: number;
  name: string;
  quality: string;
  bitrate: number | null;
  categoryId: number | null;
  position: number;
  /** True when the channel restricts View for @everyone (shows a lock). */
  private?: boolean;
}

/** One per-channel permission override target, as sent to managers. */
export interface ChannelOverride {
  targetType: 'everyone' | 'role' | 'user';
  /** Role id (as string) or user public key; empty for @everyone. */
  targetId: string;
  /** Username (user overrides) or role name (role overrides), for display. */
  targetLabel: string;
  allow: number;
  deny: number;
}

export interface CategoryInfo {
  id: number;
  name: string;
  position: number;
}

export interface ChannelInfo {
  id: number;
  name: string;
  categoryId: number | null;
  position: number;
  /** True when the channel restricts View for @everyone (shows a lock). */
  private?: boolean;
}

export interface ScreenShareSettings {
  width: number;
  height: number;
  frameRate: number;
  /** Encoder bitrate cap in bits per second. */
  maxBitrate: number;
}

export interface ScreenSharePeer {
  userId: string;
  stream: MediaStream;
}

export interface ScreenShareActive {
  userId: string;
  channelId: number;
}

/** Entry of a right-click menu. Items with `children` open a submenu instead
 *  of running an action. */
export interface ContextMenuItem {
  label: string;
  action?: () => void;
  danger?: boolean;
  icon?: string;
  children?: ContextMenuItem[];
}
