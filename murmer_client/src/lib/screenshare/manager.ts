/**
 * WebRTC screen sharing manager.
 *
 * Handles peer-to-peer screen sharing connections using WebRTC. Allows users
 * to share their screen with configurable quality and frame rate settings.
 * Uses the same signaling infrastructure as voice chat.
 */
import { chat } from '../stores/chat';
import type { Message, ScreenShareSettings, ScreenSharePeer } from '../types';

const DEFAULT_SETTINGS: ScreenShareSettings = {
  width: 1280,
  height: 720,
  frameRate: 30
};

export const QUALITY_PRESETS = {
  '480p': { width: 854, height: 480, frameRate: 15 },
  '720p': { width: 1280, height: 720, frameRate: 30 },
  '1080p': { width: 1920, height: 1080, frameRate: 30 },
  '1080p60': { width: 1920, height: 1080, frameRate: 60 },
  '1440p': { width: 2560, height: 1440, frameRate: 30 },
  '4k': { width: 3840, height: 2160, frameRate: 30 }
} as const;

export type QualityPreset = keyof typeof QUALITY_PRESETS;

export class ScreenShareManager {
  private peers: Record<string, RTCPeerConnection> = {};
  private localStream: MediaStream | null = null;
  private userName: string | null = null;
  private channelId: number | null = null;
  private listeners: Array<(peers: ScreenSharePeer[]) => void> = [];
  private settings: ScreenShareSettings = DEFAULT_SETTINGS;

  private config: RTCConfiguration = {
    iceServers: [{ urls: 'stun:stun.l.google.com:19302' }]
  };

  constructor() {
    this.setupSignaling();
  }

  private setupSignaling() {
    chat.on('screenshare-offer', (msg) => this.handleOffer(msg));
    chat.on('screenshare-answer', (msg) => this.handleAnswer(msg));
    chat.on('screenshare-candidate', (msg) => this.handleCandidate(msg));
    chat.on('screenshare-stop', (msg) => this.handleRemoteStop(msg));
  }

  subscribe(cb: (peers: ScreenSharePeer[]) => void) {
    this.listeners.push(cb);
    return () => {
      this.listeners = this.listeners.filter((fn) => fn !== cb);
    };
  }

  private emit(peers: ScreenSharePeer[]) {
    for (const cb of this.listeners) cb(peers);
  }

  private getPeersList(): ScreenSharePeer[] {
    const peers: ScreenSharePeer[] = [];
    for (const [userId, pc] of Object.entries(this.peers)) {
      const receivers = pc.getReceivers();
      for (const receiver of receivers) {
        if (receiver.track && receiver.track.kind === 'video') {
          const stream = new MediaStream([receiver.track]);
          peers.push({ userId, stream });
          break;
        }
      }
    }
    return peers;
  }

  async startSharing(
    user: string,
    channelId: number,
    settings: Partial<ScreenShareSettings> = {}
  ): Promise<void> {
    if (this.localStream) {
      throw new Error('Already sharing screen');
    }

    this.userName = user;
    this.channelId = channelId;
    this.settings = { ...DEFAULT_SETTINGS, ...settings };

    try {
      const stream = await navigator.mediaDevices.getDisplayMedia({
        video: {
          width: { ideal: this.settings.width },
          height: { ideal: this.settings.height },
          frameRate: { ideal: this.settings.frameRate }
        },
        audio: false
      });

      this.localStream = stream;

      chat.sendRaw({
        type: 'screenshare-start',
        user,
        channelId,
        settings: this.settings
      });

      stream.getVideoTracks()[0].addEventListener('ended', () => {
        this.stopSharing();
      });
    } catch (error) {
      this.userName = null;
      this.channelId = null;
      throw error;
    }
  }

  stopSharing(): void {
    if (!this.localStream || !this.userName || this.channelId === null) return;

    this.localStream.getTracks().forEach((track) => track.stop());
    this.localStream = null;

    chat.sendRaw({
      type: 'screenshare-stop',
      user: this.userName,
      channelId: this.channelId
    });

    for (const userId of Object.keys(this.peers)) {
      this.cleanupPeer(userId);
    }

    this.userName = null;
    this.channelId = null;
    this.emit([]);
  }

  updateSettings(settings: Partial<ScreenShareSettings>): void {
    this.settings = { ...this.settings, ...settings };
  }

