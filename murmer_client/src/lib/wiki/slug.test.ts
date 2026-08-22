/**
 * `slugify` is the only thing standing between free-form page titles and the
 * server's `validate_wiki_slug`, which rejects the whole save. The failure is
 * invisible until someone names a page "Release  2.0 — draft—" and the save
 * comes back `invalid-wiki-slug`, so the interesting assertions here are not
 * the pretty cases but that *every* output survives the server's rules.
 */
import { describe, expect, it } from 'vitest';
import { MAX_WIKI_SLUG_LENGTH, parseWikiTarget, slugify } from './slug';

/**
 * Re-implementation of `murmer_server/src/ws/validation.rs::validate_wiki_slug`
 * from its documented rules: non-empty, at most MAX_WIKI_SLUG_LENGTH bytes, no
 * leading/trailing/double dash, only ASCII lowercase, digits and dashes.
 * Written from the description rather than from `slugify` on purpose — the
 * point is to catch the two drifting apart.
 */
function validateWikiSlug(value: string): boolean {
  return (
    value.length > 0 &&
    new TextEncoder().encode(value).length <= MAX_WIKI_SLUG_LENGTH &&
    !value.startsWith('-') &&
    !value.endsWith('-') &&
    !value.includes('--') &&
    /^[a-z0-9-]*$/.test(value)
  );
}

describe('slugify', () => {
  it('lowercases and joins words with single dashes', () => {
    expect(slugify('Getting Started')).toBe('getting-started');
    expect(slugify('FAQ')).toBe('faq');
    expect(slugify('page-2')).toBe('page-2');
  });

  it('collapses runs of punctuation and whitespace into one dash', () => {
    expect(slugify('Release  2.0 — draft')).toBe('release-2-0-draft');
    expect(slugify('a___b')).toBe('a-b');
    expect(slugify('tabs\tand\nnewlines')).toBe('tabs-and-newlines');
  });

  it('strips leading and trailing dashes', () => {
    expect(slugify('  -- hello --  ')).toBe('hello');
    expect(slugify('!!!bang!!!')).toBe('bang');
  });

  it('drops non-ASCII characters rather than passing them to the server', () => {
    // The server's safe-list is ASCII-only, so anything else has to go.
    expect(slugify('Café')).toBe('caf');
    expect(slugify('日本語')).toBe('');
    expect(slugify('naïve approach')).toBe('na-ve-approach');
  });

  it('returns an empty slug when nothing usable is left', () => {
    for (const input of ['', '   ', '---', '!!!', '日本語']) {
      expect(slugify(input)).toBe('');
    }
  });

  it('truncates to the server limit without leaving a trailing dash', () => {
    const long = slugify('word '.repeat(40));
    expect(long.length).toBeLessThanOrEqual(MAX_WIKI_SLUG_LENGTH);
    expect(long.endsWith('-')).toBe(false);

    // A cut that lands exactly on a dash is the case the second trim exists
    // for: 64 characters of "ab-" repeated ends in "-" before it is trimmed.
    const onDash = slugify('ab '.repeat(30));
    expect(onDash.endsWith('-')).toBe(false);
    expect(onDash.length).toBeLessThanOrEqual(MAX_WIKI_SLUG_LENGTH);
  });

  it('produces slugs the server accepts, for anything a user might type', () => {
    const inputs = [
      'Getting Started',
      'Release  2.0 — draft—',
      '   ---leading and trailing---   ',
      'ALL CAPS SHOUTING',
      'a'.repeat(200),
      'word '.repeat(40),
      'ab '.repeat(30),
      'Ünïcödé Tïtlé',
      'emoji 🎉 party',
      'slashes/and\\backslashes',
      'quotes "and" \'apostrophes\'',
      '<script>alert(1)</script>',
      'trailing dash-',
      '-leading dash',
      'double--dash',
      '2026.729.0'
    ];
    for (const input of inputs) {
      const slug = slugify(input);
      if (slug === '') continue; // An empty slug is rejected before it is sent.
      expect(validateWikiSlug(slug), `slugify(${JSON.stringify(input)}) = ${slug}`).toBe(true);
    }
  });

  it('is idempotent — re-slugifying a slug changes nothing', () => {
    for (const input of ['Getting Started', 'Release  2.0 — draft', 'a'.repeat(200)]) {
      const slug = slugify(input);
      expect(slugify(slug)).toBe(slug);
    }
  });
});

describe('parseWikiTarget', () => {
  it('parses a same-channel link', () => {
    expect(parseWikiTarget('Getting Started')).toEqual({
      channel: null,
      slug: 'getting-started',
      label: 'Getting Started'
    });
  });

  it('parses a cross-channel link, slugifying only the page part', () => {
    // The channel is addressed by name, which is not a slug — lowercasing or
    // dashing it would point the link at a channel that does not exist.
    expect(parseWikiTarget('Team Docs/Getting Started')).toEqual({
      channel: 'Team Docs',
      slug: 'getting-started',
      label: 'Getting Started'
    });
  });

  it('treats only the first slash as the channel separator', () => {
    expect(parseWikiTarget('docs/nested/page')).toEqual({
      channel: 'docs',
      slug: 'nested-page',
      label: 'nested/page'
    });
  });

  it('uses an explicit label when given, falling back to the page text', () => {
    expect(parseWikiTarget('getting-started', 'Start here')?.label).toBe('Start here');
    expect(parseWikiTarget('getting-started', '  spaced  ')?.label).toBe('spaced');
    // An explicit label that is only whitespace falls back to the slug rather
    // than to the raw page text, so a link never renders as an empty span.
    expect(parseWikiTarget('Getting Started', '   ')?.label).toBe('getting-started');
    // No label at all keeps the text the author typed.
    expect(parseWikiTarget('Getting Started')?.label).toBe('Getting Started');
  });

  it('trims the target and its parts', () => {
    expect(parseWikiTarget('  docs / Getting Started  ')).toEqual({
      channel: 'docs',
      slug: 'getting-started',
      label: 'Getting Started'
    });
  });

  it('rejects targets that cannot address a page', () => {
    for (const target of ['', '   ', '/page', '  /page', 'docs/', 'docs/   ', '!!!', '日本語']) {
      expect(parseWikiTarget(target), target).toBeNull();
    }
  });

  it('produces a slug the server accepts', () => {
    const parsed = parseWikiTarget('docs/Release  2.0 — draft—');
    expect(parsed).not.toBeNull();
    expect(validateWikiSlug(parsed!.slug)).toBe(true);
  });
});
