/**
 * WebRTC screen sharing manager.
 *
 * Handles peer-to-peer screen sharing connections using WebRTC. Allows users
 * to share their screen with configurable quality and frame rate settings.
 * Uses the same signaling infrastructure as voice chat.
 */
import { chat } from '../stores/chat';
import type { Message, ScreenShareSettings, ScreenSharePeer } from '../types';

/**
 * Default screen share settings - 720p at 30fps
 */
const DEFAULT_SETTINGS: ScreenShareSettings = {
  width: 1280,
  height: 720,
  frameRate: 30
};

/**
 * Quality presets for easy selection
 */
export const QUALITY_PRESETS = {
  '480p': { width: 854, height: 480, frameRate: 15 },
  '720p': { width: 1280, height: 720, frameRate: 30 },
  '1080p': { width: 1920, height: 1080, frameRate: 30 },
  '1080p60': { width: 1920, height: 1080, frameRate: 60 },
  '1440p': { width: 2560, height: 1440, frameRate: 30 },
  '4k': { width: 3840, height: 2160, frameRate: 30 }
} as const;

export type QualityPreset = keyof typeof QUALITY_PRESETS;

/**
 * Manages screen sharing WebRTC connections and signaling.
 */
export class ScreenShareManager {
  private peers: Record<string, RTCPeerConnection> = {};
  private localStream: MediaStream | null = null;
  private userName: string | null = null;
  private channel: string | null = null;
  private listeners: Array<(peers: ScreenSharePeer[]) => void> = [];
  private settings: ScreenShareSettings = DEFAULT_SETTINGS;
  
  private config: RTCConfiguration = {
    iceServers: [{ urls: 'stun:stun.l.google.com:19302' }]
  };

  constructor() {
    // Listen for signaling messages
    this.setupSignaling();
  }

  private setupSignaling() {
    chat.on('screenshare-offer', (msg) => this.handleOffer(msg));
    chat.on('screenshare-answer', (msg) => this.handleAnswer(msg));
    chat.on('screenshare-candidate', (msg) => this.handleCandidate(msg));
    chat.on('screenshare-stop', (msg) => this.handleRemoteStop(msg));
  }

  /**
   * Subscribe to screen share peer updates
   */
  subscribe(cb: (peers: ScreenSharePeer[]) => void) {
    this.listeners.push(cb);
    return () => {
      this.listeners = this.listeners.filter((fn) => fn !== cb);
    };
  }

  private emit(peers: ScreenSharePeer[]) {
    for (const cb of this.listeners) cb(peers);
  }

  /**
   * Get current peers list from all connections
   */
  private getPeersList(): ScreenSharePeer[] {
    const peers: ScreenSharePeer[] = [];
    for (const [userId, pc] of Object.entries(this.peers)) {
      const receivers = pc.getReceivers();
      for (const receiver of receivers) {
        if (receiver.track && receiver.track.kind === 'video') {
          const stream = new MediaStream([receiver.track]);
          peers.push({ userId, stream });
          break; // Only need one video track per peer
        }
      }
    }
    return peers;
  }

  /**
   * Start sharing screen with specified settings
   */
  async startSharing(
    user: string,
    channel: string,
    settings: Partial<ScreenShareSettings> = {}
  ): Promise<void> {
    if (this.localStream) {
      throw new Error('Already sharing screen');
    }

    this.userName = user;
    this.channel = channel;
    this.settings = { ...DEFAULT_SETTINGS, ...settings };

    try {
      // Request screen sharing with specified constraints
      const stream = await navigator.mediaDevices.getDisplayMedia({
        video: {
          width: { ideal: this.settings.width },
          height: { ideal: this.settings.height },
          frameRate: { ideal: this.settings.frameRate }
        },
        audio: false // Can be enabled if system audio sharing is desired
      });

      this.localStream = stream;

      // Notify server and peers
      chat.sendRaw({
        type: 'screenshare-start',
        user,
        channel,
        settings: this.settings
      });

      // Listen for when user stops sharing via browser UI
      stream.getVideoTracks()[0].addEventListener('ended', () => {
        this.stopSharing();
      });
    } catch (error) {
      this.userName = null;
      this.channel = null;
      throw error;
    }
  }

  /**
   * Stop sharing screen
   */
  stopSharing(): void {
    if (!this.localStream || !this.userName || !this.channel) return;

    // Stop all tracks
    this.localStream.getTracks().forEach(track => track.stop());
    this.localStream = null;

    // Notify peers
    chat.sendRaw({
      type: 'screenshare-stop',
      user: this.userName,
      channel: this.channel
    });

    // Clean up all peer connections
    for (const userId of Object.keys(this.peers)) {
      this.cleanupPeer(userId);
    }

    this.userName = null;
    this.channel = null;
    this.emit([]);
  }

  /**
   * Update screen share quality settings (requires restart)
   */
  updateSettings(settings: Partial<ScreenShareSettings>): void {
    this.settings = { ...this.settings, ...settings };
  }

