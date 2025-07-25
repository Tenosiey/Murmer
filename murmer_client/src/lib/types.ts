export interface Message {
  type: string;
  user?: string;
  text?: string;
  time?: string;
  channel?: string;
  id?: number;
  messages?: Message[];
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
