/*
 * Notifications are shown by the operating system, outside anything the app
 * can blur, so a spoiler that reaches one unhidden is simply spoiled.
 */
import { describe, expect, it } from 'vitest';
import { hideSpoilers } from './spoilers';

describe('hideSpoilers', () => {
  it('blanks every spoiler and keeps the rest', () => {
    expect(hideSpoilers('so ||Bruce|| was ||dead all along||!')).toBe(
      'so [spoiler] was [spoiler]!'
    );
  });

  it('leaves bars that the renderer would not hide', () => {
    for (const text of ['a || b', '|| padded ||', '||||']) {
      expect(hideSpoilers(text)).toBe(text);
    }
  });
});
