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
  /**
   * True while the connection to this peer is being repaired (see
   * `voice/recovery.ts`). They are still in the channel and their audio is
   * expected back — the UI says so rather than leaving the silence unexplained.
   */
  reconnecting?: boolean;
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

/**
 * A user's profile as stored on their name/key binding on the server. The
 * account `user` name stays the identity everywhere (auth, roles, DMs,
 * message authorship); `displayName` is only what the UI shows instead.
 */
export interface UserProfile {
  user: string;
  /** Chosen display name, or an empty string when the account name is used. */
  displayName: string;
  /** Free-text "about me", empty when unset. */
  about: string;
  /** RFC 3339 timestamp of when the name was first claimed ("member since"). */
  createdAt: string;
}

/** A user's display role (highest-position assigned role) shown as a badge. */
export interface RoleInfo {
  role: string;
  color?: string;
  /** Icon of the highest assigned role that has one, as a `/files/<key>` URL. */
  icon?: string;
  /** Name of the role the icon belongs to; used as the icon's alt text. */
  iconRole?: string;
}

/** A server role definition as broadcast in the `role-definitions` frame. */
export interface RoleDef {
  id: number;
  name: string;
  color?: string;
  /** Uploaded image (or custom emoji) shown next to members holding the role. */
  icon?: string;
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

/** One sound in the server's shared soundboard library. */
export interface SoundboardSound {
  id: number;
  name: string;
  /** Relative upload URL (`/files/...`), resolved against the server at play time. */
  url: string;
  uploadedBy?: string;
  /** Unattributed aggregate play count kept by the server. */
  playCount: number;
  createdAt?: string;
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
  /**
   * Whether the share carries audio. Sharing system audio is a checkbox in the
   * OS picker, so most shares are silent — the viewer only offers volume and
   * mute controls when there is something to control.
   */
  hasAudio: boolean;
  /**
   * The connection carrying this share dropped and is being repaired (see
   * `webrtc/recovery.ts`). The last frame stays on screen while it is — the
   * share has not ended, so its window must not close.
   */
  reconnecting?: boolean;
}

/** One tile on the screen share stage. */
export interface WatchedScreenShare {
  /** Stable list key; the sharer's own preview and a peer never collide. */
  key: string;
  /** Account name of the sharer. */
  userId: string;
  /** The incoming stream, or null while the connection is still coming up. */
  peer: ScreenSharePeer | null;
  /** Our own capture, shown as a preview instead of received from a peer. */
  isSelf: boolean;
  onClose: () => void;
}

/** Playback settings of a single watched screen share. */
export interface ScreenShareAudio {
  /** Media element volume, a 0-1 fraction. */
  volume: number;
  muted: boolean;
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
