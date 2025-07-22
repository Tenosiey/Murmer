export interface Message {
  type: string;
  user: string;
  text?: string;
  time?: string;
  channel?: string;
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
