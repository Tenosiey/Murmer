import { writable, derived } from 'svelte/store';
import type { Message } from './chat';
import { chat } from './chat';

export interface RemotePeer {
  id: string;
  stream: MediaStream;
  stats?: ConnectionStats;
}

export interface ConnectionStats {
  rtt: number;
  jitter: number;
  strength: number; // 1-5 bars
}

function createVoiceStore() {
  const { subscribe, update, set } = writable<RemotePeer[]>([]);
  const peers: Record<string, RTCPeerConnection> = {};
  const statsIntervals: Record<string, number> = {};
  let localStream: MediaStream | null = null;
  let userName: string | null = null;

  // sounds for join/leave events
  const joinSound = new Audio('/sounds/join.mp3');
  const leaveSound = new Audio('/sounds/leave.mp3');

  const config: RTCConfiguration = {
    iceServers: [{ urls: 'stun:stun.l.google.com:19302' }]
  };

  function cleanupPeer(id: string) {
    const pc = peers[id];
    if (pc) {
      pc.close();
      delete peers[id];
      const interval = statsIntervals[id];
      if (interval) {
        clearInterval(interval);
        delete statsIntervals[id];
      }
    }
    update((p) => p.filter((r) => r.id !== id));
  }

  async function updateStats(id: string) {
    const pc = peers[id];
    if (!pc) return;
    try {
      const reports = await pc.getStats();
      let rtt = 0;
      let jitter = 0;
      reports.forEach((report) => {
        // RTT from candidate pair
        if (
          report.type === 'candidate-pair' &&
          (report as any).state === 'succeeded' &&
          (report as any).currentRoundTripTime != null
        ) {
          rtt = (report as any).currentRoundTripTime * 1000;
        }
        // Jitter from remote inbound audio
        if (
          report.type === 'remote-inbound-rtp' &&
          (report as any).kind === 'audio' &&
          (report as any).jitter != null
        ) {
          jitter = (report as any).jitter * 1000;
        }
      });
      const strength =
        rtt === 0
          ? 5
          : rtt < 50
          ? 5
          : rtt < 100
          ? 4
          : rtt < 200
          ? 3
          : rtt < 400
          ? 2
          : 1;
      update((list) => {
        const peer = list.find((p) => p.id === id);
        if (peer) {
          peer.stats = { rtt, jitter, strength };
          return [...list];
        }
        return list;
      });
    } catch {
      // ignore stats errors
    }
  }

  async function createPeer(id: string, initiator: boolean) {
    if (peers[id]) return peers[id];
    const pc = new RTCPeerConnection(config);
    peers[id] = pc;
    if (localStream) {
      for (const track of localStream.getTracks()) {
        pc.addTrack(track, localStream);
      }
    }
    pc.ontrack = (ev) => {
      const stream = ev.streams[0];
      update((list) => {
        const existing = list.find((r) => r.id === id);
        if (existing) {
          existing.stream = stream;
          return [...list];
        }
        return [...list, { id, stream }];
      });
    };
    pc.onicecandidate = (ev) => {
      if (ev.candidate && userName) {
        chat.sendRaw({
          type: 'voice-candidate',
          user: userName,
          target: id,
          candidate: ev.candidate
        });
      }
    };
    pc.onconnectionstatechange = () => {
      if (
        pc.connectionState === 'disconnected' ||
        pc.connectionState === 'closed'
      ) {
        cleanupPeer(id);
      }
    };
    statsIntervals[id] = window.setInterval(() => updateStats(id), 1000);
    if (initiator && userName) {
      const offer = await pc.createOffer();
      await pc.setLocalDescription(offer);
      chat.sendRaw({
        type: 'voice-offer',
        user: userName,
        target: id,
        sdp: offer
      });
    }
    return pc;
  }

  async function join(user: string) {
    if (userName) return;
    userName = user;
    chat.on('voice-join', handleJoin);
    chat.on('voice-offer', handleOffer);
    chat.on('voice-answer', handleAnswer);
    chat.on('voice-candidate', handleCandidate);
    chat.on('voice-leave', handleLeave);
    localStream = await navigator.mediaDevices.getUserMedia({ audio: true });
    chat.sendRaw({ type: 'voice-join', user });
  }

  function leave() {
    if (!userName) return;
    chat.sendRaw({ type: 'voice-leave', user: userName });
    for (const id of Object.keys(peers)) {
      cleanupPeer(id);
    }
    if (localStream) {
      for (const t of localStream.getTracks()) t.stop();
      localStream = null;
    }
    chat.off('voice-join');
    chat.off('voice-offer');
    chat.off('voice-answer');
    chat.off('voice-candidate');
    chat.off('voice-leave');
    userName = null;
    set([]);
  }

  function handleJoin(msg: Message) {
    if (!userName || msg.user === userName) return;
    createPeer(msg.user as string, true);
    try {
      joinSound.currentTime = 0;
      joinSound.play();
    } catch {
      // ignore play errors
    }
  }

  async function handleOffer(msg: Message) {
    if (!userName || msg.target !== userName) return;
    const pc = await createPeer(msg.user as string, false);
    await pc.setRemoteDescription(new RTCSessionDescription(msg.sdp as any));
    const answer = await pc.createAnswer();
    await pc.setLocalDescription(answer);
    chat.sendRaw({
      type: 'voice-answer',
      user: userName,
      target: msg.user,
      sdp: answer
    });
  }

  async function handleAnswer(msg: Message) {
    if (!userName || msg.target !== userName) return;
    const pc = peers[msg.user as string];
    if (pc && !pc.currentRemoteDescription) {
      await pc.setRemoteDescription(
        new RTCSessionDescription(msg.sdp as any)
      );
    }
  }

  async function handleCandidate(msg: Message) {
    if (!userName || msg.target !== userName) return;
    const pc = peers[msg.user as string];
    if (pc) {
      try {
        await pc.addIceCandidate(msg.candidate as any);
      } catch {}
    }
  }

  function handleLeave(msg: Message) {
    if (!userName) return;
    cleanupPeer(msg.user as string);
    try {
      leaveSound.currentTime = 0;
      leaveSound.play();
    } catch {
      // ignore play errors
    }
  }

  return { subscribe, join, leave };
}

export const voice = createVoiceStore();

export const voiceStats = derived(voice, ($voice) => {
  const map: Record<string, ConnectionStats> = {};
  for (const p of $voice) {
    if (p.stats) map[p.id] = p.stats;
  }
  return map;
});