  private async createPeer(userId: string, initiator: boolean): Promise<RTCPeerConnection> {
    if (this.peers[userId]) return this.peers[userId];

    const pc = new RTCPeerConnection(this.config);
    this.peers[userId] = pc;

    if (this.localStream) {
      for (const track of this.localStream.getTracks()) {
        pc.addTrack(track, this.localStream);
      }
    } else if (initiator) {
      pc.addTransceiver('video', { direction: 'recvonly' });
    }

    pc.ontrack = () => {
      this.emit(this.getPeersList());
    };

    pc.onicecandidate = (ev) => {
      if (ev.candidate && this.userName) {
        chat.sendRaw({
          type: 'screenshare-candidate',
          user: this.userName,
          target: userId,
          channelId: this.channelId,
          candidate: ev.candidate
        });
      }
    };

    pc.onconnectionstatechange = () => {
      if (pc.connectionState === 'disconnected' || pc.connectionState === 'failed') {
        this.cleanupPeer(userId);
      }
    };

    if (initiator && this.userName) {
      const offer = await pc.createOffer();
      await pc.setLocalDescription(offer);
      chat.sendRaw({
        type: 'screenshare-offer',
        user: this.userName,
        target: userId,
        channelId: this.channelId,
        sdp: offer
      });
    }

    return pc;
  }

  private async handleOffer(msg: Message): Promise<void> {
    if (
      !this.userName ||
      msg.target !== this.userName ||
      (msg as any).channelId !== this.channelId
    )
      return;

    const userId = msg.user as string;
    const pc = await this.createPeer(userId, false);

    await pc.setRemoteDescription(new RTCSessionDescription(msg.sdp as any));
    const answer = await pc.createAnswer();
    await pc.setLocalDescription(answer);

    chat.sendRaw({
      type: 'screenshare-answer',
      user: this.userName,
      target: userId,
      channelId: this.channelId,
      sdp: answer
    });
  }

  private async handleAnswer(msg: Message): Promise<void> {
    if (
      !this.userName ||
      msg.target !== this.userName ||
      (msg as any).channelId !== this.channelId
    )
      return;

    const pc = this.peers[msg.user as string];
    if (pc && !pc.currentRemoteDescription) {
      await pc.setRemoteDescription(new RTCSessionDescription(msg.sdp as any));
    }
  }

  private async handleCandidate(msg: Message): Promise<void> {
    if (
      !this.userName ||
      msg.target !== this.userName ||
      (msg as any).channelId !== this.channelId
    )
      return;

    const pc = this.peers[msg.user as string];
    if (pc) {
      try {
        await pc.addIceCandidate(msg.candidate as any);
      } catch (error) {
        console.error('Error adding ICE candidate:', error);
      }
    }
  }

  private handleRemoteStop(msg: Message): void {
    const userId = msg.user as string;
    if ((msg as any).channelId === this.channelId) {
      this.cleanupPeer(userId);
    }
  }

  private cleanupPeer(userId: string): void {
    const pc = this.peers[userId];
    if (pc) {
      pc.close();
      delete this.peers[userId];
      this.emit(this.getPeersList());
    }
  }

  async viewScreenShare(userId: string, viewerName?: string, channelId?: number): Promise<void> {
    if (viewerName && channelId !== undefined && !this.userName && this.channelId === null) {
      this.userName = viewerName;
      this.channelId = channelId;
    }

    if (!this.userName || this.channelId === null) {
      throw new Error('Not in a voice channel');
    }

    await this.createPeer(userId, true);
  }

  stopViewing(userId: string): void {
    this.cleanupPeer(userId);
  }

  leaveAsViewer(): void {
    if (!this.isSharing()) {
      for (const userId of Object.keys(this.peers)) {
        this.cleanupPeer(userId);
      }
      this.userName = null;
      this.channelId = null;
      this.emit([]);
    }
  }

  isSharing(): boolean {
    return this.localStream !== null;
  }

  getSettings(): ScreenShareSettings {
    return { ...this.settings };
  }

  destroy(): void {
    this.stopSharing();
    chat.off('screenshare-offer');
    chat.off('screenshare-answer');
    chat.off('screenshare-candidate');
    chat.off('screenshare-stop');
  }
}
