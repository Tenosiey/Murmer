import type { WikiSearchHit } from '../types';

/** Longest snippet rendered in the result list; the server already excerpts. */
const MAX_SNIPPET_CHARS = 160;

/**
 * Parse the `pages` array of a `search-results` frame.
 *
 * Wiki hits arrive on the same frame as the message hits, so a malformed or
 * missing entry must cost only that entry: anything without a usable slug and
 * title is dropped rather than rendered as a blank row. Snippets are collapsed
 * to single-line plain text — they are raw Markdown from the page body and are
 * shown as text, never rendered.
 */
export function parseWikiSearchHits(raw: unknown): WikiSearchHit[] {
  if (!Array.isArray(raw)) return [];
  const hits: WikiSearchHit[] = [];
  for (const entry of raw) {
    if (!entry || typeof entry !== 'object') continue;
    const page = entry as Record<string, unknown>;
    const slug = typeof page.slug === 'string' ? page.slug : '';
    const title = typeof page.title === 'string' ? page.title.trim() : '';
    if (!slug || !title) continue;
    hits.push({
      slug,
      title,
      snippet: normalizeSnippet(page.snippet),
      updatedBy: typeof page.updatedBy === 'string' ? page.updatedBy : '',
      updatedAt: typeof page.updatedAt === 'string' ? page.updatedAt : ''
    });
  }
  return hits;
}

/** Collapse a page excerpt to one bounded line of text. */
function normalizeSnippet(raw: unknown): string {
  if (typeof raw !== 'string') return '';
  const collapsed = raw.replace(/\s+/g, ' ').trim();
  return collapsed.length > MAX_SNIPPET_CHARS
    ? `${collapsed.slice(0, MAX_SNIPPET_CHARS - 1)}…`
    : collapsed;
}
