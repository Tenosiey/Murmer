import { writable } from 'svelte/store';
import type { Message } from './chat';
import { chat } from './chat';

export interface RemotePeer {
  id: string;
  stream: MediaStream;
}

function createVoiceStore() {
  const { subscribe, update, set } = writable<RemotePeer[]>([]);
  const peers: Record<string, RTCPeerConnection> = {};
  let localStream: MediaStream | null = null;
  let userName: string | null = null;

  const config: RTCConfiguration = {
    iceServers: [{ urls: 'stun:stun.l.google.com:19302' }]
  };

  function cleanupPeer(id: string) {
    const pc = peers[id];
    if (pc) {
      pc.close();
      delete peers[id];
    }
    update((p) => p.filter((r) => r.id !== id));
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
  }

  return { subscribe, join, leave };
}

export const voice = createVoiceStore();
