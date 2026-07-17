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

export interface RoleInfo {
  role: string;
  color?: string;
}

export type UserStatus = 'online' | 'away' | 'busy' | 'offline';

export interface VoiceChannelInfo {
  id: number;
  name: string;
  quality: string;
  bitrate: number | null;
  categoryId: number | null;
  position: number;
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
