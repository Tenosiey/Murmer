/**
 * Voice messages: a clip recorded in the composer and sent as an ordinary
 * attachment, so it travels sealed in DMs and encrypted channels exactly like
 * any other file. Nothing about it is special on the wire — what marks a clip
 * as a voice message is its file name, which is what lets the message list
 * give it an inline player.
 */
import { openMicrophone } from './voice/capture';

/** The longest clip one recording keeps before it stops on its own. */
export const VOICE_MESSAGE_MAX_MS = 5 * 60_000;

/** File names the recorder gives its clips. */
const VOICE_MESSAGE_NAME = /^voice-message-[\w-]+\.(webm|ogg|m4a)$/i;

/** Extensions an `<audio>` element plays inline. */
const AUDIO_EXTENSIONS = /\.(mp3|wav|ogg|opus|m4a|flac)$/i;

/**
 * Containers to record into, best first, with the extension the upload
 * safe-list knows them by. Chromium (WebView2 and the web client) only
 * records WebM, WebKit only MP4, Firefox prefers Ogg. WebM lands in the
 * safe-list's video category, which is why the recorder's name is what
 * marks it as audio rather than the extension.
 */
const FORMATS = [
  { mimeType: 'audio/ogg;codecs=opus', extension: 'ogg' },
  { mimeType: 'audio/webm;codecs=opus', extension: 'webm' },
  { mimeType: 'audio/mp4', extension: 'm4a' }
] as const;

export type RecordingFormat = (typeof FORMATS)[number];

/** The first format this platform can record, or null when none. */
export function pickRecordingFormat(
  isTypeSupported: (type: string) => boolean
): RecordingFormat | null {
  return FORMATS.find((format) => isTypeSupported(format.mimeType)) ?? null;
}

/** The file name for a clip recorded at `at`. */
export function voiceMessageName(at: Date, extension: string): string {
  const stamp = at.toISOString().replace(/\.\d+Z$/, '').replace(/[-:]/g, '').replace('T', '-');
  return `voice-message-${stamp}.${extension}`;
}

/** Whether an attachment should get an inline audio player. */
export function isPlayableAudio(name: string): boolean {
  return AUDIO_EXTENSIONS.test(name) || VOICE_MESSAGE_NAME.test(name);
}

/** A running recording; `stop` resolves to the finished clip. */
export interface VoiceRecording {
  stop(): Promise<File>;
  cancel(): void;
}

/**
 * Start recording the configured microphone. Rejects when the platform can
 * record no supported format or the microphone cannot be opened.
 */
export async function startVoiceRecording(): Promise<VoiceRecording> {
  const format =
    typeof MediaRecorder === 'undefined'
      ? null
      : pickRecordingFormat((type) => MediaRecorder.isTypeSupported(type));
  if (!format) throw new Error('This app cannot record audio here.');

  const stream = await openMicrophone();
  const recorder = new MediaRecorder(stream, { mimeType: format.mimeType });
  const chunks: Blob[] = [];
  recorder.ondataavailable = (event) => {
    if (event.data.size > 0) chunks.push(event.data);
  };
  const finished = new Promise<void>((resolve) => (recorder.onstop = () => resolve()));
  const release = () => stream.getTracks().forEach((track) => track.stop());
  // A forgotten recording must not keep the microphone open indefinitely.
  const limit = setTimeout(() => recorder.state !== 'inactive' && recorder.stop(), VOICE_MESSAGE_MAX_MS);
  recorder.start();

  return {
    async stop() {
      clearTimeout(limit);
      if (recorder.state !== 'inactive') recorder.stop();
      await finished;
      release();
      const type = format.mimeType.split(';')[0];
      return new File(chunks, voiceMessageName(new Date(), format.extension), { type });
    },
    cancel() {
      clearTimeout(limit);
      if (recorder.state !== 'inactive') recorder.stop();
      release();
    }
  };
}
