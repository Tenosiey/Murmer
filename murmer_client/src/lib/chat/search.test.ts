import { describe, it, expect } from 'vitest';
import { parseWikiSearchHits } from './search';

describe('parseWikiSearchHits', () => {
  it('returns an empty list for anything that is not an array', () => {
    for (const raw of [undefined, null, {}, 'pages', 3]) {
      expect(parseWikiSearchHits(raw)).toEqual([]);
    }
  });

  it('keeps well-formed hits and drops the rest', () => {
    const hits = parseWikiSearchHits([
      {
        slug: 'onboarding',
        title: 'Onboarding guide',
        snippet: 'Ask in the lobby',
        updatedBy: 'alice',
        updatedAt: '2026-01-01T00:00:00Z'
      },
      { slug: 'no-title', title: '   ' },
      { title: 'No slug' },
      null,
      'not a page'
    ]);
    expect(hits).toEqual([
      {
        slug: 'onboarding',
        title: 'Onboarding guide',
        snippet: 'Ask in the lobby',
        updatedBy: 'alice',
        updatedAt: '2026-01-01T00:00:00Z'
      }
    ]);
  });

  it('fills in missing metadata instead of rendering undefined', () => {
    const [hit] = parseWikiSearchHits([{ slug: 'rules', title: 'House rules' }]);
    expect(hit).toEqual({
      slug: 'rules',
      title: 'House rules',
      snippet: '',
      updatedBy: '',
      updatedAt: ''
    });
  });

  it('collapses a multi-line excerpt to one bounded line', () => {
    const [hit] = parseWikiSearchHits([
      { slug: 'notes', title: 'Notes', snippet: `  # Heading\n\n  body   text  ` }
    ]);
    expect(hit.snippet).toBe('# Heading body text');

    const [long] = parseWikiSearchHits([
      { slug: 'notes', title: 'Notes', snippet: 'x'.repeat(400) }
    ]);
    expect(long.snippet).toHaveLength(160);
    expect(long.snippet.endsWith('…')).toBe(true);
  });
});
