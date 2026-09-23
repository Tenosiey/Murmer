/**
 * The Svelte action behind every remote peer's `<audio>` element: plays the
 * stream at the member's own volume, on the chosen output device, and drives
 * that member's speaking indicator.
 */
import { outputDeviceId, outputMuted, userVolumes, volume } from '../stores/settings';
import { setSpeaking, SPEAKING_RMS_THRESHOLD } from '../stores/voiceSpeaking';
import { getAudioContext, resumeAudioContext } from './audioContext';
import { subscribeTick } from './ticker';

export interface RemoteAudio {
  stream: MediaStream;
  userId: string;
}

export function remoteAudio(node: HTMLAudioElement, data: RemoteAudio) {
  let currentUserId = data.userId;

  let analyser: AnalyserNode | null = null;
  let sourceNode: MediaStreamAudioSourceNode | null = null;
  let gainNode: GainNode | null = null;
  let stopTicks: (() => void) | null = null;
  let buffer: Uint8Array<ArrayBuffer> | null = null;

  // The per-user volume is applied by a gain node rather than the audio
  // element, because `HTMLMediaElement.volume` is clamped to 1 and quiet
  // members need to be boosted beyond 100%. The element still carries the
  // global volume and the output mute, and keeps `setSinkId` working.
  // If the graph could not be built we fall back to the element alone,
  // where any boost is clamped back down to 100%.
  let globalVolume = 1;
  let muted = false;
  let perUser: Record<string, number> = {};

  const updateVolume = () => {
    const userVol = perUser[currentUserId] ?? 1.0;
    if (muted) {
      node.volume = 0;
    } else {
      // Anything outside [0, 1] throws on assignment, so clamp rather than
      // trusting the stored values.
      const elementVol = globalVolume * (gainNode ? 1 : Math.min(userVol, 1));
      node.volume = Math.max(0, Math.min(1, elementVol));
    }
    if (gainNode) gainNode.gain.value = userVol;
  };

  // Svelte stores call a new subscriber straight away, so these also apply
  // the starting values.
  const unsubVol = volume.subscribe((value) => {
    globalVolume = value;
    updateVolume();
  });
  const unsubMute = outputMuted.subscribe((value) => {
    muted = value;
    updateVolume();
  });
  const unsubUserVol = userVolumes.subscribe((value) => {
    perUser = value;
    updateVolume();
  });

  const applySink = async (id: string | null) => {
    if ('setSinkId' in node) {
      try {
        await node.setSinkId(id || '');
      } catch (e) {
        console.error('Failed to set output device', e);
      }
    }
  };
  // Subscribing applies the current device straight away.
  const unsubOut = outputDeviceId.subscribe((id) => {
    applySink(id);
  });

  const disconnectNode = (audioNode: AudioNode | null, label: string) => {
    if (!audioNode) return;
    try {
      audioNode.disconnect();
    } catch (err) {
      if (import.meta.env.DEV) console.warn(`Failed to disconnect ${label}`, err);
    }
  };

  const teardownAudio = () => {
    if (stopTicks) {
      stopTicks();
      stopTicks = null;
    }
    disconnectNode(sourceNode, 'source node');
    sourceNode = null;
    disconnectNode(analyser, 'analyser');
    analyser = null;
    disconnectNode(gainNode, 'gain node');
    gainNode = null;
    buffer = null;
    setSpeaking(currentUserId, false);
  };

  const setupAudio = (stream: MediaStream | null | undefined) => {
    teardownAudio();
    node.srcObject = stream ?? null;
    if (!stream) return;

    try {
      // The context is shared with every other graph in the app: browsers
      // cap concurrent contexts at a handful, and one per peer used to
      // exhaust that budget in a busy channel — after which this whole
      // block threw and the per-user boost silently stopped working.
      const audioContext = getAudioContext();
      if (!audioContext) throw new Error('no audio context');
      resumeAudioContext();

      sourceNode = audioContext.createMediaStreamSource(stream);
      analyser = audioContext.createAnalyser();
      analyser.fftSize = 512;
      buffer = new Uint8Array(new ArrayBuffer(analyser.fftSize)) as Uint8Array<ArrayBuffer>;

      sourceNode.connect(analyser);

      // source -> gain -> destination stream, which the element then plays.
      // Playing the gained stream back through the element (instead of
      // sending it to the context destination) keeps `setSinkId` output
      // device selection and the global volume/mute working as before.
      gainNode = audioContext.createGain();
      const destination = audioContext.createMediaStreamDestination();
      sourceNode.connect(gainNode);
      gainNode.connect(destination);
      node.srcObject = destination.stream;
      updateVolume();

      stopTicks = subscribeTick(() => {
        if (!analyser || !buffer) return;
        analyser.getByteTimeDomainData(buffer);
        let sum = 0;
        for (let i = 0; i < buffer.length; i++) {
          const value = (buffer[i] - 128) / 128;
          sum += value * value;
        }
        const rms = Math.sqrt(sum / buffer.length);
        const speaking = rms > SPEAKING_RMS_THRESHOLD;
        setSpeaking(currentUserId, speaking);
      });
    } catch (error) {
      if (import.meta.env.DEV) {
        console.warn('Failed to build the remote audio graph', error);
      }
      // Never let a broken graph silence a peer: drop it and play the
      // stream straight from the element (no boost beyond 100%).
      teardownAudio();
      node.srcObject = stream;
      updateVolume();
    }
  };

  setupAudio(data.stream);

  return {
    update(newData: RemoteAudio) {
      if (currentUserId !== newData.userId) {
        setSpeaking(currentUserId, false);
        currentUserId = newData.userId;
      }
      setupAudio(newData.stream);
      updateVolume();
    },
    destroy() {
      unsubVol();
      unsubMute();
      unsubUserVol();
      unsubOut();
      teardownAudio();
    }
  };
}
