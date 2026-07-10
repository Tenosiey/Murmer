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
  strength: number;
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
}

export interface ScreenShareSettings {
  width: number;
  height: number;
  frameRate: number;
}

export interface ScreenSharePeer {
  userId: string;
  stream: MediaStream;
}

export interface ScreenShareActive {
  userId: string;
  channelId: number;
}
