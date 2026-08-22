// @vitest-environment jsdom

/**
 * `emojifyHtml` rewrites *already sanitized* markdown HTML and its result goes
 * back into `{@html}`, so it is the second half of the same security boundary
 * `markdown.test.ts` covers. Two things must hold no matter what a server
 * sends: a custom emoji's URL can never escape the `src` attribute it is
 * written into, and re-serializing the walked tree must not decode any
 * escaping DOMPurify put there.
 *
 * jsdom, not happy-dom, for the same reason `markdown.test.ts` says so — this
 * suite asserts on serialized DOM output and needs the faithful one.
 */

import { describe, expect, it } from 'vitest';
import type { CustomEmoji } from './stores/customEmojis';
import { emojifyHtml, isEmojiOnlyText, unicodeFromShortcode } from './emoji';

function custom(...names: string[]): Record<string, CustomEmoji> {
  return Object.fromEntries(
    names.map((name) => [name, { name, url: `/files/${name}.png` }])
  );
}

/** Parse rendered HTML so assertions look at elements, not string spelling. */
function parse(html: string): HTMLElement {
  const host = document.createElement('div');
  host.innerHTML = html;
  return host;
}

describe('unicodeFromShortcode', () => {
  it('resolves a known shortcode', () => {
    expect(unicodeFromShortcode(':fire:')).toBe('🔥');
    expect(unicodeFromShortcode(':+1:')).toBe('👍');
    expect(unicodeFromShortcode(':100:')).toBe('💯');
  });

  it('returns null for anything it does not know or cannot parse', () => {
    expect(unicodeFromShortcode(':nope:')).toBeNull();
    expect(unicodeFromShortcode(':FIRE:')).toBeNull();
    expect(unicodeFromShortcode('fire')).toBeNull();
    expect(unicodeFromShortcode(':fire')).toBeNull();
    expect(unicodeFromShortcode(':a b:')).toBeNull();
    expect(unicodeFromShortcode('')).toBeNull();
  });
});

describe('isEmojiOnlyText', () => {
  const emojis = custom('partyparrot');

  it('accepts unicode emoji, shortcodes and custom emoji on their own', () => {
    expect(isEmojiOnlyText('🔥', emojis)).toBe(true);
    expect(isEmojiOnlyText('🔥 🎉  👍', emojis)).toBe(true);
    expect(isEmojiOnlyText(':fire::tada:', emojis)).toBe(true);
    expect(isEmojiOnlyText(':partyparrot:', emojis)).toBe(true);
    expect(isEmojiOnlyText(' :fire: 🎉 ', emojis)).toBe(true);
  });

  it('accepts multi-codepoint glyphs', () => {
    expect(isEmojiOnlyText('🇩🇪', emojis)).toBe(true); // regional indicators
    expect(isEmojiOnlyText('👍🏽', emojis)).toBe(true); // skin-tone modifier
    expect(isEmojiOnlyText('👩‍👩‍👧', emojis)).toBe(true); // ZWJ sequence
    expect(isEmojiOnlyText('❤️', emojis)).toBe(true); // variation selector
    // Keycaps are the exception: the base character is an ASCII digit, which
    // is not pictographic, so "1️⃣" reads as text and renders at normal size.
    expect(isEmojiOnlyText('1️⃣', emojis)).toBe(false);
  });

  it('rejects anything mixed with other text', () => {
    expect(isEmojiOnlyText('🔥 hot', emojis)).toBe(false);
    expect(isEmojiOnlyText('nice :fire:', emojis)).toBe(false);
    expect(isEmojiOnlyText(':unknown:', emojis)).toBe(false);
    expect(isEmojiOnlyText(':fire: :unknown:', emojis)).toBe(false);
    expect(isEmojiOnlyText('100', emojis)).toBe(false);
  });

  it('rejects text with no emoji at all', () => {
    expect(isEmojiOnlyText('', emojis)).toBe(false);
    expect(isEmojiOnlyText('   ', emojis)).toBe(false);
    expect(isEmojiOnlyText('hello', emojis)).toBe(false);
  });
});

