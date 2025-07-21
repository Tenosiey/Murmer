import { chat } from '../stores/chat';
import { volume, inputDeviceId } from '../stores/settings';
import { get } from 'svelte/store';
import type { Message, RemotePeer, ConnectionStats } from '../types';

/**
 * Handles WebRTC peer connections and signaling for voice chat.
 * Updates listeners with the list of active remote peers and their stats.
 */
export class VoiceManager {
  private peers: Record<string, RTCPeerConnection> = {};
  private statsIntervals: Record<string, number> = {};
  private localStream: MediaStream | null = null;
  private userName: string | null = null;
  private listeners: Array<(peers: RemotePeer[]) => void> = [];

  private joinSound = new Audio('/sounds/user_join_voice_sound.mp3');
  private leaveSound = new Audio('/sounds/user_leave_voice_sound.mp3');

  private config: RTCConfiguration = {
    iceServers: [{ urls: 'stun:stun.l.google.com:19302' }]
  };

  constructor() {
    volume.subscribe((v) => {
      this.joinSound.volume = v;
      this.leaveSound.volume = v;
    });
  }

  subscribe(cb: (peers: RemotePeer[]) => void) {
    this.listeners.push(cb);
    return () => {
      this.listeners = this.listeners.filter((fn) => fn !== cb);
    };
  }

  private emit(peers: RemotePeer[]) {
    for (const cb of this.listeners) cb(peers);
  }

  private cleanupPeer(id: string, peersList: RemotePeer[]) {
    const pc = this.peers[id];
    if (pc) {
      pc.close();
      delete this.peers[id];
      const interval = this.statsIntervals[id];
      if (interval) {
        clearInterval(interval);
        delete this.statsIntervals[id];
      }
    }
    this.emit(peersList.filter((r) => r.id !== id));
  }

  private async updateStats(id: string, peersList: RemotePeer[]) {
    const pc = this.peers[id];
    if (!pc) return;
    try {
      const reports = await pc.getStats();
      let rtt = 0;
      let jitter = 0;
      reports.forEach((report) => {
        if (
          report.type === 'candidate-pair' &&
          (report as any).state === 'succeeded' &&
          (report as any).currentRoundTripTime != null
        ) {
          rtt = (report as any).currentRoundTripTime * 1000;
        }
        if (
          report.type === 'remote-inbound-rtp' &&
          (report as any).kind === 'audio' &&
          (report as any).jitter != null
        ) {
          jitter = (report as any).jitter * 1000;
        }
      });
      const strength =
        rtt === 0 ? 5 : rtt < 50 ? 5 : rtt < 100 ? 4 : rtt < 200 ? 3 : rtt < 400 ? 2 : 1;
      for (const p of peersList) {
        if (p.id === id) {
          p.stats = { rtt, jitter, strength };
        }
      }
      this.emit([...peersList]);
    } catch {
      // ignore stats errors
    }
  }

  private async createPeer(id: string, initiator: boolean, peersList: RemotePeer[]): Promise<RTCPeerConnection> {
    if (this.peers[id]) return this.peers[id];
    const pc = new RTCPeerConnection(this.config);
    this.peers[id] = pc;
    if (this.localStream) {
      for (const track of this.localStream.getTracks()) {
        pc.addTrack(track, this.localStream);
      }
    }
    pc.ontrack = (ev) => {
      const stream = ev.streams[0];
      const existing = peersList.find((r) => r.id === id);
      if (existing) {
        existing.stream = stream;
      } else {
        peersList.push({ id, stream });
      }
      this.emit([...peersList]);
    };
    pc.onicecandidate = (ev) => {
      if (ev.candidate && this.userName) {
        chat.sendRaw({
          type: 'voice-candidate',
          user: this.userName,
          target: id,
          candidate: ev.candidate
        });
      }
    };
    pc.onconnectionstatechange = () => {
      if (pc.connectionState === 'disconnected' || pc.connectionState === 'closed') {
        this.cleanupPeer(id, peersList);
      }
    };
    this.statsIntervals[id] = window.setInterval(() => this.updateStats(id, peersList), 1000);
    if (initiator && this.userName) {
      const offer = await pc.createOffer();
      await pc.setLocalDescription(offer);
      chat.sendRaw({ type: 'voice-offer', user: this.userName, target: id, sdp: offer });
    }
    return pc;
  }

  async join(user: string, peersList: RemotePeer[]) {
    if (this.userName) return;
    this.userName = user;
    chat.on('voice-join', (m) => this.handleJoin(m, peersList));
    chat.on('voice-offer', (m) => this.handleOffer(m, peersList));
    chat.on('voice-answer', (m) => this.handleAnswer(m));
    chat.on('voice-candidate', (m) => this.handleCandidate(m));
    chat.on('voice-leave', (m) => this.handleLeave(m, peersList));
    const device = get(inputDeviceId);
    const constraints: MediaStreamConstraints =
      device ? { audio: { deviceId: { exact: device } } } : { audio: true };
    this.localStream = await navigator.mediaDevices.getUserMedia(constraints);
    chat.sendRaw({ type: 'voice-join', user });
  }

  leave(peersList: RemotePeer[]) {
    if (!this.userName) return;
    chat.sendRaw({ type: 'voice-leave', user: this.userName });
    for (const id of Object.keys(this.peers)) {
      this.cleanupPeer(id, peersList);
    }
    if (this.localStream) {
      for (const t of this.localStream.getTracks()) t.stop();
      this.localStream = null;
    }
    chat.off('voice-join');
    chat.off('voice-offer');
    chat.off('voice-answer');
    chat.off('voice-candidate');
    chat.off('voice-leave');
    this.userName = null;
    peersList.length = 0;
    this.emit([]);
  }

  private handleJoin(msg: Message, peersList: RemotePeer[]) {
    if (!this.userName || msg.user === this.userName) return;
    this.createPeer(msg.user as string, true, peersList);
    try {
      this.joinSound.currentTime = 0;
      this.joinSound.play();
    } catch {}
  }

  private async handleOffer(msg: Message, peersList: RemotePeer[]) {
    if (!this.userName || msg.target !== this.userName) return;
    const pc = await this.createPeer(msg.user as string, false, peersList);
    await pc.setRemoteDescription(new RTCSessionDescription(msg.sdp as any));
    const answer = await pc.createAnswer();
    await pc.setLocalDescription(answer);
    chat.sendRaw({ type: 'voice-answer', user: this.userName, target: msg.user, sdp: answer });
  }

  private async handleAnswer(msg: Message) {
    if (!this.userName || msg.target !== this.userName) return;
    const pc = this.peers[msg.user as string];
    if (pc && !pc.currentRemoteDescription) {
      await pc.setRemoteDescription(new RTCSessionDescription(msg.sdp as any));
    }
  }

  private async handleCandidate(msg: Message) {
    if (!this.userName || msg.target !== this.userName) return;
    const pc = this.peers[msg.user as string];
    if (pc) {
      try {
        await pc.addIceCandidate(msg.candidate as any);
      } catch {}
    }
  }

  private handleLeave(msg: Message, peersList: RemotePeer[]) {
    if (!this.userName) return;
    this.cleanupPeer(msg.user as string, peersList);
    try {
      this.leaveSound.currentTime = 0;
      this.leaveSound.play();
    } catch {}
  }
}
