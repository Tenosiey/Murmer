import { describe, it, expect } from 'vitest';
import { ttsUtterance } from './tts';

describe('ttsUtterance', () => {
  it('names the speaker and never reads a spoiler out', () => {
    expect(ttsUtterance('Alice', ' the answer is ||42|| ')).toBe(
      'Alice says the answer is [spoiler]'
    );
  });
});
