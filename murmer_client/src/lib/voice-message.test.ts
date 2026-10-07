import { describe, expect, it } from 'vitest';
import { isPlayableAudio, pickRecordingFormat, voiceMessageName } from './voice-message';

describe('voice messages', () => {
  it('records the first format the platform supports', () => {
    expect(pickRecordingFormat((t) => t.startsWith('audio/webm'))?.extension).toBe('webm');
    expect(pickRecordingFormat((t) => t === 'audio/mp4')?.extension).toBe('m4a');
    expect(pickRecordingFormat(() => false)).toBeNull();
  });

  it('names a clip so the message list recognises it', () => {
    const name = voiceMessageName(new Date('2026-10-06T07:05:09.123Z'), 'webm');
    expect(name).toBe('voice-message-20261006-070509.webm');
    expect(isPlayableAudio(name)).toBe(true);
  });

  it('plays audio files inline but not other WebM attachments', () => {
    expect(isPlayableAudio('song.MP3')).toBe(true);
    expect(isPlayableAudio('clip.webm')).toBe(false);
    expect(isPlayableAudio('notes.txt')).toBe(false);
  });
});
