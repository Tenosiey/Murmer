import { describe, expect, it } from 'vitest';
import { DEFAULT_ACCENT, parseThemeCode, themeCode } from './theme';

/**
 * A theme code is pasted in from somebody else, so it is untrusted text: a
 * typo must be refused rather than repaint the app, and a code must come back
 * as the theme it was copied from.
 */
describe('theme codes', () => {
  it('round-trips the mode and the accent', () => {
    const code = themeCode('light', { hue: 215, saturation: 78 });
    expect(code).toMatch(/^light #[0-9a-f]{6}$/);
    const parsed = parseThemeCode(code);
    expect(parsed?.mode).toBe('light');
    expect(parsed?.accent.hue).toBeCloseTo(215, -1);
    expect(parsed?.accent.saturation).toBeCloseTo(78, -1);
  });

  it('renders the built-in palette when no accent is set', () => {
    expect(parseThemeCode(themeCode('dark', null))?.accent).toEqual(DEFAULT_ACCENT);
  });

  it('accepts a bare hex code and leaves the mode alone', () => {
    expect(parseThemeCode(' #27c0e8 ')?.mode).toBeNull();
    expect(parseThemeCode('DARK 27c0e8')?.mode).toBe('dark');
  });

  it('refuses anything that is not a theme', () => {
    for (const input of ['', 'dark', 'sepia #27c0e8', 'dark #27c0e8 extra', '#12345']) {
      expect(parseThemeCode(input)).toBeNull();
    }
  });
});