  /**
   * Create or get existing peer connection
   */
  private async createPeer(userId: string, initiator: boolean): Promise<RTCPeerConnection> {
    if (this.peers[userId]) return this.peers[userId];

    const pc = new RTCPeerConnection(this.config);
    this.peers[userId] = pc;

    // Add local screen share tracks if available
    if (this.localStream && initiator) {
      for (const track of this.localStream.getTracks()) {
        pc.addTrack(track, this.localStream);
      }
    }

    // Handle incoming tracks
    pc.ontrack = (ev) => {
      console.log('Received screen share track from', userId);
      this.emit(this.getPeersList());
    };

    // Handle ICE candidates
    pc.onicecandidate = (ev) => {
      if (ev.candidate && this.userName) {
        chat.sendRaw({
          type: 'screenshare-candidate',
          user: this.userName,
          target: userId,
          channel: this.channel,
          candidate: ev.candidate
        });
      }
    };

    // Handle connection state changes
    pc.onconnectionstatechange = () => {
      if (pc.connectionState === 'disconnected' || pc.connectionState === 'failed') {
        this.cleanupPeer(userId);
      }
    };

    // If initiator, create and send offer
    if (initiator && this.userName) {
      const offer = await pc.createOffer();
      await pc.setLocalDescription(offer);
      chat.sendRaw({
        type: 'screenshare-offer',
        user: this.userName,
        target: userId,
        channel: this.channel,
        sdp: offer
      });
    }

    return pc;
  }

  /**
   * Handle incoming offer
   */
  private async handleOffer(msg: Message): Promise<void> {
    if (!this.userName || msg.target !== this.userName || msg.channel !== this.channel) return;

    const userId = msg.user as string;
    const pc = await this.createPeer(userId, false);

    await pc.setRemoteDescription(new RTCSessionDescription(msg.sdp as any));
    const answer = await pc.createAnswer();
    await pc.setLocalDescription(answer);

    chat.sendRaw({
      type: 'screenshare-answer',
      user: this.userName,
      target: userId,
      channel: this.channel,
      sdp: answer
    });
  }

  /**
   * Handle incoming answer
   */
  private async handleAnswer(msg: Message): Promise<void> {
    if (!this.userName || msg.target !== this.userName || msg.channel !== this.channel) return;

    const pc = this.peers[msg.user as string];
    if (pc && !pc.currentRemoteDescription) {
      await pc.setRemoteDescription(new RTCSessionDescription(msg.sdp as any));
    }
  }

  /**
   * Handle incoming ICE candidate
   */
  private async handleCandidate(msg: Message): Promise<void> {
    if (!this.userName || msg.target !== this.userName || msg.channel !== this.channel) return;

    const pc = this.peers[msg.user as string];
    if (pc) {
      try {
        await pc.addIceCandidate(msg.candidate as any);
      } catch (error) {
        console.error('Error adding ICE candidate:', error);
      }
    }
  }

  /**
   * Handle remote user stopping their screen share
   */
  private handleRemoteStop(msg: Message): void {
    const userId = msg.user as string;
    if (msg.channel === this.channel) {
      this.cleanupPeer(userId);
    }
  }

  /**
   * Clean up a peer connection
   */
  private cleanupPeer(userId: string): void {
    const pc = this.peers[userId];
    if (pc) {
      pc.close();
      delete this.peers[userId];
      this.emit(this.getPeersList());
    }
  }

  /**
   * Request to view a specific user's screen share
   */
  async viewScreenShare(userId: string, viewerName?: string, channel?: string): Promise<void> {
    // Set viewer info if provided and not already set
    if (viewerName && channel && !this.userName && !this.channel) {
      this.userName = viewerName;
      this.channel = channel;
    }

    if (!this.userName || !this.channel) {
      throw new Error('Not in a voice channel');
    }

    // Create peer connection as initiator
    await this.createPeer(userId, true);
  }

  /**
   * Stop viewing a specific user's screen share
   */
  stopViewing(userId: string): void {
    this.cleanupPeer(userId);
  }

  /**
   * Leave the screen share session as a viewer (cleanup without stopping sharing)
   */
  leaveAsViewer(): void {
    // Only clean up if not actively sharing
    if (!this.isSharing()) {
      for (const userId of Object.keys(this.peers)) {
        this.cleanupPeer(userId);
      }
      this.userName = null;
      this.channel = null;
      this.emit([]);
    }
  }

  /**
   * Check if currently sharing
   */
  isSharing(): boolean {
    return this.localStream !== null;
  }

  /**
   * Get current settings
   */
  getSettings(): ScreenShareSettings {
    return { ...this.settings };
  }

  /**
   * Clean up all resources
   */
  destroy(): void {
    this.stopSharing();
    chat.off('screenshare-offer');
    chat.off('screenshare-answer');
    chat.off('screenshare-candidate');
    chat.off('screenshare-stop');
  }
}