describe('emojifyHtml', () => {
  it('replaces known shortcodes in text with unicode', () => {
    expect(emojifyHtml('<p>ship it :rocket:</p>', {}, 'https://host')).toBe(
      '<p>ship it 🚀</p>'
    );
  });

  it('renders a custom emoji as an inline image resolved against the server', () => {
    const html = emojifyHtml('<p>:partyparrot:</p>', custom('partyparrot'), 'https://host');
    const img = parse(html).querySelector('img')!;
    expect(img.getAttribute('src')).toBe('https://host/files/partyparrot.png');
    expect(img.getAttribute('alt')).toBe(':partyparrot:');
    expect(img.getAttribute('title')).toBe(':partyparrot:');
    expect(img.className).toBe('inline-emoji');
  });

  it('leaves unknown shortcodes untouched', () => {
    const html = '<p>ratio :nope: and :also_not_a_code:</p>';
    expect(emojifyHtml(html, {}, 'https://host')).toBe(html);
  });

  it('resumes scanning on the closing colon of an unknown code', () => {
    // ":unknown:fire:" must still light up — the colon that ended the unknown
    // code is the one that opens the known one.
    expect(emojifyHtml('<p>:unknown:fire:</p>', {}, 'https://host')).toBe('<p>:unknown🔥</p>');
  });

  it('keeps code spans and blocks literal', () => {
    expect(emojifyHtml('<p><code>:fire:</code></p>', {}, 'https://host')).toBe(
      '<p><code>:fire:</code></p>'
    );
    expect(emojifyHtml('<pre><code><span>:fire:</span></code></pre>', {}, 'https://host')).toBe(
      '<pre><code><span>:fire:</span></code></pre>'
    );
    // …while the text around them is still replaced.
    expect(emojifyHtml('<p><code>:fire:</code> :fire:</p>', {}, 'https://host')).toBe(
      '<p><code>:fire:</code> 🔥</p>'
    );
  });

  it('returns the input untouched when there is no colon to act on', () => {
    const html = '<p>nothing to do here</p>';
    expect(emojifyHtml(html, custom('partyparrot'), 'https://host')).toBe(html);
  });

  it('does not decode escaping that the sanitizer put there', () => {
    // The `&lt;` came out of DOMPurify; walking the tree and re-serializing it
    // must not hand `{@html}` a live <script> back.
    const html = '<p>&lt;script&gt;alert(1)&lt;/script&gt; :fire:</p>';
    const result = emojifyHtml(html, {}, 'https://host');
    expect(result).toBe('<p>&lt;script&gt;alert(1)&lt;/script&gt; 🔥</p>');
    expect(parse(result).querySelector('script')).toBeNull();
  });

  it('escapes markup that a shortcode replacement sits next to', () => {
    const result = emojifyHtml('<p>a &amp; b :fire: &lt;b&gt;</p>', {}, 'https://host');
    expect(result).toBe('<p>a &amp; b 🔥 &lt;b&gt;</p>');
    expect(parse(result).querySelector('b')).toBeNull();
  });

  it('cannot be made to break out of the emoji src attribute', () => {
    // The emoji URL comes from the server, which a client does not trust; it
    // is assigned as a property, so serialization escapes it either way.
    const hostile: Record<string, CustomEmoji> = {
      evil: { name: 'evil', url: '/x.png" onerror="alert(1)' }
    };
    const result = emojifyHtml('<p>:evil:</p>', hostile, 'https://host');
    const img = parse(result).querySelector('img')!;
    expect(img.getAttribute('onerror')).toBeNull();
    expect(img.getAttribute('src')).toBe('https://host/x.png" onerror="alert(1)');
    expect(result).not.toContain('onerror="');
  });

  it('cannot be made to break out through the server base either', () => {
    // The base is the selected server's URL, which the user typed.
    const result = emojifyHtml('<p>:parrot:</p>', custom('parrot'), 'https://host"><script>');
    const host = parse(result);
    expect(host.querySelector('script')).toBeNull();
    expect(host.querySelector('img')!.getAttribute('src')).toBe(
      'https://host"><script>/files/parrot.png'
    );
  });

  it('replaces several codes in one text node and keeps the text between them', () => {
    const result = emojifyHtml(
      '<p>:rocket: to the :star: and back :partyparrot:</p>',
      custom('partyparrot'),
      'https://host'
    );
    const host = parse(result);
    expect(host.textContent).toBe('🚀 to the ⭐ and back ');
    expect(host.querySelectorAll('img')).toHaveLength(1);
  });

  it('re-renders when the custom emoji map changes', () => {
    // Memoised per (map, base) pair: a newly uploaded emoji must not keep
    // showing as its literal shortcode because the old result was cached.
    const html = '<p>:newbie:</p>';
    expect(emojifyHtml(html, {}, 'https://host')).toBe(html);
    const after = emojifyHtml(html, custom('newbie'), 'https://host');
    expect(parse(after).querySelector('img')?.getAttribute('src')).toBe(
      'https://host/files/newbie.png'
    );
  });

  it('re-renders when the server base changes', () => {
    const html = '<p>:parrot:</p>';
    const emojis = custom('parrot');
    expect(parse(emojifyHtml(html, emojis, 'https://a')).querySelector('img')?.getAttribute('src')).toBe(
      'https://a/files/parrot.png'
    );
    expect(parse(emojifyHtml(html, emojis, 'https://b')).querySelector('img')?.getAttribute('src')).toBe(
      'https://b/files/parrot.png'
    );
  });

  it('returns the same result when called twice with the same inputs', () => {
    const emojis = custom('parrot');
    const html = '<p>:fire: :parrot:</p>';
    const first = emojifyHtml(html, emojis, 'https://host');
    expect(emojifyHtml(html, emojis, 'https://host')).toBe(first);
  });
});
