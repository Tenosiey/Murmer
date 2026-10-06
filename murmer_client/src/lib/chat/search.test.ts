import { describe, it, expect } from 'vitest';
import { parseSearchQuery, parseWikiSearchHits } from './search';

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

describe('parseSearchQuery', () => {
  const channels = [
    { id: 1, name: 'general' },
    { id: 2, name: 'Off-Topic' }
  ];

  it('separates words from filters', () => {
    expect(parseSearchQuery('deploy from:@alice in:#off-topic has:file notes', channels)).toEqual({
      text: 'deploy notes',
      channelId: 2,
      filters: { from: 'alice', hasFile: true }
    });
  });

  it('leaves plain words and the current channel alone', () => {
    expect(parseSearchQuery('  hello   world ', channels)).toEqual({
      text: 'hello world',
      channelId: null,
      filters: {}
    });
  });

  it('turns dates into local day bounds that exclude the named day', () => {
    const parsed = parseSearchQuery('after:2026-01-30 before:2026-02-01', channels);
    expect(parsed).toEqual({
      text: '',
      channelId: null,
      filters: {
        after: new Date(2026, 0, 31).toISOString(),
        before: new Date(2026, 1, 1).toISOString()
      }
    });
  });

  it('refuses filters it cannot honour', () => {
    for (const raw of [
      'in:#nowhere',
      'has:link',
      'before:yesterday',
      'after:2026-02-31',
      'from:'
    ]) {
      expect(parseSearchQuery(raw, channels)).toHaveProperty('error');
    }
  });
});
